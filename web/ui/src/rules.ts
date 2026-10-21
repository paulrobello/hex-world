// The placement rules the sheet reads (PRD §4.1–4.2), from the Rust catalog and socket table in
// bridge-schema.json. hex-core stays the authority: test/rules.test.ts checks legalRotations
// against hex-core's own answers for every slot of four scripted worlds (rules-parity.json).
import type { Cell, PlayTile, SocketKind } from "./bridge.ts";
import schema from "./bridge-schema.json" with { type: "json" };

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
   * Rotations after which its look repeats (hex-core `orientation_period`): 6 with hero props, which
   * turn with it, 3 for the wheat field's rows, 1 where the sockets alone decide (`same_orientation`).
   */
  period: number;
}

export const TILES: TileDef[] = schema.catalog.tiles.map((t) => ({
  id: t.id,
  edges: t.edges as SocketKind[],
  land: t.land as (Side | null)[],
  period: t.period,
}));
export const START_TILE: string = schema.catalog.start;
const COMPAT = schema.catalog.compatible as Record<SocketKind, SocketKind[]>;
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

/** Equal for two rotations exactly when their world-edge sockets are equal and they are a whole number of periods apart. */
const orientation = (def: TileDef, k: number): string =>
  `${k % def.period}|${rotated(def, k).join()}|${rotatedLand(def, k).join()}`;

/**
 * Whether my edge may meet the edge it faces (hex-core `edges_compatible` for MVP edges): the kinds
 * are compatible, and directional edges have opposite land sides (both absent, or Left on Right).
 */
export const meets = (theirs: SocketKind, theirLand: Side | null, mine: SocketKind, myLand: Side | null): boolean =>
  compatible(theirs, mine) && (theirLand === null || myLand === null ? theirLand === myLand : theirLand !== myLand);

export type TileMap = Map<string, PlayTile>;
export const tileMap = (tiles: readonly PlayTile[]): TileMap => new Map(tiles.map((t) => [cellKey(t), t]));

/** The socket kinds around a slot, by world edge, null where nothing touches it. */
export type Kinds = (SocketKind | null)[];

/** What each world edge of a slot touches: the neighbour's facing socket kind and its land side. */
export interface Needs {
  kinds: Kinds;
  land: (Side | null)[];
}

export function needsAt(slot: Cell, tiles: TileMap): Needs {
  const facing = [0, 1, 2, 3, 4, 5].map((w) => {
    const n = tiles.get(cellKey(neighbor(slot, w)));
    const def = n && tileDef(n.tile);
    const back = (w + 3) % 6;
    return def ? { kind: rotated(def, n.rotation)[back]!, land: rotatedLand(def, n.rotation)[back]! } : null;
  });
  return { kinds: facing.map((f) => f?.kind ?? null), land: facing.map((f) => f?.land ?? null) };
}

export const isSlot = (c: Cell, tiles: TileMap): boolean =>
  !tiles.has(cellKey(c)) && [0, 1, 2, 3, 4, 5].some((w) => tiles.has(cellKey(neighbor(c, w))));

/** World edges where `def` turned by k does not meet what the slot touches. */
function clashes(def: TileDef, k: number, needs: Needs): number[] {
  const [r, l] = [rotated(def, k), rotatedLand(def, k)];
  return [0, 1, 2, 3, 4, 5].filter((w) => {
    const theirs = needs.kinds[w];
    return theirs != null && !meets(theirs, needs.land[w] ?? null, r[w]!, l[w]!);
  });
}

/** Rotations 0..6 at which `def` fits a slot with these needs, ascending (hex-core legal_rotations). */
export function legalRotations(def: TileDef, needs: Needs): number[] {
  return [0, 1, 2, 3, 4, 5].filter((k) => clashes(def, k, needs).length === 0);
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
  legal: number[];
  /** First rotation of each distinct legal orientation (PRD §4.7 Q6 counts these). */
  distinct: number[];
  /** Why it does not fit, in the player's words; empty when it fits. */
  why: string;
  /** The rotation with the fewest clashing edges (what an unfitting ghost shows). */
  bestK: number;
}

