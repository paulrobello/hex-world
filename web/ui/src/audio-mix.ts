// The focus mix (PRD §8.1) and the music playlist (§8.2) as pure functions: which tiles sound, how loud
// each emitter is for the current camera, the biome mix in view that drives the global bed and the
// music, and the equal-power crossfade between music cues. No DOM or Web Audio here (node --test).

export type Biome = "meadow" | "forest" | "river" | "coast" | "town";
export const BIOMES: readonly Biome[] = ["meadow", "forest", "river", "coast", "town"];
export type Mix = Record<Biome, number>;

/** A tile's biome from its catalog definition: houses are town, then river, coast and forest edges. */
export function biomeOf(id: string, edges: readonly string[]): Biome {
  if (id.startsWith("house")) return "town";
  if (edges.includes("River")) return "river";
  if (edges.includes("Ocean") || edges.includes("Beach")) return "coast";
  if (edges.includes("Forest")) return "forest";
  return "meadow";
}

/** The positional loop each biome's tiles carry (§8.1). Town murmur has no mastered cue yet. */
export const EMITTER_CUE: Record<Biome, string | null> = {
  meadow: null,
  forest: "forest-birdsong",
  river: "river-babble",
  coast: "surf",
  town: "town-murmur",
};

/** The non-positional bed per biome, weighted by its share of the view. Meadow is the main bed. */
export const BED_CUE: Record<Biome, { cue: string; level: number } | null> = {
  meadow: { cue: "meadow-bed", level: 0.6 },
  forest: { cue: "forest-birdsong", level: 0.25 },
  river: { cue: "river-babble", level: 0.25 },
  coast: { cue: "surf", level: 0.3 },
  town: null,
};

/** World x, z of an axial cell: hex-core `Axial::to_world` (pointy-top). */
export function cellWorld(q: number, r: number, radius: number): [number, number] {
  return [radius * Math.sqrt(3) * (q + r / 2), radius * 1.5 * r];
}

/** Web Audio's "inverse" distance model (refDistance, rolloffFactor), computed here so it can be read back. */
export function attenuation(d: number, ref: number, rolloff = 1): number {
  return ref / (ref + rolloff * (Math.max(d, ref) - ref));
}

/** Emitters stay at full level within this distance of the camera, then fall off as 1/d. */
export const REF_DISTANCE = 40;
/** The focus spread is this share of the zoom distance, never under one hex radius. */
const FOCUS_SPREAD = 0.35;
/** The view radius (biome mix) is this share of the zoom distance, never under two hex radii. */
const VIEW_SPREAD = 0.9;

/** Gaussian weight of a tile `d` metres (on the ground) from the focus point; the spread follows zoom. */
export function focusWeight(d: number, zoom: number, radius: number): number {
  const s = Math.max(radius, FOCUS_SPREAD * zoom);
  return Math.exp(-0.5 * (d / s) ** 2);
}

export interface View {
  camera: readonly [number, number, number];
  focus: readonly [number, number, number];
  zoom: number;
  radius: number;
}

/** An emitter's gain: distance attenuation from the camera times the focus weight (§8.1 focus mix). */
export function emitterGain(x: number, z: number, v: View): number {
  const d = Math.hypot(x - v.camera[0], v.camera[1], z - v.camera[2]);
  const f = Math.hypot(x - v.focus[0], z - v.focus[2]);
  return attenuation(d, REF_DISTANCE) * focusWeight(f, v.zoom, v.radius);
}

/** Share of a tile in the view: 1 at the focus point, 0 at the view radius. */
export function viewWeight(d: number, zoom: number, radius: number): number {
  const r = Math.max(2 * radius, VIEW_SPREAD * zoom);
  return Math.max(0, 1 - (d / r) ** 2);
}

export interface PlacedTile {
  x: number;
  z: number;
  biome: Biome;
}

export const emptyMix = (): Mix => ({ meadow: 0, forest: 0, river: 0, coast: 0, town: 0 });

/** The biome mix in view, summing to 1; the whole world counts evenly when nothing is in view. */
export function biomeMix(tiles: readonly PlacedTile[], v: View): Mix {
  const mix = emptyMix();
  let total = 0;
  for (const t of tiles) {
    const w = viewWeight(Math.hypot(t.x - v.focus[0], t.z - v.focus[2]), v.zoom, v.radius);
    mix[t.biome] += w;
    total += w;
  }
  if (total === 0) {
    for (const t of tiles) mix[t.biome] += 1;
    total = tiles.length;
  }
  if (total > 0) for (const b of BIOMES) mix[b] /= total;
  return mix;
}

