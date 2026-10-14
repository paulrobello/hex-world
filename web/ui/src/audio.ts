// The thin Web Audio layer (PRD §8, D13): game events from the bridge fire the manifest's cues,
// per-tile ambience loops follow the camera focus mix, a global bed follows the biome mix in view,
// and the music playlist crossfades by mood. Nothing is fetched before the first pointer or key
// press in the play scenario: the AudioContext needs that gesture, and the initial download stays
// as it was. `hexWorld.audio.state()` reports what is playing, so tests can verify it without listening.
import {
  type Bus,
  type Cue,
  type Format,
  type Manifest,
  busOf,
  eventCues,
  pickFormat,
  planDecode,
  playable,
} from "./audio-cues.ts";
import {
  type Biome,
  BED_CUE,
  BEE_CUE,
  BEE_LEVEL,
  type BeeState,
  type FlowerTile,
  beeGate,
  beeIdle,
  beeStep,
  isFlowerTile,
  pickBeeTile,
  type Candidate,
  EMITTER_CUE,
  type Mix,
  type MusicState,
  type PlacedTile,
  type View,
  bedGains,
  biomeMix,
  biomeOf,
  cellWorld,
  emitterGain,
  emptyMix,
  musicGains,
  musicIdle,
  musicStep,
  pickEmitters,
} from "./audio-mix.ts";
import type { GameEvent, PlayState, State } from "./bridge.ts";
import schema from "./bridge-schema.json" with { type: "json" };

const BASE = "assets/audio/";
const STORE_KEY = "hexWorld.audio";
/** cues.json `sampleRate`: the context runs at the mastering rate, so loop points stay sample-exact. */
const SAMPLE_RATE = 48_000;
const UPDATE_MS = 100;
/** Smoothing time constant for gain moves, seconds. */
const GLIDE = 0.12;
const VOICES_PER_CUE = 3;
const SAME_CUE_GAP_S = 0.05;
const MAX_INSTANCES = 4;
/** The thunk waits for the tile to settle (D15); this long without a settle, it plays anyway. */
const SETTLE_TIMEOUT_S = 4;
/** One-shots that wait for a later `riseStage` at their cell instead of playing on the event. */
const WAIT_FOR_STAGE: Record<string, string> = { "place-thunk": "settled" };
const RADIUS: number = schema.catalog.hex_radius;
const BIOME_OF = new Map<string, Biome>(schema.catalog.tiles.map((t) => [t.id, biomeOf(t.id, t.edges)]));
/** Tile defs whose scatter carries wildflowers: where the occasional bee swarm may play. */
const FLOWER_DEFS = new Set(
  schema.catalog.tiles.filter((t) => isFlowerTile(biomeOf(t.id, t.edges), t.scatter)).map((t) => t.id),
);

export interface Volumes {
  master: number;
  music: number;
  ambience: number;
  effects: number;
  interface: number;
}
type VolumeKey = keyof Volumes;

/** The Legend Settings → Audio defaults (design/ui/legend/settings.html). */
const DEFAULTS: Volumes = { master: 0.8, music: 0.55, ambience: 0.7, effects: 0.75, interface: 0.5 };

interface Settings {
  muted: boolean;
  volumes: Volumes;
}

function loadSettings(): Settings {
  const s: Settings = { muted: false, volumes: { ...DEFAULTS } };
  try {
    const raw = JSON.parse(localStorage.getItem(STORE_KEY) ?? "null") as Partial<Settings> | null;
    if (raw && typeof raw === "object") {
      s.muted = raw.muted === true;
      for (const k of Object.keys(DEFAULTS) as VolumeKey[]) {
        const v = raw.volumes?.[k];
        if (typeof v === "number" && v >= 0 && v <= 1) s.volumes[k] = v;
      }
    }
  } catch {
    /* No storage (private window, blocked site data): the defaults hold for this visit. */
  }
  return s;
}

function saveSettings(s: Settings): void {
  try {
    localStorage.setItem(STORE_KEY, JSON.stringify(s));
  } catch {
    /* Storage refused: the setting holds until the page closes. */
  }
}

interface Fired {
  id: string;
  event: string;
  t: number;
}

