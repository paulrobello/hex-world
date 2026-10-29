// The placement rules the sheet reads (PRD §4.1–4.2), from the Rust catalog and socket table in
// bridge-schema.json. hex-core stays the authority: test/rules.test.ts checks
// legalOrientations against hex-core's own answers for every slot of six worlds (rules-parity.json).
import type { Cell, PlayTile, SocketKind } from "./bridge.ts";
import schema from "./bridge-schema.json" with { type: "json" };

/** A network crossing at an edge, in metres (hex-core `LaneSpec`): `[offset, width]`. */
export type Lane = readonly [offset: number, width: number];

/**
 * Whether two lanes line up across a seam (hex-core `LaneSpec::meets`, as `edges_compatible`
 * uses it): both absent, or both present with equal width and mirrored offset.
 */
export const laneMeets = (a: Lane | null, b: Lane | null): boolean =>
  a === null || b === null ? a === b : a[1] === b[1] && a[0] === -b[0];

/**
 * Which side of a directional (beach) edge the land is on, looking out across the edge from its own
 * tile: `Right` is the corner shared with edge e + 1 (hex-core `Side`). Across a seam the two tiles
 * look opposite ways, so a coast carries on only where `Left` meets `Right` (owner O1).
 */
export type Side = "Left" | "Right";

export interface TileDef {
  id: string;
  edges: SocketKind[];
  /** The land side of each directional edge, null on the others (hex-core `EdgeSocket::land`). */
  land: (Side | null)[];
  /**
   * Which local edges are rail ends that get a buffer stop (hex-core `rail_end::stopped_edges`):
   * such an end also meets a tile that carries no rail, as `STOPPED_END_GROUND` (owner, 2026-10-25).
   */
  stopped: boolean[];
  /** Whether any edge carries rail (hex-core `rail_end::carries_rail`). */
  rail: boolean;
  /**
   * Rotations after which its look repeats (hex-core `orientation_period`): 6 with hero props, which
   * turn with it, 3 for the wheat field's rows, 1 where the sockets alone decide (`same_orientation`).
   */
  period: number;
  /**
   * Each local edge's relative corner heights `[start, end]`, clockwise viewed from above (hex-core
   * `EdgeSocket::corners`). Equal heights are a flat edge; a slope differs by one.
   */
  heights: [number, number][];
  /**
   * Each local edge's lane `[offset, width]` in metres, null where it has none (hex-core
   * `EdgeSocket::lane`): edges with a lane never meet across a cliff.
   */
  lanes: (Lane | null)[];
  /** Whether the tile may stand on a level above 0 (hex-core `TileDef::raisable`). */
  raisable: boolean;
  /** Its highest relative corner (hex-core `TileDef::top`): `level + top` stays within MAX_LEVEL. */
  top: number;
  /** The local edge a tunnel portal's mouth opens on (hex-core `TileDef::tunnel`), null on every other tile. */
  tunnel: number | null;
}

export const TILES: TileDef[] = schema.catalog.tiles.map((t) => ({
  id: t.id,
  edges: t.edges as SocketKind[],
  land: t.land as (Side | null)[],
  stopped: t.stopped,
  rail: t.rail,
  period: t.period,
  heights: t.heights as [number, number][],
  lanes: t.lanes as (Lane | null)[],
  raisable: t.raisable,
  top: t.top,
  tunnel: t.tunnel,
}));
export const START_TILE: string = schema.catalog.start;
/** Highest level a tile may stand on (hex-core `MAX_LEVEL`). */
export const MAX_LEVEL: number = schema.catalog.max_level;
const COMPAT = schema.catalog.compatible as Record<SocketKind, SocketKind[]>;
/** What a stopped rail end meets a tile without rail as (hex-core `rail_end::STOPPED_END_GROUND`). */
const STOPPED_END_GROUND = schema.catalog.stopped_end_ground as SocketKind;
const BY_ID = new Map(TILES.map((t) => [t.id, t]));

export const tileDef = (id: string): TileDef | undefined => BY_ID.get(id);

