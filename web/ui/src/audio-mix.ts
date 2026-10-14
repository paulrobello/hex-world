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

/** One row of the occasional one-shot scheduler. */
export interface OneShotRow {
  name: string;
  /** The pool whose variants it plays, resolved from the manifest by `resolveOneShot`. */
  pool: string;
  /** Which tile defs it may play at, from the def's biome and scatter. */
  tile: (biome: Biome, scatter: readonly string[]) => boolean;
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
];

/** The catalog defs a row may play at. */
export function oneShotDefs(
  row: OneShotRow,
  tiles: readonly { id: string; edges: readonly string[]; scatter: readonly string[] }[],
): Set<string> {
  return new Set(tiles.filter((t) => row.tile(biomeOf(t.id, t.edges), t.scatter)).map((t) => t.id));
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