interface Voice {
  cue: string;
  src: AudioBufferSourceNode;
  gain: GainNode;
  pan: PannerNode | null;
  q: number;
  r: number;
  level: number;
}

interface Track {
  el: HTMLAudioElement;
  gain: GainNode;
  level: number;
}

interface Pending {
  id: string;
  event: string;
  q: number;
  r: number;
  deadline: number;
}

export interface AudioDebug {
  context: AudioContextState | "none";
  sampleRate: number | null;
  currentTime: number;
  format: Format | null;
  loading: "idle" | "manifest" | "cues" | "done" | "failed";
  error: string | null;
  muted: boolean;
  volumes: Volumes;
  masterGain: number;
  cues: {
    loaded: string[];
    /** Decoded length in sample frames per loaded cue: a loop must decode to its manifest `endSample`. */
    frames: Record<string, number>;
    streams: string[];
    failed: { id: string; reason: string }[];
    skipped: { id: string; reason: string }[];
  };
  decodedBytes: number;
  decodedBudgetBytes: number | null;
  eventCues: Record<string, string[]>;
  emitters: { cue: string; q: number; r: number; gain: number; voiced: boolean }[];
  missingEmitters: string[];
  bed: { cue: string; gain: number }[];
  mix: Mix;
  view: { focus: [number, number, number]; zoom: number } | null;
  music: { tracks: number; current: string | null; previous: string | null; gains: { id: string; gain: number }[] };
  lastCue: Fired | null;
  fired: number;
  recent: Fired[];
  outputRms: number;
  /** The occasional meadow-bees one-shot: schedule, last play and why the last due swarm was skipped. */
  bees: BeeState & { cue: string; flowerTiles: number };
}

export interface AudioApi {
  state(): AudioDebug;
  setMuted(on: boolean): void;
  /** Sets a bus level (0–1); master when `bus` is omitted. */
  setVolume(v: number, bus?: VolumeKey): void;
}

const round = (v: number, n = 4): number => Math.round(v * 10 ** n) / 10 ** n;
const key = (q: number, r: number): string => `${q},${r}`;

export class GameAudio {
  private ctx: AudioContext | null = null;
  private master: GainNode | null = null;
  private analyser: AnalyserNode | null = null;
  private buses = new Map<Bus, GainNode>();
  private settings = loadSettings();
  private active = false;
  private loading: AudioDebug["loading"] = "idle";
  private error: string | null = null;
  private format: Format | null = null;
  private manifest: Manifest | null = null;
  private cues = new Map<string, Cue>();
  private buffers = new Map<string, AudioBuffer>();
  private streams: Cue[] = [];
  private failed: { id: string; reason: string }[] = [];
  private skipped: { id: string; reason: string }[] = [];
  private decoded = 0;
  private byEvent = new Map<string, string[]>();
  private emitters = new Map<string, Voice>();
  private candidates: Candidate[] = [];
  private bees: BeeState = beeIdle();
  private flowers: FlowerTile[] = [];
  private beds = new Map<string, Voice>();
  private bedLevels = new Map<string, number>();
  private tracks = new Map<string, Track>();
  private music: MusicState = musicIdle();
  private mix: Mix = emptyMix();
  private view: View | null = null;
  private instances = new Map<string, AudioBufferSourceNode[]>();
  private pending = new Map<string, Pending>();
  private recent: Fired[] = [];
  private fired = 0;
  private lastAt = 0;

  /** Unlocks on the first pointer or key press. Install before the HUD's own capture listeners. */
  listen(): void {
    const unlock = () => this.unlock();
    addEventListener("pointerdown", unlock, { capture: true, passive: true });
    addEventListener("keydown", unlock, { capture: true, passive: true });
  }

  api(): AudioApi {
    return {
      state: () => this.debug(),
      setMuted: (on) => this.setMuted(on),
      setVolume: (v, bus) => this.setVolume(v, bus),
    };
  }