/** Bed gain per cue from the mix; two biomes sharing a cue add up. */
export function bedGains(mix: Mix): Map<string, number> {
  const out = new Map<string, number>();
  for (const b of BIOMES) {
    const bed = BED_CUE[b];
    if (bed) out.set(bed.cue, (out.get(bed.cue) ?? 0) + bed.level * mix[b]);
  }
  return out;
}

export interface Candidate {
  key: string;
  cue: string;
  gain: number;
}

/** Below this an emitter is inaudible and gets no voice. */
export const MIN_EMITTER_GAIN = 0.004;
/** A voiced emitter keeps its voice until a rival is this much louder, so voices don't thrash. */
const KEEP_BIAS = 1.25;

/** The emitters to voice: the loudest `perCue` per cue above the floor, biased towards the current set. */
export function pickEmitters(cands: readonly Candidate[], voiced: ReadonlySet<string>, perCue: number): Set<string> {
  const score = (c: Candidate) => c.gain * (voiced.has(c.key) ? KEEP_BIAS : 1);
  const sorted = cands.filter((c) => c.gain >= MIN_EMITTER_GAIN).sort((a, b) => score(b) - score(a));
  const per = new Map<string, number>();
  const out = new Set<string>();
  for (const c of sorted) {
    const n = per.get(c.cue) ?? 0;
    if (n >= perCue) continue;
    per.set(c.cue, n + 1);
    out.add(c.key);
  }
  return out;
}

// ---------- Occasional bees (meadow-bees) ----------

export const BEE_CUE = "meadow-bees";
/** Seconds between bee swarms, drawn uniformly. */
export const BEE_DELAY_S: readonly [number, number] = [45, 120];
/** A swarm plays only at a flower tile within this many hex steps of the focus. */
export const BEE_RANGE_STEPS = 3;
/** The swarm's level before the focus mix: a quiet, passing one-shot. */
export const BEE_LEVEL = 0.35;
/** The scatter piece that makes a meadow tile a flower tile (data/tiles/*.ron `props.scatter`). */
export const FLOWER_PIECE = "wildflower_clump";

export const isFlowerTile = (biome: Biome, scatter: readonly string[]): boolean =>
  biome === "meadow" && scatter.includes(FLOWER_PIECE);

export const beeDelay = (rand: () => number): number => BEE_DELAY_S[0] + rand() * (BEE_DELAY_S[1] - BEE_DELAY_S[0]);

export interface FlowerTile {
  q: number;
  r: number;
  x: number;
  z: number;
}

/** A flower tile in range of the focus, drawn at random weighted by the focus mix; null when none is in range. */
export function pickBeeTile(tiles: readonly FlowerTile[], v: View, rand: () => number): FlowerTile | null {
  const range = BEE_RANGE_STEPS * v.radius * Math.sqrt(3);
  const near = tiles
    .map((t) => ({ t, d: Math.hypot(t.x - v.focus[0], t.z - v.focus[2]) }))
    .filter(({ d }) => d <= range)
    .map(({ t, d }) => ({ t, w: focusWeight(d, v.zoom, v.radius) }));
  const total = near.reduce((s, n) => s + n.w, 0);
  if (total <= 0) return null;
  let pick = rand() * total;
  for (const n of near) {
    pick -= n.w;
    if (pick <= 0) return n.t;
  }
  return near.at(-1)?.t ?? null;
}

/** Why a due swarm may not play, or null when it may. */
export type BeeGate = "muted" | "ambience-off" | "not-loaded" | "no-flower-tile" | null;

export function beeGate(o: { muted: boolean; master: number; ambience: number; loaded: boolean }): BeeGate {
  if (o.muted || o.master <= 0) return "muted";
  if (o.ambience <= 0) return "ambience-off";
  if (!o.loaded) return "not-loaded";
  return null;
}

export interface BeeState {
  /** Audio-clock seconds when the next swarm is due; null until the first step. */
  nextAt: number | null;
  lastAt: number | null;
  lastTile: { q: number; r: number } | null;
  plays: number;
  /** Why the last due swarm was skipped, or null when it played. */
  gatedBy: BeeGate;
}