/**
 * One Tiles sheet entry (hex-core `TileCatalog::picker`): a lone tile, or a picker group such as the
 * Beach Curve, whose members R steps through in turn (owner, 2026-10-20). `id` is what `select_tile`
 * takes: the tile id, or the group id.
 */
export interface PickerEntry {
  id: string;
  tiles: TileDef[];
}

export const PICKER: PickerEntry[] = schema.catalog.picker.map((e) => ({
  id: e.id,
  tiles: e.tiles.map((t) => BY_ID.get(t)!),
}));
const ENTRY_OF = new Map(PICKER.flatMap((e) => e.tiles.map((t) => [t.id, e] as const)));

/** The picker entry a tile id is listed under. */
export const entryOf = (id: string): PickerEntry | undefined => ENTRY_OF.get(id);

/** Axial offsets per world edge: E, SE, SW, W, NW, NE (hex-core axial.rs). */
const DIRS: ReadonlyArray<readonly [number, number]> = [
  [1, 0],
  [0, 1],
  [-1, 1],
  [-1, 0],
  [0, -1],
  [1, -1],
];

export const cellKey = (c: Cell): string => `${c.q},${c.r}`;
export const neighbor = (c: Cell, edge: number): Cell => {
  const d = DIRS[edge % 6]!;
  return { q: c.q + d[0], r: c.r + d[1] };
};
export const compatible = (a: SocketKind, b: SocketKind): boolean => COMPAT[a].includes(b);

/** World-edge sockets of a tile turned by k: local edge e faces world edge e + k. */
export function rotated(def: TileDef, k: number): SocketKind[] {
  return [0, 1, 2, 3, 4, 5].map((w) => def.edges[(w - k + 12) % 6]!);
}

/** World-edge land sides of a tile turned by k (a turn never changes a side). */
export function rotatedLand(def: TileDef, k: number): (Side | null)[] {
  return [0, 1, 2, 3, 4, 5].map((w) => def.land[(w - k + 12) % 6]!);
}

/** Which world edges of a tile turned by k are stopped rail ends. */
export function rotatedStopped(def: TileDef, k: number): boolean[] {
  return [0, 1, 2, 3, 4, 5].map((w) => def.stopped[(w - k + 12) % 6]!);
}

/** How many levels a tile's highest corner stands above its own level (hillside, ramp: 1; flat: 0). */
export const tileRise = (id: string): number => Math.max(0, ...(tileDef(id)?.heights.flat() ?? [0]));

/** World-edge corner heights `[start, end]` of a tile turned by k and standing on `level`. */
export function rotatedHeights(def: TileDef, k: number, level: number): [number, number][] {
  return [0, 1, 2, 3, 4, 5].map((w) => {
    const [s, e] = def.heights[(w - k + 12) % 6]!;
    return [s + level, e + level];
  });
}

/** Which lane, if any, each world edge of a tile turned by k carries. */
export function rotatedLanes(def: TileDef, k: number): (Lane | null)[] {
  return [0, 1, 2, 3, 4, 5].map((w) => def.lanes[(w - k + 12) % 6]!);
}

/** One edge's height: its relative corners `[start, end]` and whether it carries a lane. */
export interface EdgeHeight {
  heights: readonly [number, number];
  lane: boolean;
}

export type HeightMeet = "Seamless" | "Cliff" | "Refused";

/** {@link heightsMeet} on absolute corner heights. */
function absoluteMeet(
  a: readonly [number, number],
  aLane: boolean,
  b: readonly [number, number],
  bLane: boolean,
): HeightMeet {
  if (a[0] === b[1] && a[1] === b[0]) return "Seamless";
  const flat = (h: readonly [number, number]) => h[0] === h[1];
  return flat(a) && flat(b) && !aLane && !bLane ? "Cliff" : "Refused";
}

/**
 * How edge `a` of a tile on `aLevel` and edge `b` across the seam on `bLevel` meet in height
 * (hex-core `heights_meet`): seamless when a's absolute (start, end) equals b's (end, start), as the
 * edges run opposite ways; a cliff when both are flat without lanes but differ; refused otherwise.
 */