  // The context is created and resumed synchronously inside the gesture (Safari needs that).
  private unlock(): void {
    if (!this.active) return;
    if (!this.ctx) {
      try {
        this.ctx = new AudioContext({ sampleRate: SAMPLE_RATE, latencyHint: "interactive" });
      } catch {
        try {
          this.ctx = new AudioContext();
        } catch {
          this.error = "Web Audio is unavailable";
          this.active = false;
          return;
        }
      }
      this.graph(this.ctx);
    }
    if (this.ctx.state !== "running") this.ctx.resume().catch(() => {});
    if (this.loading === "idle") void this.load(this.ctx);
  }

  private graph(ctx: AudioContext): void {
    this.master = ctx.createGain();
    this.master.gain.value = this.settings.muted ? 0 : this.settings.volumes.master;
    // A safety limiter on the sum; loudness comes from mastering and the bus levels, not from here.
    const limiter = ctx.createDynamicsCompressor();
    limiter.threshold.value = -3;
    limiter.knee.value = 3;
    limiter.ratio.value = 20;
    limiter.attack.value = 0.002;
    limiter.release.value = 0.2;
    this.analyser = ctx.createAnalyser();
    this.analyser.fftSize = 2048;
    this.master.connect(limiter).connect(this.analyser).connect(ctx.destination);
    for (const b of ["music", "ambience", "effects", "interface"] as const) {
      const g = ctx.createGain();
      g.gain.value = this.settings.volumes[b];
      g.connect(this.master);
      this.buses.set(b, g);
    }
  }

  private async load(ctx: AudioContext): Promise<void> {
    this.loading = "manifest";
    try {
      const res = await fetch(`${BASE}manifest.json`);
      if (!res.ok) throw new Error(`manifest HTTP ${res.status}`);
      const m = (await res.json()) as Manifest;
      this.manifest = m;
      const probe = document.createElement("audio");
      this.format = pickFormat(m.formats, (mime) => probe.canPlayType(mime));
      if (!this.format) throw new Error("no playable audio format");
      const cues = playable(m.cues, this.format);
      for (const c of cues) this.cues.set(c.id, c);
      this.byEvent = eventCues(cues);
      this.streams = cues.filter((c) => c.playback === "stream");
      const plan = planDecode(cues, m.decodedBudgetBytes, m.sampleRate, ctx.sampleRate);
      this.skipped = plan.skipped;
      this.loading = "cues";
      // Two at a time, in plan order: the interface and placement one-shots land first.
      const queue = [...plan.load];
      const worker = async () => {
        for (let c = queue.shift(); c; c = queue.shift()) await this.decode(ctx, c);
      };
      await Promise.all([worker(), worker()]);
      this.loading = "done";
    } catch (e) {
      this.loading = "failed";
      this.error = String((e as Error)?.message ?? e);
    }
  }

  private async decode(ctx: AudioContext, c: Cue): Promise<void> {
    const file = this.format && c.files?.[this.format];
    if (!file) return;
    try {
      const res = await fetch(BASE + file.path);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const buf = await ctx.decodeAudioData(await res.arrayBuffer());
      const bytes = buf.length * buf.numberOfChannels * 4;
      const budget = this.manifest?.decodedBudgetBytes ?? Number.POSITIVE_INFINITY;
      if (this.decoded + bytes > budget) {
        this.skipped.push({ id: c.id, reason: `decoded budget: ${this.decoded + bytes} B > ${budget} B` });
        return;
      }
      this.decoded += bytes;
      this.buffers.set(c.id, buf);
    } catch (e) {
      this.failed.push({ id: c.id, reason: String((e as Error)?.message ?? e) });
    }
  }

  // ---------- Controls ----------
  private setMuted(on: boolean): void {
    this.settings.muted = on;
    saveSettings(this.settings);
    this.applyMaster();
  }

  private setVolume(v: number, bus: VolumeKey = "master"): void {
    if (!Number.isFinite(v) || !(bus in DEFAULTS)) return;
    this.settings.volumes[bus] = Math.min(1, Math.max(0, v));
    saveSettings(this.settings);
    if (bus === "master") this.applyMaster();
    else if (this.ctx) this.glide(this.buses.get(bus)?.gain, this.settings.volumes[bus], 0.03);
  }

  private applyMaster(): void {
    if (this.ctx) this.glide(this.master?.gain, this.settings.muted ? 0 : this.settings.volumes.master, 0.03);
  }

