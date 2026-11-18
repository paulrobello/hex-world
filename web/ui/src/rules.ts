// The catalog and the wording the sheet and tag read: tile definitions from bridge-schema.json,
// cell math, occupancy, the slot key's lines, the R readouts and the bulldoze preview. hex-core is
// the only implementation of placement legality; the sheet and tag read their fits from `play.fits`
// (ENH-002), so this file carries no placement rules of its own.
import type { Cell, MegaSlot, PlayTile, SocketKind } from "./bridge.ts";
import schema from "./bridge-schema.json" with { type: "json" };

/**
 * A network crossing at an edge (hex-core `LaneSpec`): `[offset, width]` in metres, then `deck`,
 * the levels a rail lane stands above its ground (a viaduct's deck ends; 0 elsewhere).
 */
export type Lane = readonly [offset: number, width: number, deck?: number];

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
  /** Its highest relative corner (hex-core `TileDef::top`): `level + top` stays within hex-core `MAX_LEVEL`. */
  top: number;
  /** The local edge a tunnel portal's mouth opens on (hex-core `TileDef::tunnel`), null on every other tile. */
  tunnel: number | null;
  /**
   * Whether it covers its cell and the six around it (hex-core `TileDef::is_mega`). Such a tile never
   * fits a frontier slot: where it fits comes from the engine's `play.fits` entries.
   */
  footprint: boolean;
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
  footprint: t.footprint,
}));
export const START_TILE: string = schema.catalog.start;
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

/** How many levels a tile's highest corner stands above its own level (hillside, ramp: 1; flat: 0). */
export const tileRise = (id: string): number => Math.max(0, ...(tileDef(id)?.heights.flat() ?? [0]));

/** World-edge land sides of a tile turned by k (a turn never changes a side). */
const rotatedLand = (def: TileDef, k: number): (Side | null)[] =>
  [0, 1, 2, 3, 4, 5].map((w) => def.land[(w - k + 12) % 6]!);

/** Equal for two rotations exactly when their world-edge sockets and land sides are equal, a whole number of periods apart. */
export const orientation = (def: TileDef, k: number): string =>
  `${k % def.period}|${rotated(def, k).join()}|${rotatedLand(def, k).join()}`;

/**
 * What covers a cell: a placed tile, or a ring cell of a footprint tile, which repeats its anchor's
 * tile, rotation and level at its own `q`, `r` and names its `anchor` and the `ring` direction
 * (world edge) from it.
 */
export interface Occupant extends PlayTile {
  member: { anchor: Cell; ring: number } | null;
}

/** Every covered cell, keyed by {@link cellKey}: each tile at its cell and a footprint's six members. */
export type TileMap = Map<string, Occupant>;
export function tileMap(tiles: readonly PlayTile[]): TileMap {
  const map: TileMap = new Map();
  for (const t of tiles) {
    map.set(cellKey(t), { ...t, member: null });
    if (!tileDef(t.tile)?.footprint) continue;
    const anchor = { q: t.q, r: t.r };
    for (let ring = 0; ring < 6; ring++) {
      const c = neighbor(t, ring);
      map.set(cellKey(c), { ...t, q: c.q, r: c.r, member: { anchor, ring } });
    }
  }
  return map;
}

/** The anchor of the tile covering `cell`: the cell itself, a member's anchor, or null when it is empty. */
const anchorOf = (tiles: TileMap, cell: Cell): Cell | null => {
  const o = tiles.get(cellKey(cell));
  return o ? (o.member?.anchor ?? { q: o.q, r: o.r }) : null;
};

export const isSlot = (c: Cell, tiles: TileMap): boolean =>
  !tiles.has(cellKey(c)) && [0, 1, 2, 3, 4, 5].some((w) => tiles.has(cellKey(neighbor(c, w))));

/**
 * Whether the Tiles sheet may sit on `c`: a frontier slot, or a legal centre of the selected footprint
 * tile (`play.mega_slots`), which can lie further out.
 */
export const isSheetSlot = (c: Cell, tiles: TileMap, megaSlots: readonly MegaSlot[], selected: string): boolean =>
  isSlot(c, tiles) || megaSlots.some((m) => m.tile === selected && m.q === c.q && m.r === c.r);

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

/** A tile, a rotation and a level: one step of what R walks through for a picker entry. */
export interface Turn {
  def: TileDef;
  k: number;
  level: number;
}

/** Row meta for a fitting entry, counting every member's distinct steps on its first step's level together. */
export function entryMeta(entry: PickerEntry, steps: readonly Turn[]): string {
  const lone = entry.tiles.length === 1 ? entry.tiles[0]! : null;
  const n = steps.filter((t) => t.level === steps[0]?.level).length;
  if (n === 1) return lone && new Set(lone.edges).size === 1 ? "Fits any way round" : "1 rotation fits";
  return `${n} rotations fit`;
}

/** Row meta with no slot pointed at, over every member's edges. */
export function entrySummary(entry: PickerEntry): string {
  const first = entry.tiles[0]!;
  return summary({ ...first, edges: entry.tiles.flatMap((t) => t.edges) });
}

/** The socket kinds around a slot, by world edge, null where nothing touches it (`play.fits.needs`). */
export type Kinds = (SocketKind | null)[];

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
    if (a.member) continue;
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
 * Tiles that removing the tile covering `target` (a footprint whole) would cut off: every tile
 * outside the remaining part with the most tiles, connected over covered cells. A tunnel cover cell
 * is refused before any connectivity check (hex-core `BulldozeError::TunnelCover`), so nothing is
 * cut off by it.
 */
export function cutOff(tiles: TileMap, target: Cell): PlayTile[] {
  if (isCover(tiles, target)) return [];
  const gone = anchorOf(tiles, target);
  const goneKey = gone && cellKey(gone);
  const rest = new Map([...tiles].filter(([, t]) => cellKey(t.member?.anchor ?? t) !== goneKey));
  const seen = new Set<string>();
  const parts: PlayTile[][] = [];
  for (const [key, t] of rest) {
    if (seen.has(key)) continue;
    const part: PlayTile[] = [];
    const stack = [t];
    seen.add(key);
    while (stack.length) {
      const c = stack.pop()!;
      if (!c.member) part.push(tileOf(c));
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

/** The placed tile an anchor's occupant stands for. */
const tileOf = ({ member: _, ...t }: Occupant): PlayTile => t;

export function diffTiles(before: TileMap, after: TileMap): TileDiff {
  const tiles = (m: TileMap) => [...m.values()].filter((t) => !t.member);
  const added = tiles(after)
    .filter((t) => before.get(cellKey(t))?.tile !== t.tile)
    .map(tileOf);
  const removed = tiles(before)
    .filter((t) => after.get(cellKey(t))?.tile !== t.tile)
    .map(tileOf);
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
