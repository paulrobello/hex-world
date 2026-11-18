// Player-facing words: tile names, coordinates, and the toast and tag lines built from engine
// facts (the engine's own reason strings are for logs, not players).
import type { Cell, PlayTile, SocketKind } from "./bridge.ts";
import { tileDef, type TileDiff } from "./rules.ts";
import schema from "./bridge-schema.json" with { type: "json" };

/** Display names for every catalog id (test/rules.test.ts keeps it in step with the catalog). */
export const TILE_NAMES: Record<string, string> = {
  meadow: "Meadow",
  forest: "Pine forest",
  oak: "Oak",
  windmill: "Windmill",
  farm: "Farm",
  wheat_field: "Wheat field",
  house_cluster: "House cluster",
  bakery: "Bakery",
  road_straight: "Road straight",
  road_bend_120: "Road bend 120°",
  road_bend_60: "Road bend 60°",
  road_junction: "Road junction",
  // One picker entry, the Hillside group (P4): R turns it through every shape and level.
  hillside_000001: "Hillside",
  hillside_000011: "Hillside",
  hillside_000101: "Hillside",
  hillside_000111: "Hillside",
  hillside_001001: "Hillside",
  hillside_001011: "Hillside",
  hillside_001101: "Hillside",
  hillside_001111: "Hillside",
  hillside_010101: "Hillside",
  hillside_010111: "Hillside",
  hillside_011011: "Hillside",
  hillside_011111: "Hillside",
  massif_granite: "Granite massif",
  hill_meadow: "Grassy hill",
  peak_granite: "Granite peak",
  road_ramp: "Road ramp",
  road_tunnel_portal: "Tunnel portal",
  rail_straight: "Rail straight",
  rail_bend_60: "Rail bend 60°",
  rail_bend_120: "Rail bend 120°",
  rail_junction: "Rail junction",
  rail_road_crossing: "Level crossing",
  rail_station: "Rail station",
  rail_tunnel_portal: "Rail tunnel portal",
  rail_viaduct_l1: "Rail viaduct",
  rail_viaduct_l2: "Rail viaduct",
  rail_viaduct_l3: "Rail viaduct",
  river_straight: "River straight",
  river_bend: "River bend",
  river_bend_60: "River bend 60°",
  river_road_bridge: "River bridge",
  river_rail_bridge: "River rail bridge",
  river_spring: "River spring",
  river_fork: "River fork",
  beach: "Beach",
  // One picker entry, the Beach Curve (owner, 2026-10-20): R turns it between the headland and the bay.
  beach_outer: "Beach Curve",
  beach_inner: "Beach Curve",
  // One picker entry for both handednesses (owner, 2026-10-20): R flips it where the mirror fits.
  river_mouth: "River mouth",
  river_mouth_l: "River mouth",
  ocean: "Ocean",
};

/** Why a footprint tile (the massif) is refused at a slot no legal centre covers. */
export const FOOTPRINT_REFUSAL = "Needs seven free cells beside the world.";

/** Why a footprint tile is refused where seven free cells touch the world but an outer edge clashes. */
export const FOOTPRINT_EDGES = "Its edges don't meet the tiles beside it.";

/** Why a viaduct is refused where a deck end faces an empty cell: its deck spans to the rail it meets. */
export const DECK_END_REFUSAL = "Needs raised rail at its deck height on both ends.";

/** Why a single tile is refused at a cell the engine offers only to a massif. */
export const FOOTPRINT_INNER = "Only a massif can stand here.";

/** Why a single tile is refused over a massif (hex-core `ReplaceError::FootprintMismatch`). */
export const MASSIF_SWAP = "A massif can only be swapped for another massif.";

/** Why a massif is refused over a single tile (the same mismatch, the other way round). */
export const SINGLE_SWAP = "A massif can't replace a single tile.";

export const tileName = (id: string): string => TILE_NAMES[id] ?? id.replace(/_/g, " ");

const num = (n: number): string => (n < 0 ? `−${-n}` : String(n));
/** "(−1, 0)" with a true minus sign. */
export const coord = (c: Cell): string => `(${num(c.q)}, ${num(c.r)})`;

export const plural = (n: number, one: string, many = `${one}s`): string => `${n} ${n === 1 ? one : many}`;

/** What an undo or redo changed, from the tile diff across it: "placed River bend at (1, 0)". */
export function actionText(diff: TileDiff, isRedo: boolean): string {
  // Undoing a placement removes the tile; redoing it adds it back. Bulldozes run the other way.
  const placed = isRedo ? diff.added : diff.removed;
  const removed = isRedo ? diff.removed : diff.added;
  if (placed.length === 1 && !removed.length) {
    const t = placed[0]!;
    return `placed ${tileName(t.tile)} at ${coord(t)}`;
  }
  if (removed.length === 1 && !placed.length) return `removed ${tileName(removed[0]!.tile)}`;
  // A replace swaps the tile on one cell: undo brings the old one back, redo the new one.
  const [put, took] = [placed[0], removed[0]];
  if (placed.length === 1 && removed.length === 1 && put && took && put.q === took.q && put.r === took.r)
    return `replaced ${tileName(took.tile)} with ${tileName(put.tile)}`;
  return "the last change";
}