  /** Moves a gain towards `v` from wherever it is now, so a change never clicks. */
  private glide(p: AudioParam | undefined, v: number, tau = GLIDE): void {
    if (!p || !this.ctx) return;
    const t = this.ctx.currentTime;
    if (typeof p.cancelAndHoldAtTime === "function") p.cancelAndHoldAtTime(t);
    else {
      const now = p.value;
      p.cancelScheduledValues(t);
      p.setValueAtTime(now, t);
    }
    p.setTargetAtTime(v, t, tau);
  }

  // ---------- Events ----------
  onEvents(batch: readonly GameEvent[]): void {
    const ctx = this.ctx;
    if (ctx?.state !== "running" || !this.active) return;
    for (const e of batch) {
      if (e.type === "riseStage") {
        this.onRise(e.q, e.r, e.stage);
        continue;
      }
      const at = "q" in e && "r" in e ? { q: e.q, r: e.r } : null;
      for (const id of this.byEvent.get(e.type) ?? []) {
        const wait = WAIT_FOR_STAGE[id];
        if (wait && at) {
          this.pending.set(key(at.q, at.r), {
            id,
            event: e.type,
            q: at.q,
            r: at.r,
            deadline: ctx.currentTime + SETTLE_TIMEOUT_S,
          });
          continue;
        }
        // Redo shares the undo cue, pitched up a little (assets-source/audio/README.md).
        this.play(id, e.type, at, e.type === "redone" ? 1.12 : 1);
      }
    }
  }

  private onRise(q: number, r: number, stage: string): void {
    const p = this.pending.get(key(q, r));
    if (!p) return;
    if (stage === "sink") this.pending.delete(key(q, r));
    else if (stage === WAIT_FOR_STAGE[p.id]) {
      this.pending.delete(key(q, r));
      this.play(p.id, p.event, p, 1);
    }
  }

  private play(
    id: string,
    event: string,
    at: { q: number; r: number } | null,
    rate: number,
    gain?: number,
    record = true,
  ): void {
    const ctx = this.ctx;
    const buf = this.buffers.get(id);
    const cue = this.cues.get(id);
    if (!ctx || !buf || !cue) return;
    const t = ctx.currentTime;
    const last = this.recent.findLast((f) => f.id === id);
    if (last && t - last.t < SAME_CUE_GAP_S) return;
    const src = ctx.createBufferSource();
    src.buffer = buf;
    src.playbackRate.value = rate;
    const g = ctx.createGain();
    // Short attack and release, so a buffer that starts or ends off zero never clicks.
    const dur = buf.duration / rate;
    const level = gain ?? (at && this.view ? this.placementLevel(at.q, at.r, this.view) : 1);
    g.gain.setValueAtTime(0, t);
    g.gain.linearRampToValueAtTime(level, t + 0.005);
    g.gain.setValueAtTime(level, Math.max(t + 0.005, t + dur - 0.012));
    g.gain.linearRampToValueAtTime(0, t + dur);
    let out: AudioNode = g;
    if (at && this.view) {
      const pan = this.panner(ctx, at.q, at.r);
      g.connect(pan);
      out = pan;
    }
    out.connect(this.buses.get(busOf(cue))!);
    src.connect(g);
    src.start(t);
    const live = (this.instances.get(id) ?? []).filter((s) => s !== src);
    live.push(src);
    if (live.length > MAX_INSTANCES) live.shift()?.stop();
    this.instances.set(id, live);
    src.onended = () => {
      g.disconnect();
      out.disconnect();
      this.instances.set(
        id,
        (this.instances.get(id) ?? []).filter((s) => s !== src),
      );
    };
    if (!record) return;
    const f = { id, event, t };
    this.recent.push(f);
    if (this.recent.length > 16) this.recent.shift();
    this.fired++;
  }

  /** Placement one-shots attenuate with distance but stay audible as feedback. */
  private placementLevel(q: number, r: number, v: View): number {
    const [x, z] = cellWorld(q, r, RADIUS);
    const d = Math.hypot(x - v.camera[0], v.camera[1], z - v.camera[2]);
    return 0.4 + 0.6 * Math.min(1, 40 / Math.max(d, 40));
  }

