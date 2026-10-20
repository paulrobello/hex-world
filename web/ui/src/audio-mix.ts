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

/**
 * The positional loop each biome's tiles carry (§8.1). Town murmur has no mastered cue yet. Meadow and forest
 * have no loop: they sound only through the occasional one-shots below.
 */
export const EMITTER_CUE: Record<Biome, string | null> = {
  meadow: null,
  forest: null,
  river: "river-babble",
  coast: "surf",
  town: "town-murmur",
};

/** The positional loops a tile carries: its biome's, plus the surf on a river mouth (River and sea edges). */
export function emitterCues(id: string, edges: readonly string[]): string[] {
  const biome = biomeOf(id, edges);
  const cues = [EMITTER_CUE[biome]];
  if (biome === "river" && (edges.includes("Ocean") || edges.includes("Beach"))) cues.push(EMITTER_CUE.coast);
  return cues.filter((c): c is string => !!c);
}

/** The non-positional bed per biome, weighted by its share of the view. Only river and coast carry one. */
export const BED_CUE: Record<Biome, { cue: string; level: number } | null> = {
  meadow: null,
  forest: null,
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

// ---------- Occasional one-shots (meadow bees, forest birds and leaves) ----------

/** The scatter piece that makes a meadow tile a flower tile (data/tiles/*.ron `props.scatter`). */
export const FLOWER_PIECE = "wildflower_clump";

export const isFlowerTile = (biome: Biome, scatter: readonly string[]): boolean =>
  biome === "meadow" && scatter.includes(FLOWER_PIECE);

const isForestTile = (biome: Biome): boolean => biome === "forest";

/** A predicate for one tile def by id: farm, windmill and wheat field share the meadow biome and scatter. */
const isTileId =
  (want: string) =>
  (_biome: Biome, _scatter: readonly string[], id: string): boolean =>
    id === want;

/** One row of the occasional one-shot scheduler. */
export interface OneShotRow {
  name: string;
  /** The pool whose variants it plays, resolved from the manifest by `resolveOneShot`. */
  pool: string;
  /** Which tile defs it may play at, from the def's biome, scatter and id. */
  tile: (biome: Biome, scatter: readonly string[], id: string) => boolean;
  /** Seconds between plays, drawn uniformly. */
  delay: readonly [number, number];
  /** It plays only at a matching tile within this many hex steps of the focus. */
  rangeSteps: number;
  /** Its level before the focus mix. */
  level: number;
}

export const ONE_SHOTS: readonly OneShotRow[] = [
  { name: "bees", pool: "meadow-bees", tile: isFlowerTile, delay: [45, 120], rangeSteps: 3, level: 0.35 },
  { name: "forest-birds", pool: "forest-birds", tile: isForestTile, delay: [12, 40], rangeSteps: 3, level: 0.4 },
  { name: "forest-leaves", pool: "forest-leaves", tile: isForestTile, delay: [20, 60], rangeSteps: 3, level: 0.3 },
  { name: "farm-cow", pool: "farm-cow", tile: isTileId("farm"), delay: [30, 90], rangeSteps: 3, level: 0.25 },
  { name: "farm-sheep", pool: "farm-sheep", tile: isTileId("farm"), delay: [30, 90], rangeSteps: 3, level: 0.25 },
  {
    name: "windmill-creak",
    pool: "windmill-creak",
    tile: isTileId("windmill"),
    delay: [25, 70],
    rangeSteps: 3,
    level: 0.3,
  },
  {
    name: "wheat-rustle",
    pool: "wheat-rustle",
    tile: isTileId("wheat_field"),
    delay: [20, 60],
    rangeSteps: 3,
    level: 0.3,
  },
];

/** The catalog defs a row may play at. */
export function oneShotDefs(
  row: OneShotRow,
  tiles: readonly { id: string; edges: readonly string[]; scatter: readonly string[] }[],
): Set<string> {
  return new Set(tiles.filter((t) => row.tile(biomeOf(t.id, t.edges), t.scatter, t.id)).map((t) => t.id));
}

export interface ResolvedOneShot {
  pool: string;
  /** Cue ids in manifest order. */
  variants: string[];
  row: OneShotRow | null;
}

/**
 * A row name, pool or cue id against the manifest's cues: a pool's variants are the cues whose `pool` is it,
 * else the one cue with that id (which keeps its own pool, if any). Null when nothing matches.
 */
export function resolveOneShot(
  name: string,
  cues: readonly { id: string; pool?: string }[],
  rows: readonly OneShotRow[] = ONE_SHOTS,
): ResolvedOneShot | null {
  const pool = rows.find((r) => r.name === name)?.pool ?? name;
  const rowOf = (p: string) => rows.find((r) => r.pool === p) ?? null;
  const variants = cues.filter((c) => c.pool === pool).map((c) => c.id);
  if (variants.length > 0) return { pool, variants, row: rowOf(pool) };
  const cue = cues.find((c) => c.id === pool);
  if (!cue) return null;
  const own = cue.pool ?? cue.id;
  return { pool: own, variants: [cue.id], row: rowOf(own) };
}

/** A variant at random, never `last` again while the pool has another; null for an empty pool. */
export function pickVariant(variants: readonly string[], last: string | null, rand: () => number): string | null {
  const pool = variants.length > 1 ? variants.filter((v) => v !== last) : variants;
  if (pool.length <= 1) return pool[0] ?? null;
  return pool[Math.min(pool.length - 1, Math.floor(rand() * pool.length))]!;
}

export const oneShotDelay = (row: OneShotRow, rand: () => number): number =>
  row.delay[0] + rand() * (row.delay[1] - row.delay[0]);

export interface OneShotTile {
  q: number;
  r: number;
  x: number;
  z: number;
}

/** A matching tile in range of the focus, drawn at random weighted by the focus mix; null when none is in range. */
export function pickOneShotTile(
  row: OneShotRow,
  tiles: readonly OneShotTile[],
  v: View,
  rand: () => number,
): OneShotTile | null {
  const range = row.rangeSteps * v.radius * Math.sqrt(3);
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

/** The matching tile nearest the focus, at any distance; null when there is none. */
export function nearestOneShotTile(tiles: readonly OneShotTile[], focus: View["focus"]): OneShotTile | null {
  let best: OneShotTile | null = null;
  let bestD = Number.POSITIVE_INFINITY;
  for (const t of tiles) {
    const d = Math.hypot(t.x - focus[0], t.z - focus[2]);
    if (d < bestD) [best, bestD] = [t, d];
  }
  return best;
}

/** Why a due one-shot may not play, or null when it may. */
export type OneShotGate = "muted" | "ambience-off" | "not-loaded" | "no-tile" | null;

export function oneShotGate(o: { muted: boolean; master: number; ambience: number; loaded: boolean }): OneShotGate {
  if (o.muted || o.master <= 0) return "muted";
  if (o.ambience <= 0) return "ambience-off";
  if (!o.loaded) return "not-loaded";
  return null;
}

export interface OneShotState {
  /** Audio-clock seconds when the next play is due; null until the first step. */
  nextAt: number | null;
  lastAt: number | null;
  lastTile: { q: number; r: number } | null;
  /** The variant played last, so the next pick avoids it. */
  lastCue: string | null;
  plays: number;
  /** Why the last due play was skipped, or null when it played. */
  gatedBy: OneShotGate;
}

export const oneShotIdle = (): OneShotState => ({
  nextAt: null,
  lastAt: null,
  lastTile: null,
  lastCue: null,
  plays: 0,
  gatedBy: null,
});

/**
 * One scheduler step at audio time `now`. When a play is due it sounds a variant (from the decoded `variants`)
 * at a picked tile unless gated; either way the next one is drawn from the row's delay, so a gated play is
 * skipped rather than queued.
 */
export function oneShotStep(
  row: OneShotRow,
  st: OneShotState,
  now: number,
  gate: OneShotGate,
  pick: () => OneShotTile | null,
  variants: readonly string[],
  rand: () => number,
): { state: OneShotState; play: { tile: OneShotTile; cue: string } | null } {
  if (st.nextAt === null) return { state: { ...st, nextAt: now + oneShotDelay(row, rand) }, play: null };
  if (now < st.nextAt) return { state: st, play: null };
  const next = now + oneShotDelay(row, rand);
  if (gate) return { state: { ...st, nextAt: next, gatedBy: gate }, play: null };
  const tile = pick();
  if (!tile) return { state: { ...st, nextAt: next, gatedBy: "no-tile" }, play: null };
  const cue = pickVariant(variants, st.lastCue, rand);
  if (!cue) return { state: { ...st, nextAt: next, gatedBy: "not-loaded" }, play: null };
  return {
    state: {
      nextAt: next,
      lastAt: now,
      lastTile: { q: tile.q, r: tile.r },
      lastCue: cue,
      plays: st.plays + 1,
      gatedBy: null,
    },
    play: { tile, cue },
  };
}

// ---------- Cars (owner request 2026-10-14) ----------

/** A car cue: the pool it plays from and its level before the focus mix. */
export interface CarCue {
  pool: string;
  level: number;
}

export type CarKind = "start" | "door" | "drive";

/** Trip events and their cues; the drive pass-by comes from `play.car_list`, not from an event. */
export const CAR_CUES: Record<CarKind, CarCue> = {
  start: { pool: "car-start", level: 0.4 },
  door: { pool: "car-door", level: 0.4 },
  drive: { pool: "car-drive", level: 0.3 },
};
export const CAR_EVENT_KIND: Record<string, CarKind> = { carPulledOut: "start", carParked: "door" };
/** Cars sound only within this many hex steps of the focus. */
export const CAR_RANGE_STEPS = 3;
/** Car voices sounding at once, every kind together. */
export const CAR_VOICES = 3;
/** Least time between two drive pass-bys starting, so cars entering range together never stack. */
export const CAR_DRIVE_GAP_S = 1.5;
/** A trip event waits this long for a free voice (its car's or the cap's), then it is dropped. */
export const CAR_EVENT_WAIT_S = 2;
/** Each car plays its cues at its own rate, up to this fraction either way, since the pools are small. */
export const CAR_RATE_SPREAD = 0.05;

/** A trip event waiting for a voice, or a driving car that may get its pass-by. */
export interface CarRequest {
  car: number;
  kind: CarKind;
  x: number;
  z: number;
  /** Audio time the event arrived; drives use the step's time. */
  at: number;
}

export interface CarVoice {
  car: number;
  cue: string;
  kind: CarKind;
  endsAt: number;
}

export interface CarState {
  /** Voices still sounding. */
  voices: CarVoice[];
  /** Trip events waiting for a voice. */
  pending: CarRequest[];
  /** Cars whose pass-by already played on their current trip. */
  drove: number[];
  /** Per pool, the variant played last, so the next pick avoids it. */
  lastCue: Record<string, string>;
  lastDriveAt: number | null;
  plays: number;
  /** Why the last car cue was held back, or null when it played. */
  gatedBy: OneShotGate | "out-of-range" | "cap" | null;
}

export const carIdle = (): CarState => ({
  voices: [],
  pending: [],
  drove: [],
  lastCue: {},
  lastDriveAt: null,
  plays: 0,
  gatedBy: null,
});

/** A car's playback rate: fixed per car id, within CAR_RATE_SPREAD of 1. */
export function carRate(id: number): number {
  let h = Math.imul(id, 2654435761) >>> 0;
  h = (h ^ (h >>> 15)) >>> 0;
  return 1 + CAR_RATE_SPREAD * ((h % 2001) / 1000 - 1);
}

export function carInRange(x: number, z: number, v: View): boolean {
  return Math.hypot(x - v.focus[0], z - v.focus[2]) <= CAR_RANGE_STEPS * v.radius * Math.sqrt(3);
}

export interface DrivingCar {
  id: number;
  phase: string;
  x: number;
  z: number;
}

export interface CarPlay {
  car: number;
  kind: CarKind;
  cue: string;
  x: number;
  z: number;
  gain: number;
  rate: number;
}

/**
 * One car step at audio time `now`. New trip events join the queue (a trip's start or end also clears the
 * car's pass-by mark), then the queue, oldest first, and the driving cars on the road, loudest first, get
 * voices while fewer than CAR_VOICES sound: never a second voice for one car, never a car outside the range,
 * one pass-by per car per trip, and pass-bys at least CAR_DRIVE_GAP_S apart. A gated step drops the queue.
 * While `paused` (photo mode holding the agents) `driving` is frozen, so no pass-by starts.
 */
export function carStep(
  st: CarState,
  now: number,
  events: readonly CarRequest[],
  driving: readonly DrivingCar[],
  v: View,
  gate: OneShotGate,
  variants: (pool: string) => readonly string[],
  seconds: (cue: string) => number,
  rand: () => number,
  paused = false,
): { state: CarState; play: CarPlay[] } {
  const live = new Set(driving.map((c) => c.id));
  const tripped = new Set(events.map((e) => e.car));
  const s: CarState = {
    ...st,
    voices: st.voices.filter((x) => x.endsAt > now),
    pending: [...st.pending, ...events].filter((e) => now - e.at <= CAR_EVENT_WAIT_S),
    drove: st.drove.filter((id) => live.has(id) && !tripped.has(id)),
    lastCue: { ...st.lastCue },
  };
  if (gate) return { state: { ...s, pending: [], gatedBy: gate }, play: [] };
  const play: CarPlay[] = [];
  const voiced = (car: number) => s.voices.some((x) => x.car === car);
  const start = (r: CarRequest): boolean => {
    const row = CAR_CUES[r.kind];
    const gain = row.level * emitterGain(r.x, r.z, v);
    const cue = gain >= MIN_EMITTER_GAIN ? pickVariant(variants(row.pool), s.lastCue[row.pool] ?? null, rand) : null;
    if (!cue) return false;
    const rate = carRate(r.car);
    s.lastCue[row.pool] = cue;
    s.voices.push({ car: r.car, cue, kind: r.kind, endsAt: now + seconds(cue) / rate });
    s.plays++;
    s.gatedBy = null;
    play.push({ car: r.car, kind: r.kind, cue, x: r.x, z: r.z, gain, rate });
    return true;
  };
  const waiting: CarRequest[] = [];
  for (const e of s.pending) {
    if (!carInRange(e.x, e.z, v)) {
      s.gatedBy = "out-of-range";
      continue;
    }
    if (voiced(e.car) || s.voices.length >= CAR_VOICES) {
      if (s.voices.length >= CAR_VOICES) s.gatedBy = "cap";
      waiting.push(e);
      continue;
    }
    start(e);
  }
  s.pending = waiting;
  if (paused) return { state: s, play };
  const drives = driving
    .filter((c) => c.phase === "road" && !s.drove.includes(c.id) && !voiced(c.id) && carInRange(c.x, c.z, v))
    .map((c) => ({ c, g: emitterGain(c.x, c.z, v) }))
    .sort((a, b) => b.g - a.g);
  for (const { c } of drives) {
    if (s.voices.length >= CAR_VOICES) {
      s.gatedBy = "cap";
      break;
    }
    if (s.lastDriveAt !== null && now - s.lastDriveAt < CAR_DRIVE_GAP_S) break;
    if (start({ car: c.id, kind: "drive", x: c.x, z: c.z, at: now })) {
      s.drove.push(c.id);
      s.lastDriveAt = now;
    }
  }
  return { state: s, play };
}

// ---------- Shop bell (P3 bakery) ----------

/** The bell a car's visit rings at a shop: its pool, the tile def that is a shop, its level and its lag. */
export const SHOP_BELL = {
  pool: "shop-bell",
  tile: "bakery",
  level: 0.35,
  /** Seconds after the park (and its door) before the shop door's bell rings. */
  delayS: 1.2,
} as const;

/** A park at a shop, waiting for its bell. */
export interface BellRequest {
  q: number;
  r: number;
  x: number;
  z: number;
  /** Audio time the bell is due. */
  dueAt: number;
}

export interface BellState {
  pending: BellRequest[];
  lastAt: number | null;
  lastTile: { q: number; r: number } | null;
  lastCue: string | null;
  plays: number;
  /** Why the last due bell was dropped, or null when it rang. */
  gatedBy: OneShotGate | "out-of-range" | null;
}

export const bellIdle = (): BellState => ({
  pending: [],
  lastAt: null,
  lastTile: null,
  lastCue: null,
  plays: 0,
  gatedBy: null,
});

export interface BellPlay {
  cue: string;
  q: number;
  r: number;
  x: number;
  z: number;
  gain: number;
}

/**
 * One bell step at audio time `now`. A park whose cell (the spot's owner) is a shop joins the queue, due
 * SHOP_BELL.delayS later; any other park is ignored. A due bell rings a variant (never the last one again) at
 * the spot under the focus mix, or is dropped when gated, out of range or too quiet.
 */
export function bellStep(
  st: BellState,
  now: number,
  parks: readonly { q: number; r: number; x: number; z: number }[],
  isShop: (q: number, r: number) => boolean,
  v: View,
  gate: OneShotGate,
  variants: readonly string[],
  rand: () => number,
): { state: BellState; play: BellPlay[] } {
  const s: BellState = {
    ...st,
    pending: [
      ...st.pending,
      ...parks.filter((p) => isShop(p.q, p.r)).map((p) => ({ ...p, dueAt: now + SHOP_BELL.delayS })),
    ],
  };
  const play: BellPlay[] = [];
  const waiting: BellRequest[] = [];
  for (const b of s.pending) {
    if (b.dueAt > now) {
      waiting.push(b);
      continue;
    }
    if (gate) {
      s.gatedBy = gate;
      continue;
    }
    if (!carInRange(b.x, b.z, v)) {
      s.gatedBy = "out-of-range";
      continue;
    }
    const gain = SHOP_BELL.level * emitterGain(b.x, b.z, v);
    const cue = gain >= MIN_EMITTER_GAIN ? pickVariant(variants, s.lastCue, rand) : null;
    if (!cue) {
      s.gatedBy = variants.length === 0 ? "not-loaded" : "out-of-range";
      continue;
    }
    Object.assign(s, { lastAt: now, lastTile: { q: b.q, r: b.r }, lastCue: cue, plays: s.plays + 1, gatedBy: null });
    play.push({ cue, q: b.q, r: b.r, x: b.x, z: b.z, gain });
  }
  s.pending = waiting;
  return { state: s, play };
}

// ---------- Music playlist (§8.2) ----------

/**
 * The biomes each playlist cue scores on. The manifest carries no mood yet, so it lives here; cues not
 * listed (photo-mode, title-theme, discovery-stinger) never join the biome playlist. photo-mode plays as
 * the forced cue while photo mode is open; title-theme and discovery-stinger have no hook yet.
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

/** The cue photo mode (D7) plays in place of the biome playlist. */
export const PHOTO_CUE = "photo-mode";

/**
 * One playlist step at audio time `now`. With no playable cues it stays idle. A playable `forced` cue
 * (photo mode's) takes over at once, past the dwell and margin; when it clears, the mood's pick returns
 * at once, since a cue outside the playlist never holds the dwell.
 */
export function musicStep(
  st: MusicState,
  ids: readonly string[],
  mix: Mix,
  now: number,
  o = MUSIC_OPTS,
  forced: string | null = null,
): MusicState {
  const pinned = forced && ids.includes(forced) ? forced : null;
  const best = pinned ?? bestCue(ids, mix);
  let next = st;
  if (!best) {
    if (st.current) next = { current: null, previous: st.current, since: now };
  } else if (!st.current || !ids.includes(st.current)) next = { current: best, previous: st.current, since: now };
  else if (
    best !== st.current &&
    (pinned !== null ||
      !(st.current in MUSIC_MOOD) ||
      (now - st.since >= o.dwell && moodScore(best, mix) - moodScore(st.current, mix) >= o.margin))
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