export function fit(def: TileDef, needs: Needs): Fit {
  const legal = legalRotations(def, needs);
  if (legal.length) return { ok: true, legal, distinct: distinctRotations(def, legal), why: "", bestK: legal[0]! };
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
  return { ok: false, legal, distinct: [], why, bestK };
}

/** Row meta for a fitting tile: "Fits any way round", "1 rotation fits", "3 rotations fit". */
export function fitMeta(def: TileDef, f: Fit): string {
  if (f.distinct.length === 1) return new Set(def.edges).size === 1 ? "Fits any way round" : "1 rotation fits";
  return `${f.distinct.length} rotations fit`;
}

/** A tile and a rotation: one step of what R walks through for a picker entry. */
export interface Turn {
  def: TileDef;
  k: number;
}

export interface EntryFit {
  /** Whether any member fits. */
  ok: boolean;
  /** Every member's distinct legal orientations, member after member (hex-core group_orientations). */
  seq: Turn[];
  /** Why no member fits, from the member with the fewest clashes; empty when one fits. */
  why: string;
  /** What an unfitting row shows: that member at its least-clashing rotation. */
  best: Turn;
}

/** The fit of a picker entry: fitting when any member fits (the Legend's "Fits here" filter). */
export function fitEntry(entry: PickerEntry, needs: Needs): EntryFit {
  const fits = entry.tiles.map((def) => ({ def, f: fit(def, needs) }));
  const seq = fits.flatMap(({ def, f }) => f.distinct.map((k) => ({ def, k })));
  if (seq.length) return { ok: true, seq, why: "", best: seq[0]! };
  const least = (def: TileDef, k: number) => clashes(def, k, needs).length;
  const worst = fits.reduce((a, b) => (least(b.def, b.f.bestK) < least(a.def, a.f.bestK) ? b : a));
  return { ok: false, seq, why: worst.f.why, best: { def: worst.def, k: worst.f.bestK } };
}

/** Row meta for a fitting entry, counting every member's distinct orientations together. */
export function entryMeta(entry: PickerEntry, f: EntryFit): string {
  const lone = entry.tiles.length === 1 ? entry.tiles[0]! : null;
  if (f.seq.length === 1) return lone && new Set(lone.edges).size === 1 ? "Fits any way round" : "1 rotation fits";
  return `${f.seq.length} rotations fit`;
}

/** Where the ghost's tile and rotation sit in an entry's sequence ("Rotation 4 of 5"). */
export function entryReadout(f: EntryFit, tile: string, k: number): { index: number; count: number } {
  const index = f.seq.findIndex((t) => t.def.id === tile && orientation(t.def, t.k) === orientation(t.def, k));
  return { index: index < 0 ? 0 : index, count: f.seq.length };
}

/** Row meta with no slot pointed at, over every member's edges. */
export function entrySummary(entry: PickerEntry): string {
  const first = entry.tiles[0]!;
  return summary({ ...first, edges: entry.tiles.flatMap((t) => t.edges) });
}

const LINE_ORDER: SocketKind[] = ["River", "Road", "Rail", "Ocean", "Beach", "Forest", "Grass"];
/** Kinds that carry a lane: they meet only their own kind, so the key names them first. */
const isLane = (k: SocketKind): boolean => k === "Road" || k === "River" || k === "Rail";

/** What the slot key prints: lanes first ("River on one edge"), then what may sit beside them. */
export function needLines(needs: Kinds): { kind: SocketKind; text: string }[] {
  const kinds = needs.filter((n): n is SocketKind => n != null);
  const uniq = LINE_ORDER.filter((k) => kinds.includes(k));
  const hasLane = uniq.some(isLane);
  return uniq.map((n) => {
    const count = kinds.filter((x) => x === n).length;
    const where = count === 1 ? "on one edge" : `on ${count} edges`;
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

/** Tiles that removing `target` would cut off: everything outside the largest remaining part. */
export function cutOff(tiles: TileMap, target: Cell): PlayTile[] {
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