  private panner(ctx: AudioContext, q: number, r: number): PannerNode {
    const [x, z] = cellWorld(q, r, RADIUS);
    // Panning only: the distance and focus gain is computed in audio-mix.ts and applied before this.
    return new PannerNode(ctx, {
      panningModel: "HRTF",
      distanceModel: "inverse",
      refDistance: 1,
      rolloffFactor: 0,
      positionX: x,
      positionY: 0,
      positionZ: z,
    });
  }

  // ---------- Frame ----------
  /** Called every HUD frame; the mix itself updates ten times a second. */
  update(play: PlayState | null, state: State): void {
    const wasActive = this.active;
    this.active = !!play;
    if (!play) {
      if (wasActive) this.silence();
      return;
    }
    const ctx = this.ctx;
    if (ctx?.state !== "running" || this.loading === "idle") return;
    const now = performance.now();
    if (now - this.lastAt < UPDATE_MS) return;
    this.lastAt = now;
    const cam = state.camera;
    if (!cam || !play.focus || play.zoom == null) return;
    const v: View = { camera: cam.pos, focus: play.focus, zoom: play.zoom, radius: RADIUS };
    this.view = v;
    this.listener(ctx, cam.pos, cam.forward);

    const placed: PlacedTile[] = [];
    const flowers: FlowerTile[] = [];
    const cands: Candidate[] = [];
    for (const t of play.tiles) {
      const [x, z] = cellWorld(t.q, t.r, RADIUS);
      const biome = BIOME_OF.get(t.tile) ?? "meadow";
      placed.push({ x, z, biome });
      if (FLOWER_DEFS.has(t.tile)) flowers.push({ q: t.q, r: t.r, x, z });
      const cue = EMITTER_CUE[biome];
      if (cue && this.buffers.has(cue)) cands.push({ key: `${cue}@${key(t.q, t.r)}`, cue, gain: emitterGain(x, z, v) });
    }
    this.candidates = cands;
    this.mix = biomeMix(placed, v);
    this.updateEmitters(ctx, cands);
    this.updateBeds(ctx);
    this.updateMusic(ctx);
    this.flowers = flowers;
    this.updateBees(ctx, flowers, v);
    for (const [k, p] of this.pending)
      if (ctx.currentTime > p.deadline) {
        this.pending.delete(k);
        this.play(p.id, p.event, p, 1);
      }
  }

  private listener(ctx: AudioContext, pos: readonly number[], fwd: readonly number[]): void {
    const l = ctx.listener;
    const [fx = 0, fy = -1, fz = 0] = fwd;
    // Up is world up made perpendicular to forward; straight down, the screen's up is -z.
    let [ux, uy, uz] = [-fy * fx, 1 - fy * fy, -fy * fz];
    const n = Math.hypot(ux, uy, uz);
    if (n < 1e-3) [ux, uy, uz] = [0, 0, -1];
    else [ux, uy, uz] = [ux / n, uy / n, uz / n];
    const vals = [pos[0] ?? 0, pos[1] ?? 0, pos[2] ?? 0, fx, fy, fz, ux, uy, uz];
    if (l.positionX) {
      const params = [l.positionX, l.positionY, l.positionZ, l.forwardX, l.forwardY, l.forwardZ, l.upX, l.upY, l.upZ];
      for (const [i, p] of params.entries()) p.setTargetAtTime(vals[i]!, ctx.currentTime, 0.05);
    } else {
      l.setPosition(vals[0]!, vals[1]!, vals[2]!);
      l.setOrientation(fx, fy, fz, ux, uy, uz);
    }
  }