export function heightsMeet(a: EdgeHeight, aLevel: number, b: EdgeHeight, bLevel: number): HeightMeet {
  const abs = (e: EdgeHeight, l: number): [number, number] => [e.heights[0] + l, e.heights[1] + l];
  return absoluteMeet(abs(a, aLevel), a.lane, abs(b, bLevel), b.lane);
}

/** Equal for two rotations exactly when their world-edge sockets are equal and they are a whole number of periods apart. */
const orientation = (def: TileDef, k: number): string =>
  `${k % def.period}|${rotated(def, k).join()}|${rotatedLand(def, k).join()}`;

/**
 * Whether my edge may meet the edge it faces (hex-core `edges_compatible` for MVP edges): the kinds
 * are compatible, and directional edges have opposite land sides (both absent, or Left on Right).
 */
export const meets = (theirs: SocketKind, theirLand: Side | null, mine: SocketKind, myLand: Side | null): boolean =>
  compatible(theirs, mine) && (theirLand === null || myLand === null ? theirLand === myLand : theirLand !== myLand);

/**
 * One side of a seam: the edge's kind, land side and whether it is a stopped rail end, whether its
 * tile carries rail, the edge's absolute corner heights `[start, end]` and its lane.
 */
export interface Facing {
  kind: SocketKind;
  land: Side | null;
  stopped: boolean;
  rail: boolean;
  heights: readonly [number, number];
  lane: Lane | null;
}

/**
 * Whether `end` is a stopped rail end that `other`, a tile without rail, meets as grass (owner,
 * 2026-10-25). The stop meets it as lane-free ground at the end's own height, so it may face raised
 * ground across a cliff.
 */
const stopCloses = (end: Facing, other: Facing): boolean =>
  end.stopped &&
  !other.rail &&
  other.land === null &&
  other.lane === null &&
  compatible(STOPPED_END_GROUND, other.kind) &&
  absoluteMeet(end.heights, false, other.heights, false) !== "Refused";

/**
 * Whether my edge may be placed against the placed edge it faces (hex-core
 * `TileCatalog::placement_meets`): the sockets meet (kinds, land sides, lanes and heights), or
 * theirs is a stopped rail end and my tile carries no rail. A tile with rail never turns an end
 * toward a placed tile without rail.
 */
export const placementMeets = (theirs: Facing, mine: Facing): boolean =>
  (meets(theirs.kind, theirs.land, mine.kind, mine.land) &&
    laneMeets(theirs.lane, mine.lane) &&
    absoluteMeet(mine.heights, mine.lane !== null, theirs.heights, theirs.lane !== null) !== "Refused") ||
  stopCloses(theirs, mine);

export type TileMap = Map<string, PlayTile>;
export const tileMap = (tiles: readonly PlayTile[]): TileMap => new Map(tiles.map((t) => [cellKey(t), t]));

/** The socket kinds around a slot, by world edge, null where nothing touches it. */
export type Kinds = (SocketKind | null)[];

/**
 * What each world edge of a slot touches: the neighbour's facing socket kind and its land side,
 * whether that edge is a stopped rail end, whether the neighbour carries rail, the neighbour's
 * level, the facing edge's absolute corner heights `[start, end]` (null where nothing touches)
 * and whether it carries a lane.
 */
export interface Needs {
  kinds: Kinds;
  land: (Side | null)[];
  stopped: boolean[];
  rail: boolean[];
  levels: number[];
  heights: ([number, number] | null)[];
  lanes: (Lane | null)[];
}