export const beeIdle = (): BeeState => ({ nextAt: null, lastAt: null, lastTile: null, plays: 0, gatedBy: null });

/**
 * One scheduler step at audio time `now`. When a swarm is due it plays at a picked tile unless gated; either
 * way the next one is drawn 45–120 s later, so a gated swarm is skipped rather than queued.
 */
export function beeStep(
  st: BeeState,
  now: number,
  gate: BeeGate,
  pick: () => FlowerTile | null,
  rand: () => number,
): { state: BeeState; play: FlowerTile | null } {
  if (st.nextAt === null) return { state: { ...st, nextAt: now + beeDelay(rand) }, play: null };
  if (now < st.nextAt) return { state: st, play: null };
  const next = now + beeDelay(rand);
  if (gate) return { state: { ...st, nextAt: next, gatedBy: gate }, play: null };
  const tile = pick();
  if (!tile) return { state: { ...st, nextAt: next, gatedBy: "no-flower-tile" }, play: null };
  return {
    state: { nextAt: next, lastAt: now, lastTile: { q: tile.q, r: tile.r }, plays: st.plays + 1, gatedBy: null },
    play: tile,
  };
}

// ---------- Music playlist (§8.2) ----------

/**
 * The biomes each playlist cue scores on. The manifest carries no mood yet, so it lives here; cues not
 * listed (photo-mode, title-theme, discovery-stinger) never join the biome playlist.
 */
export const MUSIC_MOOD: Record<string, readonly Biome[]> = {
  "meadow-day": ["meadow", "forest", "river"],
  "coast-breeze": ["coast"],
  "hearth-town": ["town"],
};

export interface MusicState {
  current: string | null;
  previous: string | null;
  /** When the current cue was chosen (seconds, the audio clock). */
  since: number;
}

export interface MusicOpts {
  /** Crossfade length in seconds. */
  fade: number;
  /** The least time a cue plays before the mood may change it. */
  dwell: number;
  /** How much more of the mix a rival mood needs before it takes over. */
  margin: number;
}

export const MUSIC_OPTS: MusicOpts = { fade: 6, dwell: 30, margin: 0.15 };
export const musicIdle = (): MusicState => ({ current: null, previous: null, since: 0 });

export const moodScore = (id: string, mix: Mix): number => (MUSIC_MOOD[id] ?? []).reduce((s, b) => s + mix[b], 0);

/** The playlist cue the mood asks for: the highest score, ties to the earlier cue. */
export function bestCue(ids: readonly string[], mix: Mix): string | null {
  let best: string | null = null;
  let score = -1;
  for (const id of ids) {
    if (!(id in MUSIC_MOOD)) continue;
    const s = moodScore(id, mix);
    if (s > score) [best, score] = [id, s];
  }
  return best;
}

/** One playlist step at audio time `now`. With no playable cues it stays idle. */
export function musicStep(st: MusicState, ids: readonly string[], mix: Mix, now: number, o = MUSIC_OPTS): MusicState {
  const best = bestCue(ids, mix);
  let next = st;
  if (!best) {
    if (st.current) next = { current: null, previous: st.current, since: now };
  } else if (!st.current || !ids.includes(st.current)) next = { current: best, previous: st.current, since: now };
  else if (
    best !== st.current &&
    now - st.since >= o.dwell &&
    moodScore(best, mix) - moodScore(st.current, mix) >= o.margin
  )
    next = { current: best, previous: st.current, since: now };
  // The outgoing cue is forgotten once its fade has finished.
  if (next.previous && now - next.since >= o.fade) next = { ...next, previous: null };
  return next;
}

/** Equal-power crossfade gains at `now`: the incoming cue rises on a sine, the outgoing falls on a cosine. */
export function musicGains(st: MusicState, now: number, fade = MUSIC_OPTS.fade): Map<string, number> {
  const p = Math.min(1, Math.max(0, (now - st.since) / fade));
  const out = new Map<string, number>();
  if (st.previous) out.set(st.previous, Math.cos((p * Math.PI) / 2));
  if (st.current) out.set(st.current, Math.sin((p * Math.PI) / 2));
  return out;
}