  private loopVoice(
    ctx: AudioContext,
    cue: string,
    seed: string,
    q: number,
    r: number,
    positional: boolean,
  ): Voice | null {
    const buf = this.buffers.get(cue);
    const c = this.cues.get(cue);
    if (!buf || !c) return null;
    const src = ctx.createBufferSource();
    src.buffer = buf;
    src.loop = true;
    const start = c.loop?.startSeconds ?? 0;
    const end = Math.min(c.loop?.endSeconds ?? buf.duration, buf.duration);
    src.loopStart = start;
    src.loopEnd = end;
    const gain = ctx.createGain();
    gain.gain.value = 0;
    const pan = positional ? this.panner(ctx, q, r) : null;
    src.connect(gain);
    (pan ? gain.connect(pan) : gain).connect(this.buses.get("ambience")!);
    // Each emitter starts at its own point in the loop, so two copies of one buffer never comb.
    let h = 0;
    for (const ch of seed) h = (h * 31 + ch.charCodeAt(0)) >>> 0;
    src.start(ctx.currentTime, start + ((h % 997) / 997) * (end - start));
    return { cue, src, gain, pan, q, r, level: 0 };
  }

  private stopVoice(ctx: AudioContext, v: Voice): void {
    this.glide(v.gain.gain, 0, 0.2);
    v.src.stop(ctx.currentTime + 1.2);
    v.src.onended = () => {
      v.gain.disconnect();
      v.pan?.disconnect();
    };
  }

  private updateEmitters(ctx: AudioContext, cands: readonly Candidate[]): void {
    const want = pickEmitters(cands, new Set(this.emitters.keys()), VOICES_PER_CUE);
    for (const [k, v] of this.emitters)
      if (!want.has(k)) {
        this.stopVoice(ctx, v);
        this.emitters.delete(k);
      }
    for (const c of cands) {
      if (!want.has(c.key)) continue;
      let v = this.emitters.get(c.key);
      if (!v) {
        const [q = 0, r = 0] = c.key.split("@")[1]!.split(",").map(Number);
        const made = this.loopVoice(ctx, c.cue, c.key, q, r, true);
        if (!made) continue;
        v = made;
        this.emitters.set(c.key, v);
      }
      v.level = c.gain;
      this.glide(v.gain.gain, c.gain);
    }
  }

  /** The occasional swarm: due every 45–120 s, at a flower tile near the focus, quiet and under the focus mix. */
  private updateBees(ctx: AudioContext, flowers: readonly FlowerTile[], v: View): void {
    const { muted, volumes } = this.settings;
    const gate = beeGate({
      muted,
      master: volumes.master,
      ambience: volumes.ambience,
      loaded: this.buffers.has(BEE_CUE),
    });
    const step = beeStep(this.bees, ctx.currentTime, gate, () => pickBeeTile(flowers, v, Math.random), Math.random);
    this.bees = step.state;
    // Not recorded in `recent`: a swarm is ambience, and the event-cue history stays deterministic for tests.
    if (step.play)
      this.play(BEE_CUE, "bees", step.play, 1, BEE_LEVEL * emitterGain(step.play.x, step.play.z, v), false);
  }

  private updateBeds(ctx: AudioContext): void {
    this.bedLevels = bedGains(this.mix);
    for (const [cue, g] of this.bedLevels) {
      let v = this.beds.get(cue);
      if (!v && g > 0) {
        v = this.loopVoice(ctx, cue, `bed:${cue}`, 0, 0, false) ?? undefined;
        if (v) this.beds.set(cue, v);
      }
      if (!v) continue;
      v.level = g;
      this.glide(v.gain.gain, g, 0.6);
    }
  }

  private updateMusic(ctx: AudioContext): void {
    const ids = this.streams.filter((c) => c.kind === "music" || c.class === "music").map((c) => c.id);
    this.music = musicStep(this.music, ids, this.mix, ctx.currentTime);
    const gains = musicGains(this.music, ctx.currentTime);
    for (const [id, g] of gains) {
      let tr = this.tracks.get(id);
      if (!tr && g > 0) tr = this.track(ctx, id) ?? undefined;
      if (!tr) continue;
      tr.level = g;
      this.glide(tr.gain.gain, g, 0.25);
      if (tr.el.paused && g > 0) tr.el.play().catch(() => {});
    }
    for (const [id, tr] of this.tracks)
      if (!gains.has(id) && !tr.el.paused) {
        tr.level = 0;
        this.glide(tr.gain.gain, 0, 0.05);
        tr.el.pause();
      }
  }