export function needsAt(slot: Cell, tiles: TileMap): Needs {
  const facing = [0, 1, 2, 3, 4, 5].map((w) => {
    const n = tiles.get(cellKey(neighbor(slot, w)));
    const def = n && tileDef(n.tile);
    const back = (w + 3) % 6;
    const level = n?.level ?? 0;
    return def
      ? {
          kind: rotated(def, n.rotation)[back]!,
          land: rotatedLand(def, n.rotation)[back]!,
          stopped: rotatedStopped(def, n.rotation)[back]!,
          rail: def.rail,
          level,
          heights: rotatedHeights(def, n.rotation, level)[back]!,
          lane: rotatedLanes(def, n.rotation)[back]!,
        }
      : null;
  });
  return {
    kinds: facing.map((f) => f?.kind ?? null),
    land: facing.map((f) => f?.land ?? null),
    stopped: facing.map((f) => f?.stopped ?? false),
    rail: facing.map((f) => f?.rail ?? false),
    levels: facing.map((f) => f?.level ?? 0),
    heights: facing.map((f) => f?.heights ?? null),
    lanes: facing.map((f) => f?.lane ?? null),
  };
}

export const isSlot = (c: Cell, tiles: TileMap): boolean =>
  !tiles.has(cellKey(c)) && [0, 1, 2, 3, 4, 5].some((w) => tiles.has(cellKey(neighbor(c, w))));

/** What my edge on world edge `w` of `def` turned by k on `level` offers, and what it faces. */
function seam(
  def: TileDef,
  k: number,
  level: number,
  needs: Needs,
  w: number,
): { mine: Facing; theirs: Facing } | null {
  const kind = needs.kinds[w];
  if (kind == null) return null;
  const theirs: Facing = {
    kind,
    land: needs.land[w] ?? null,
    stopped: needs.stopped[w] ?? false,
    rail: needs.rail[w] ?? false,
    heights: needs.heights[w] ?? [0, 0],
    lane: needs.lanes[w] ?? null,
  };
  const mine: Facing = {
    kind: rotated(def, k)[w]!,
    land: rotatedLand(def, k)[w]!,
    stopped: rotatedStopped(def, k)[w]!,
    rail: def.rail,
    heights: rotatedHeights(def, k, level)[w]!,
    lane: rotatedLanes(def, k)[w]!,
  };
  return { mine, theirs };
}

/** World edges where `def` turned by k on `level` (0 by default) does not meet what the slot touches. */
function clashes(def: TileDef, k: number, needs: Needs, level = 0): number[] {
  return [0, 1, 2, 3, 4, 5].filter((w) => {
    const s = seam(def, k, level, needs, w);
    return s !== null && !placementMeets(s.theirs, s.mine);
  });
}

/**
 * Whether `def` turned by k on `level` meets any placed neighbour across a cliff (hex-core
 * `World::cliff_edges`): an accepted seam whose corners differ, a stopped rail end included.
 */
function hasCliff(def: TileDef, k: number, needs: Needs, level: number): boolean {
  return [0, 1, 2, 3, 4, 5].some((w) => {
    const s = seam(def, k, level, needs, w);
    return (
      s !== null &&
      absoluteMeet(s.mine.heights, s.mine.lane !== null, s.theirs.heights, s.theirs.lane !== null) !== "Seamless"
    );
  });
}

/** A rotation and the level a tile stands on (hex-core `Orientation`). */
export interface Orientation {
  rotation: number;
  level: number;
}

/**
 * Orientations in which `def` fits a slot with these needs, in hex-core `legal_orientations` order:
 * cliff-free first, then by level, then by rotation. A tile that is not raisable stands on level 0.
 */
export function legalOrientations(def: TileDef, needs: Needs): Orientation[] {
  return legalEntries(def, needs).map((e) => e.o);
}

/** {@link legalOrientations} with each orientation's cliff flag (hex-core `cliff_edges` has any). */
function legalEntries(def: TileDef, needs: Needs): { o: Orientation; cliff: boolean }[] {
  const top = def.raisable ? MAX_LEVEL - def.top : 0;
  const out: { o: Orientation; cliff: boolean }[] = [];
  for (let level = 0; level <= top; level++)
    for (let rotation = 0; rotation < 6; rotation++)
      if (clashes(def, rotation, needs, level).length === 0)
        out.push({ o: { rotation, level }, cliff: hasCliff(def, rotation, needs, level) });
  out.sort((a, b) => Number(a.cliff) - Number(b.cliff) || a.o.level - b.o.level || a.o.rotation - b.o.rotation);
  return out;
}