/** The refusal headline and its second line for a bulldoze target that cannot go. */
export function bulldozeRefusal(reason: string | null, cut: readonly PlayTile[]): { head: string; sub: string } {
  if (reason && /last tile/i.test(reason))
    return { head: "The world needs at least one tile", sub: "Place another before removing this one" };
  if (reason && /tunnel/i.test(reason))
    return { head: "This tile covers a tunnel", sub: "Remove one of its portals first" };
  if (reason && /split/i.test(reason)) {
    const kinds = new Set(cut.map((t) => t.tile));
    const what = kinds.size === 1 ? `${tileName(cut[0]!.tile).toLowerCase()} tile` : "tile";
    const n = cut.length;
    return {
      head: "Removing this would split your world",
      sub: n ? `${plural(n, what)} would be cut off` : "Part of it would be cut off",
    };
  }
  return { head: "This tile can't be removed", sub: reason ?? "" };
}

/** Socket kinds in hex-core declaration order (the order `enums.refusal` lists kind codes in). */
const KINDS: SocketKind[] = ["Grass", "Forest", "Road", "River", "Beach", "Ocean", "Rail"];
const COMPAT = schema.catalog.compatible as Record<SocketKind, SocketKind[]>;

/** What may meet a need, the needed kind first ("Ocean or beach") — rules.ts's `accepting`. */
const accepting = (n: SocketKind): SocketKind[] => [n, ...COMPAT[n].filter((k) => k !== n)];

/** Why a tile cannot meet a socket kind at all — rules.ts's `missing`, verbatim. */
const missingWord = (n: SocketKind): string => {
  if (n === "River" || n === "Road" || n === "Rail" || n === "Beach")
    return `Needs a ${n.toLowerCase()} edge to meet the ${n.toLowerCase()}.`;
  const all = accepting(n).map((k) => k.toLowerCase());
  const phrase = all.length === 1 ? all[0]! : `${all.slice(0, -1).join(", ")} or ${all.at(-1)}`;
  return `Needs ${phrase} to meet the ${n === "Ocean" ? "sea" : n.toLowerCase()}.`;
};

/** Where a beach would end: "the sea" for ocean, else the kind — rules.ts's `end`. */
const end = (k: SocketKind): string => (k === "Ocean" ? "the sea" : `the ${k.toLowerCase()}`);

/** Why a tile's edge kind cannot meet the placed edge it faces — rules.ts's `clashPhrase`, verbatim. */
const clashWord = (mine: SocketKind, theirs: SocketKind): string => {
  if (mine === "Road") return `Its road would dead-end into the ${theirs.toLowerCase()}.`;
  if (mine === "River") return `Its river would run into the ${theirs.toLowerCase()}.`;
  if (mine === "Rail") return `Its railway would dead-end into the ${theirs.toLowerCase()}.`;
  if (theirs === "Road") return `The road here would dead-end into its ${mine.toLowerCase()}.`;
  if (theirs === "River") return `The river here would run into its ${mine.toLowerCase()}.`;
  if (theirs === "Rail") return `The railway here would dead-end into its ${mine.toLowerCase()}.`;
  // Beach meets only beach, with the land on opposite sides (owner O1): beach on beach is a side clash.
  if (mine === "Beach" && theirs === "Beach") return "The coast here runs the other way.";
  if (mine === "Beach") return `Its beach would end at ${end(theirs)}.`;
  if (theirs === "Beach") return `The beach here would end at its ${mine === "Ocean" ? "sea" : mine.toLowerCase()}.`;
  if (mine === "Ocean") return `Ocean can't meet ${theirs.toLowerCase()}. Coasts need a beach.`;
  if (theirs === "Ocean") return `${mine} can't meet the ocean. Coasts need a beach.`;
  return `${mine} can't meet ${theirs.toLowerCase()}.`;
};

/**
 * Every refusal code's player-facing wording (ENH-002 step 2), copied from the mirror's own
 * phrases so nothing changes user-visibly. rules.test.ts keeps every schema code covered here.
 */
const REFUSAL_WORDS: Record<string, string> = {
  inner_slot: FOOTPRINT_INNER,
  deck_end: DECK_END_REFUSAL,
  footprint_edges: FOOTPRINT_EDGES,
  no_footprint_room: FOOTPRINT_REFUSAL,
};
for (const k of KINDS) REFUSAL_WORDS[`missing_${k.toLowerCase()}`] = missingWord(k);
for (const mine of KINDS)
  for (const theirs of KINDS)
    REFUSAL_WORDS[`clash_${mine.toLowerCase()}_${theirs.toLowerCase()}`] = clashWord(mine, theirs);

/** The wording for an engine refusal code, empty for one the sheet does not word yet. */
export const refusalWord = (code: string): string => REFUSAL_WORDS[code] ?? "";

/** Why a refused replace of a tunnel's cover cell cannot go. */
export const TUNNEL_COVER = "This tile covers a tunnel.";

/**
 * The tag text for a refused placement over a placed tile (the sheet mirror's `refusedWhy` chain,
 * ENH-002): a tunnel's cover, the engine's level reason verbatim, a single-tile/massif mismatch
 * from the two defs (MASSIF_SWAP, SINGLE_SWAP), then the engine's refusal code; "An edge doesn't
 * match its neighbour." last.
 */
export function replaceRefusal(
  selected: string,
  there: PlayTile | undefined,
  reason: string,
  covered: boolean,
  code: string | null,
): string {
  if (covered) return TUNNEL_COVER;
  if (reason.includes("cannot stand at level")) return `${reason[0]!.toUpperCase()}${reason.slice(1)}.`;
  const mine = tileDef(selected)?.footprint;
  const theirs = there ? tileDef(there.tile)?.footprint : undefined;
  if (theirs !== undefined && mine !== theirs) return theirs ? MASSIF_SWAP : SINGLE_SWAP;
  if (code) {
    const said = refusalWord(code);
    if (said) return said;
  }
  return "An edge doesn't match its neighbour.";
}