  /** A music cue streams through a media element (playback "stream"), created when it first plays. */
  private track(ctx: AudioContext, id: string): Track | null {
    const file = this.format && this.cues.get(id)?.files?.[this.format];
    if (!file) return null;
    const el = document.createElement("audio");
    el.src = BASE + file.path;
    el.loop = true;
    el.preload = "auto";
    const gain = ctx.createGain();
    gain.gain.value = 0;
    ctx.createMediaElementSource(el).connect(gain).connect(this.buses.get("music")!);
    const tr = { el, gain, level: 0 };
    this.tracks.set(id, tr);
    return tr;
  }

  /** Out of the play scenario: every loop fades out and the music pauses. */
  private silence(): void {
    const ctx = this.ctx;
    if (!ctx) return;
    for (const v of [...this.emitters.values(), ...this.beds.values()]) this.stopVoice(ctx, v);
    this.emitters.clear();
    this.beds.clear();
    for (const tr of this.tracks.values()) tr.el.pause();
    this.music = musicIdle();
    this.bees = beeIdle();
    this.pending.clear();
  }

  // ---------- Debug hook ----------
  private rms(): number {
    const a = this.analyser;
    if (!a) return 0;
    const buf = new Float32Array(a.fftSize);
    a.getFloatTimeDomainData(buf);
    let s = 0;
    for (const x of buf) s += x * x;
    return Math.sqrt(s / buf.length);
  }

  private debug(): AudioDebug {
    const ctx = this.ctx;
    const voiced = new Set(this.emitters.keys());
    const emitters = [...this.candidates]
      .sort((a, b) => b.gain - a.gain)
      .slice(0, 24)
      .map((c) => {
        const [q = 0, r = 0] = c.key.split("@")[1]!.split(",").map(Number);
        return { cue: c.cue, q, r, gain: round(c.gain), voiced: voiced.has(c.key) };
      });
    const wanted = new Set(Object.values(EMITTER_CUE).filter((c): c is string => !!c));
    for (const b of Object.values(BED_CUE)) if (b) wanted.add(b.cue);
    const last = this.recent.at(-1) ?? null;
    return {
      context: ctx?.state ?? "none",
      sampleRate: ctx?.sampleRate ?? null,
      currentTime: round(ctx?.currentTime ?? 0, 3),
      format: this.format,
      loading: this.loading,
      error: this.error,
      muted: this.settings.muted,
      volumes: { ...this.settings.volumes },
      masterGain: round(this.master?.gain.value ?? 0),
      cues: {
        loaded: [...this.buffers.keys()],
        frames: Object.fromEntries([...this.buffers].map(([id, b]) => [id, b.length])),
        streams: this.streams.map((c) => c.id),
        failed: [...this.failed],
        skipped: [...this.skipped],
      },
      decodedBytes: this.decoded,
      decodedBudgetBytes: this.manifest?.decodedBudgetBytes ?? null,
      eventCues: Object.fromEntries(this.byEvent),
      emitters,
      missingEmitters: this.loading === "done" ? [...wanted].filter((c) => !this.buffers.has(c)).sort() : [],
      bed: [...this.bedLevels].map(([cue, gain]) => ({ cue, gain: round(gain) })),
      mix: Object.fromEntries(Object.entries(this.mix).map(([b, v]) => [b, round(v)])) as Mix,
      view: this.view ? { focus: [...this.view.focus] as [number, number, number], zoom: this.view.zoom } : null,
      music: {
        tracks: this.streams.length,
        current: this.music.current,
        previous: this.music.previous,
        gains: [...this.tracks].map(([id, tr]) => ({ id, gain: round(tr.level) })),
      },
      lastCue: last && { ...last },
      fired: this.fired,
      recent: this.recent.map((f) => ({ ...f })),
      outputRms: round(this.rms(), 6),
      bees: {
        cue: BEE_CUE,
        ...this.bees,
        nextAt: this.bees.nextAt === null ? null : round(this.bees.nextAt, 3),
        lastAt: this.bees.lastAt === null ? null : round(this.bees.lastAt, 3),
        lastTile: this.bees.lastTile && { ...this.bees.lastTile },
        flowerTiles: this.flowers.length,
      },
    };
  }
}