/**
 * Rotations at which `def` fits a slot with these needs on `level` (the lowest level it fits on when
 * omitted: the ground first, as the ghost lands), ascending.
 */
export function legalRotations(def: TileDef, needs: Needs, level?: number): number[] {
  const all = legalOrientations(def, needs);
  const on = level ?? all[0]?.level;
  return all
    .filter((o) => o.level === on)
    .map((o) => o.rotation)
    .sort((a, b) => a - b);
}

/** The first rotation of each distinct orientation among `ks` (equal world-edge sockets count once). */
export function distinctRotations(def: TileDef, ks: readonly number[]): number[] {
  const seen = new Set<string>();
  return ks.filter((k) => {
    const key = orientation(def, k);
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

export const KIND_NAME: Record<SocketKind, string> = {
  Grass: "Grass",
  Forest: "Forest",
  Road: "Road",
  River: "River",
  Beach: "Beach",
  Ocean: "Ocean",
  Rail: "Rail",
};
const lower = (k: SocketKind): string => KIND_NAME[k].toLowerCase();
const cap = (s: string): string => s.charAt(0).toUpperCase() + s.slice(1);
const listOr = (xs: string[]): string => (xs.length === 1 ? xs[0]! : `${xs.slice(0, -1).join(", ")} or ${xs.at(-1)}`);

/** What may meet a need, the needed kind first ("Ocean or beach"), from the socket table. */
const accepting = (n: SocketKind): SocketKind[] =>
  [n, ...COMPAT[n].filter((k) => k !== n)].filter((k) => COMPAT[n].includes(k));

/** Why a tile cannot meet a socket kind at all. */
function missing(n: SocketKind): string {
  if (n === "River" || n === "Road" || n === "Rail" || n === "Beach")
    return `Needs a ${lower(n)} edge to meet the ${lower(n)}.`;
  return `Needs ${listOr(accepting(n).map(lower))} to meet the ${n === "Ocean" ? "sea" : lower(n)}.`;
}

/** Where a beach would end: "the sea" for ocean, else the kind ("the grass"). */
const end = (k: SocketKind): string => (k === "Ocean" ? "the sea" : `the ${lower(k)}`);

function clashPhrase(mine: SocketKind, theirs: SocketKind): string {
  if (mine === "Road") return `Its road would dead-end into the ${lower(theirs)}.`;
  if (mine === "River") return `Its river would run into the ${lower(theirs)}.`;
  if (mine === "Rail") return `Its railway would dead-end into the ${lower(theirs)}.`;
  if (theirs === "Road") return `The road here would dead-end into its ${lower(mine)}.`;
  if (theirs === "River") return `The river here would run into its ${lower(mine)}.`;
  if (theirs === "Rail") return `The railway here would dead-end into its ${lower(mine)}.`;
  // Beach meets only beach, with the land on opposite sides (owner O1): beach on beach is a side clash.
  if (mine === "Beach" && theirs === "Beach") return "The coast here runs the other way.";
  if (mine === "Beach") return `Its beach would end at ${end(theirs)}.`;
  if (theirs === "Beach") return `The beach here would end at its ${mine === "Ocean" ? "sea" : lower(mine)}.`;
  if (mine === "Ocean") return `Ocean can't meet ${lower(theirs)}. Coasts need a beach.`;
  if (theirs === "Ocean") return `${KIND_NAME[mine]} can't meet the ocean. Coasts need a beach.`;
  return `${KIND_NAME[mine]} can't meet ${lower(theirs)}.`;
}

export interface Fit {
  ok: boolean;
  /** Rotations legal on `level`, ascending. */
  legal: number[];
  /** First rotation of each distinct legal orientation (PRD §4.7 Q6 counts these). */
  distinct: number[];
  /** Why it does not fit, in the player's words; empty when it fits. */
  why: string;
  /** The rotation with the fewest clashing edges (what an unfitting ghost shows). */
  bestK: number;
  /** The level `legal` is read on: the one asked for, else the lowest it fits on. */
  level: number;
}

export function fit(def: TileDef, needs: Needs, level?: number): Fit {
  const all = legalOrientations(def, needs);
  const on = level ?? all[0]?.level ?? 0;
  const legal = legalRotations(def, needs, on);
  const distinct = distinctRotations(def, legal);
  if (all.length) return { ok: true, legal, distinct, why: "", bestK: legal[0] ?? all[0]!.rotation, level: on };
  let bestK = 0;
  let best = 7;
  for (let k = 0; k < 6; k++) {
    const n = clashes(def, k, needs).length;
    if (n < best) [best, bestK] = [n, k];
  }
  const kinds = [...new Set(needs.kinds.filter((n): n is SocketKind => n != null))];
  const lacking = kinds.find((n) => !def.edges.some((e) => compatible(n, e)));
  const w = clashes(def, bestK, needs)[0] ?? 0;
  const why = lacking ? missing(lacking) : clashPhrase(rotated(def, bestK)[w]!, needs.kinds[w]!);
  return { ok: false, legal, distinct: [], why, bestK, level: on };
}

/** Row meta for a fitting tile: "Fits any way round", "1 rotation fits", "3 rotations fit". */
export function fitMeta(def: TileDef, f: Fit): string {
  if (f.distinct.length === 1) return new Set(def.edges).size === 1 ? "Fits any way round" : "1 rotation fits";
  return `${f.distinct.length} rotations fit`;
}

/** A tile, a rotation and a level: one step of what R walks through for a picker entry. */
export interface Turn {
  def: TileDef;
  k: number;
  level: number;
}

export interface EntryFit {
  /** Whether any member fits. */
  ok: boolean;
  /**
   * Every member's distinct legal orientations, cliff-free first, then by level, then member after
   * member, then by rotation (hex-core `group_orientations`): the steps R takes, every level included.
   */
  seq: Turn[];
  /** Why no member fits, from the member with the fewest clashes; empty when one fits. */
  why: string;
  /** What an unfitting row shows: that member at its least-clashing rotation. */
  best: Turn;
}

/** The fit of a picker entry: fitting when any member fits (the Legend's "Fits here" filter). */
export function fitEntry(entry: PickerEntry, needs: Needs): EntryFit {
  const fits = entry.tiles.map((def) => ({ def, f: fit(def, needs) }));
  const keyed = entry.tiles.flatMap((def, i) => {
    const reps: { def: TileDef; o: Orientation; cliff: boolean }[] = [];
    for (const { o, cliff } of legalEntries(def, needs))
      if (!reps.some((r) => r.o.level === o.level && orientation(def, r.o.rotation) === orientation(def, o.rotation)))
        reps.push({ def, o, cliff });
    return reps.map((r) => ({ ...r, i }));
  });
  keyed.sort(
    (a, b) => Number(a.cliff) - Number(b.cliff) || a.o.level - b.o.level || a.i - b.i || a.o.rotation - b.o.rotation,
  );
  const seq = keyed.map((e) => ({ def: e.def, k: e.o.rotation, level: e.o.level }));
  if (seq.length) return { ok: true, seq, why: "", best: seq[0]! };
  const least = (def: TileDef, k: number) => clashes(def, k, needs).length;
  const worst = fits.reduce((a, b) => (least(b.def, b.f.bestK) < least(a.def, a.f.bestK) ? b : a));
  return { ok: false, seq, why: worst.f.why, best: { def: worst.def, k: worst.f.bestK, level: 0 } };
}

/** The steps of an entry's sequence on one level: what the rotation count reads (the first step's level when omitted). */
const onLevel = (f: EntryFit, level?: number): Turn[] => {
  const on = level ?? f.seq[0]?.level;
  return f.seq.filter((t) => t.level === on);
};

/** Row meta for a fitting entry, counting every member's distinct orientations on its lowest level together. */
export function entryMeta(entry: PickerEntry, f: EntryFit): string {
  const lone = entry.tiles.length === 1 ? entry.tiles[0]! : null;
  const n = onLevel(f).length;
  if (n === 1) return lone && new Set(lone.edges).size === 1 ? "Fits any way round" : "1 rotation fits";
  return `${n} rotations fit`;
}

/** Where the ghost's tile and rotation sit among an entry's orientations on `level` ("Rotation 4 of 5"). */
export function entryReadout(f: EntryFit, tile: string, k: number, level = 0): { index: number; count: number } {
  const steps = onLevel(f, level);
  const index = steps.findIndex((t) => t.def.id === tile && orientation(t.def, t.k) === orientation(t.def, k));
  return { index: index < 0 ? 0 : index, count: steps.length };
}

/** Row meta with no slot pointed at, over every member's edges. */
export function entrySummary(entry: PickerEntry): string {
  const first = entry.tiles[0]!;
  return summary({ ...first, edges: entry.tiles.flatMap((t) => t.edges) });
}

const LINE_ORDER: SocketKind[] = ["River", "Road", "Rail", "Ocean", "Beach", "Forest", "Grass"];
/** Kinds that carry a lane: they meet only their own kind, so the key names them first. */
const isLane = (k: SocketKind): boolean => k === "Road" || k === "River" || k === "Rail";

/**
 * What the slot key prints: lanes first ("River on one edge"), then what may sit beside them. A
 * stopped rail end (`stopped`, by world edge) takes a tile without rail as grass (owner,
 * 2026-10-25), so its line names both ("Rail, grass or forest on one edge"), after any rail line.
 */
export function needLines(needs: Kinds, stopped: readonly boolean[] = []): { kind: SocketKind; text: string }[] {
  const edges = needs.flatMap((n, w) => (n == null ? [] : [{ kind: n, stop: n === "Rail" && stopped[w] === true }]));
  const groups = LINE_ORDER.flatMap((k) =>
    [false, true].map((stop) => ({
      kind: k,
      stop,
      count: edges.filter((e) => e.kind === k && e.stop === stop).length,
    })),
  ).filter((g) => g.count > 0);
  const hasLane = groups.some((g) => isLane(g.kind));
  return groups.map(({ kind: n, stop, count }) => {
    const where = count === 1 ? "on one edge" : `on ${count} edges`;
    if (stop)
      return { kind: n, text: `${listOr([KIND_NAME[n], ...accepting(STOPPED_END_GROUND).map(lower)])} ${where}` };
    if (isLane(n)) return { kind: n, text: `${KIND_NAME[n]} ${where}` };
    return { kind: n, text: `${cap(listOr(accepting(n).map(lower)))} ${hasLane ? "beside it" : where}` };
  });
}

const CARRY: Record<SocketKind, string> = {
  River: "a river",
  Road: "a road",
  Rail: "a railway",
  Ocean: "the sea",
  Beach: "a beach",
  Forest: "forest",
  Grass: "grass",
};

/** Why nothing fits a slot: two of its needs no single tile meets together, when there are two. */
export function nothingFitsWhy(needs: Kinds): string {
  const kinds = LINE_ORDER.filter((k) => needs.includes(k));
  const meets = (t: TileDef, n: SocketKind) => t.edges.some((e) => compatible(n, e));
  for (const [i, a] of kinds.entries())
    for (const b of kinds.slice(i + 1))
      if (!TILES.some((t) => meets(t, a) && meets(t, b))) return `No tile carries both ${CARRY[a]} and ${CARRY[b]}.`;
  return "No tile matches every edge here.";
}

/** Row meta with no slot pointed at: "Road and grass edges", "Grass all round". */
export function summary(def: TileDef): string {
  const ks = LINE_ORDER.filter((k) => def.edges.includes(k)).map(lower);
  if (ks.length === 1) return `${cap(ks[0]!)} all round`;
  const txt = ks.length === 2 ? `${ks[0]} and ${ks[1]}` : `${ks.slice(0, -1).join(", ")} and ${ks.at(-1)}`;
  return `${cap(txt)} edges`;
}

/** Where the ghost's rotation sits among the distinct legal orientations ("Rotation 2 of 3"). */
export function rotationReadout(def: TileDef, legal: readonly number[], k: number): { index: number; count: number } {
  const distinct = distinctRotations(def, legal);
  const key = orientation(def, k);
  const index = distinct.findIndex((d) => orientation(def, d) === key);
  return { index: index < 0 ? 0 : index, count: distinct.length };
}

/** The only tile that may cover a tunnel (hex-core `tunnel::COVER_DEF`). */
const COVER_DEF = "meadow";

/**
 * The cells under the cover of a complete tunnel (hex-core `tunnel::tunnels`): two portals on one
 * level face each other along a straight line, every cell between them a meadow one level higher.
 * Such a cell can be neither bulldozed nor replaced.
 */
export function tunnelCover(tiles: TileMap): Set<string> {
  const cover = new Set<string>();
  for (const a of tiles.values()) {
    const mouth = tileDef(a.tile)?.tunnel;
    if (mouth == null) continue;
    const edge = (mouth + a.rotation) % 6;
    const between: string[] = [];
    let at = neighbor(a, edge);
    let n = tiles.get(cellKey(at));
    while (n && n.tile === COVER_DEF && n.level === a.level + 1) {
      between.push(cellKey(at));
      at = neighbor(at, edge);
      n = tiles.get(cellKey(at));
    }
    const far = n && tileDef(n.tile)?.tunnel;
    if (n && far != null && n.level === a.level && (far + n.rotation) % 6 === (edge + 3) % 6)
      for (const k of between) cover.add(k);
  }
  return cover;
}

/** Whether `cell` is under the cover of a complete tunnel (hex-core `tunnel::is_cover`). */
export const isCover = (tiles: TileMap, cell: Cell): boolean => tunnelCover(tiles).has(cellKey(cell));

/**
 * Tiles that removing `target` would cut off: everything outside the largest remaining part. A tunnel
 * cover cell is refused before any connectivity check (hex-core `BulldozeError::TunnelCover`), so
 * nothing is cut off by it.
 */
export function cutOff(tiles: TileMap, target: Cell): PlayTile[] {
  if (isCover(tiles, target)) return [];
  const rest = new Map(tiles);
  rest.delete(cellKey(target));
  const seen = new Set<string>();
  const parts: PlayTile[][] = [];
  for (const [key, t] of rest) {
    if (seen.has(key)) continue;
    const part: PlayTile[] = [];
    const stack = [t];
    seen.add(key);
    while (stack.length) {
      const c = stack.pop()!;
      part.push(c);
      for (let w = 0; w < 6; w++) {
        const nk = cellKey(neighbor(c, w));
        const n = rest.get(nk);
        if (n && !seen.has(nk)) {
          seen.add(nk);
          stack.push(n);
        }
      }
    }
    parts.push(part);
  }
  parts.sort((a, b) => b.length - a.length);
  return parts.slice(1).flat();
}

export interface TileDiff {
  added: PlayTile[];
  removed: PlayTile[];
}

export function diffTiles(before: TileMap, after: TileMap): TileDiff {
  const added = [...after.values()].filter((t) => before.get(cellKey(t))?.tile !== t.tile);
  const removed = [...before.values()].filter((t) => after.get(cellKey(t))?.tile !== t.tile);
  return { added, removed };
}

/**
 * The angle (degrees) to turn a socket hex or glyph to for rotation k, from the angle it shows now:
 * the nearest equivalent, so a CSS transition from rotation 5 to 0 turns 60°, not 300° the long way.
 */
export function turnAngle(current: number, k: number): number {
  const d = ((((k * 60 - current) % 360) + 540) % 360) - 180;
  return current + d;
}

/**
 * Whether the ghost tag shows the R hint: its readout counts more than one rotation, or R's walk
 * (`seqLength` orientations across every level and group member) has somewhere else to go.
 */
export const turnHint = (count: number, seqLength: number): boolean => count > 1 || seqLength > 1;
