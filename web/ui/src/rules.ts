// The placement rules the sheet reads (PRD §4.1–4.2), from the Rust catalog and socket table in
// bridge-schema.json. hex-core stays the authority: test/rules.test.ts checks legalRotations
// against hex-core's own answers for every slot of two scripted worlds (rules-parity.json).
import type { Cell, PlayTile, SocketKind } from "./bridge.ts";
import schema from "./bridge-schema.json" with { type: "json" };

export interface TileDef {
  id: string;
  edges: SocketKind[];
}

export const TILES: TileDef[] = schema.catalog.tiles.map((t) => ({ id: t.id, edges: t.edges as SocketKind[] }));
export const START_TILE: string = schema.catalog.start;
const COMPAT = schema.catalog.compatible as Record<SocketKind, SocketKind[]>;
const BY_ID = new Map(TILES.map((t) => [t.id, t]));

export const tileDef = (id: string): TileDef | undefined => BY_ID.get(id);

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

export type TileMap = Map<string, PlayTile>;
export const tileMap = (tiles: readonly PlayTile[]): TileMap => new Map(tiles.map((t) => [cellKey(t), t]));

/** What each world edge of `slot` touches: the neighbour's facing socket, or null. */
export type Needs = (SocketKind | null)[];
export function needsAt(slot: Cell, tiles: TileMap): Needs {
  return [0, 1, 2, 3, 4, 5].map((w) => {
    const n = tiles.get(cellKey(neighbor(slot, w)));
    const def = n && tileDef(n.tile);
    return def ? rotated(def, n.rotation)[(w + 3) % 6]! : null;
  });
}

export const isSlot = (c: Cell, tiles: TileMap): boolean =>
  !tiles.has(cellKey(c)) && [0, 1, 2, 3, 4, 5].some((w) => tiles.has(cellKey(neighbor(c, w))));

const clashes = (r: SocketKind[], needs: Needs): number[] =>
  [0, 1, 2, 3, 4, 5].filter((w) => needs[w] != null && !compatible(needs[w]!, r[w]!));

/** Rotations 0..6 at which `def` fits a slot with these needs, ascending (hex-core legal_rotations). */
export function legalRotations(def: TileDef, needs: Needs): number[] {
  return [0, 1, 2, 3, 4, 5].filter((k) => clashes(rotated(def, k), needs).length === 0);
}

/** The first rotation of each distinct orientation among `ks` (equal world-edge sockets count once). */
export function distinctRotations(def: TileDef, ks: readonly number[]): number[] {
  const seen = new Set<string>();
  return ks.filter((k) => {
    const key = rotated(def, k).join();
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
};
const lower = (k: SocketKind): string => KIND_NAME[k].toLowerCase();
const cap = (s: string): string => s.charAt(0).toUpperCase() + s.slice(1);
const listOr = (xs: string[]): string => (xs.length === 1 ? xs[0]! : `${xs.slice(0, -1).join(", ")} or ${xs.at(-1)}`);

/** What may meet a need, the needed kind first ("Ocean or beach"), from the socket table. */
const accepting = (n: SocketKind): SocketKind[] =>
  [n, ...COMPAT[n].filter((k) => k !== n)].filter((k) => COMPAT[n].includes(k));

/** Why a tile cannot meet a socket kind at all. */
function missing(n: SocketKind): string {
  if (n === "River" || n === "Road") return `Needs a ${lower(n)} edge to meet the ${lower(n)}.`;
  return `Needs ${listOr(accepting(n).map(lower))} to meet the ${n === "Ocean" ? "sea" : lower(n)}.`;
}

function clashPhrase(mine: SocketKind, theirs: SocketKind): string {
  if (mine === "Road") return `Its road would dead-end into the ${lower(theirs)}.`;
  if (mine === "River") return `Its river would run into the ${lower(theirs)}.`;
  if (theirs === "Road") return `The road here would dead-end into its ${lower(mine)}.`;
  if (theirs === "River") return `The river here would run into its ${lower(mine)}.`;
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
    const n = clashes(rotated(def, k), needs).length;
    if (n < best) [best, bestK] = [n, k];
  }
  const kinds = [...new Set(needs.filter((n): n is SocketKind => n != null))];
  const lacking = kinds.find((n) => !def.edges.some((e) => compatible(n, e)));
  const w = clashes(rotated(def, bestK), needs)[0] ?? 0;
  const why = lacking ? missing(lacking) : clashPhrase(rotated(def, bestK)[w]!, needs[w]!);
  return { ok: false, legal, distinct: [], why, bestK };
}

/** Row meta for a fitting tile: "Fits any way round", "1 rotation fits", "3 rotations fit". */
export function fitMeta(def: TileDef, f: Fit): string {
  if (f.distinct.length === 1) return new Set(def.edges).size === 1 ? "Fits any way round" : "1 rotation fits";
  return `${f.distinct.length} rotations fit`;
}

const LINE_ORDER: SocketKind[] = ["River", "Road", "Ocean", "Beach", "Forest", "Grass"];

/** What the slot key prints: lanes first ("River on one edge"), then what may sit beside them. */
export function needLines(needs: Needs): { kind: SocketKind; text: string }[] {
  const kinds = needs.filter((n): n is SocketKind => n != null);
  const uniq = LINE_ORDER.filter((k) => kinds.includes(k));
  const hasLane = uniq.some((k) => k === "Road" || k === "River");
  const hasRiver = uniq.includes("River");
  return uniq.map((n) => {
    const count = kinds.filter((x) => x === n).length;
    const where = count === 1 ? "on one edge" : `on ${count} edges`;
    if (n === "Road" || n === "River") return { kind: n, text: `${KIND_NAME[n]} ${where}` };
    // A beach can no longer sit beside a river (directional beach sockets, PRD §5.2), so the key
    // never offers it there, ahead of the socket table catching up.
    const accepts = accepting(n).filter((k) => !(hasRiver && k === "Beach"));
    return { kind: n, text: `${cap(listOr(accepts.map(lower)))} ${hasLane ? "beside it" : where}` };
  });
}

const CARRY: Record<SocketKind, string> = {
  River: "a river",
  Road: "a road",
  Ocean: "the sea",
  Beach: "a beach",
  Forest: "forest",
  Grass: "grass",
};

/** Why nothing fits a slot: two of its needs no single tile meets together, when there are two. */
export function nothingFitsWhy(needs: Needs): string {
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
  const key = rotated(def, k).join();
  const index = distinct.findIndex((d) => rotated(def, d).join() === key);
  return { index: index < 0 ? 0 : index, count: distinct.length };
}

/**
 * Engine rotate steps (each moves to the next legal rotation, wrapping) until the ghost shows a
 * different orientation; 0 when every legal rotation looks the same.
 */
export function stepsToNextOrientation(def: TileDef, legal: readonly number[], k: number, back: boolean): number {
  if (!legal.length) return 1;
  const key = rotated(def, k).join();
  let cur = k;
  for (let step = 1; step <= 6; step++) {
    let next = cur;
    for (let s = 1; s <= 6; s++) {
      const c = back ? (cur + 6 - s) % 6 : (cur + s) % 6;
      if (legal.includes(c)) {
        next = c;
        break;
      }
    }
    if (rotated(def, next).join() !== key) return step;
    cur = next;
  }
  return 0;
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
