// Player-facing words: tile names, coordinates, and the toast and tag lines built from engine
// facts (the engine's own reason strings are for logs, not players).
import type { Cell, PlayTile } from "./bridge.ts";
import type { TileDiff } from "./rules.ts";

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
