// What the tag beside the ghost or the bulldoze target says (hud.ts draws it): six states, one
// signature string and the parts of its line as plain descriptors. Free of the DOM so Node tests
// read it directly; the HUD turns `parts` into elements and skips the write while `sig` is unchanged.
import type { Cell, PlayState } from "./bridge.ts";
import {
  type TileMap,
  cellKey,
  cutOff,
  entryOf,
  entryReadout,
  fit,
  fitEntry,
  needsAt,
  rotationReadout,
  tileDef,
  turnHint,
} from "./rules.ts";
import { ghostCovers } from "./sheet-controller.ts";
import { bulldozeRefusal, plural, tileName } from "./words.ts";

/** A tile just placed (or swapped in), shown for a moment until `until` (ms on the HUD clock). */
export interface PlacedNote {
  cell: Cell;
  level: number;
  text: string;
  until: number;
}

/** A refused replace: the placed tile stays, and `text` says why the selected one cannot swap in. */
export interface RefusalNote {
  cell: Cell;
  tile: string;
  text: string;
  until: number;
}

/** One piece of the tag's line. `fit` is the small trailing phrase, toned like its CSS class. */
export type TagPart =
  | { kind: "icon"; name: string; cls?: string }
  | { kind: "text"; text: string }
  | { kind: "fit"; text: string; tone: "plain" | "quiet" | "bad" }
  | { kind: "stack"; head: string; sub: string }
  | { kind: "kbd"; key: string };

export interface TagModel {
  cell: Cell;
  /** The level the tag's anchor rect is read at. */
  level: number;
  /** Identifies what the tag shows: the HUD rewrites the tag only when it changes. */
  sig: string;
  bad: boolean;
  stacked: boolean;
  parts: TagPart[];
}

const sameCell = (a: Cell | null | undefined, b: Cell | null | undefined): boolean =>
  !!a && !!b && a.q === b.q && a.r === b.r;

const icon = (name: string, cls?: string): TagPart => (cls ? { kind: "icon", name, cls } : { kind: "icon", name });
const text = (t: string): TagPart => ({ kind: "text", text: t });
const fitPart = (t: string, tone: "plain" | "quiet" | "bad"): TagPart => ({ kind: "fit", text: t, tone });

/**
 * The tag for `st`, or null when nothing is pointed at. The first state that applies wins: a
 * bulldoze target (removable or refused), a tile just placed, a refused replace, the pointer on a
 * tunnel's cover, then the ghost (fits or does not). `placed` and `refusal` past their `until`
 * count as absent. `coverCells` is read only when the pointer is on a placed tile.
 */
export function tagModel(
  st: PlayState,
  map: TileMap,
  placed: PlacedNote | null,
  refusal: RefusalNote | null,
  coverCells: () => ReadonlySet<string>,
  now: number,
): TagModel | null {
  const note = placed && now <= placed.until ? placed : null;
  const refused = refusal && now <= refusal.until ? refusal : null;
  if (st.tool === "bulldoze" && st.bulldoze) return bulldozeTag(st.bulldoze, map);
  if (note && st.tool === "place" && (!st.ghost || sameCell(st.ghost, note.cell))) {
    return {
      cell: note.cell,
      level: note.level,
      sig: `placed|${cellKey(note.cell)}|${note.text}`,
      bad: false,
      stacked: false,
      parts: [icon("check", "ic ok-ic"), text(note.text)],
    };
  }
  if (refused && st.tool === "place" && map.has(cellKey(refused.cell))) {
    return {
      cell: refused.cell,
      level: map.get(cellKey(refused.cell))?.level ?? 0,
      sig: `swap|${cellKey(refused.cell)}|${refused.tile}|${refused.text}`,
      bad: true,
      stacked: false,
      parts: [icon("alert"), text(tileName(refused.tile)), fitPart(refused.text, "bad")],
    };
  }
  if (
    st.tool === "place" &&
    st.hovered_kind === "tile" &&
    st.hovered &&
    !(st.ghost && sameCell(st.pinned, st.ghost)) &&
    coverCells().has(cellKey(st.hovered))
  ) {
    // The pointer is on a tunnel's cover: a replace here is refused, so say so before the click.
    const cell = st.hovered;
    return {
      cell,
      level: map.get(cellKey(cell))?.level ?? 0,
      sig: `cover|${cellKey(cell)}`,
      bad: true,
      stacked: false,
      parts: [icon("alert"), text(tileName(map.get(cellKey(cell))?.tile ?? "")), fitPart("Covers a tunnel", "bad")],
    };
  }
  if (st.tool === "place" && st.ghost) return ghostTag(st, st.ghost, map, refused);
  return null;
}

function bulldozeTag(b: NonNullable<PlayState["bulldoze"]>, map: TileMap): TagModel {
  const t = map.get(cellKey(b));
  const level = t?.level ?? 0;
  if (b.allowed) {
    return {
      cell: b,
      level,
      sig: `rm|${cellKey(b)}`,
      bad: false,
      stacked: false,
      parts: [icon("bulldoze"), text(`Remove ${tileName(t?.tile ?? "")}`), fitPart("Click", "quiet")],
    };
  }
  const r = bulldozeRefusal(b.reason, cutOff(map, b));
  return {
    cell: b,
    level,
    sig: `no|${cellKey(b)}|${r.sub}`,
    bad: true,
    stacked: true,
    parts: [icon("alert"), { kind: "stack", head: r.head, sub: r.sub }],
  };
}

function ghostTag(
  st: PlayState,
  g: NonNullable<PlayState["ghost"]>,
  map: TileMap,
  refusal: RefusalNote | null,
): TagModel {
  const def = tileDef(g.tile);
  const needs = needsAt(g, map);
  const f = def ? fit(def, needs, g.level) : null;
  const entry = entryOf(g.tile);
  // A footprint ghost stands on its centre, which fits where the engine lists it in `mega_slots`.
  const entryFit =
    entry && (def?.footprint || f?.ok)
      ? fitEntry(entry, needs, { slot: g, megaSlots: st.mega_slots, tiles: map })
      : null;
  const ok = def?.footprint ? !!entryFit?.ok : !!f?.ok;
  const name = tileName(g.tile);
  if (ok && g.valid) {
    // A group (the Beach Curve) counts every member's orientations together, as R walks them.
    const { index, count } =
      entry && entryFit && entry.tiles.length > 1
        ? entryReadout(entryFit, g.tile, g.rotation, g.level)
        : rotationReadout(def!, g.legal_rotations, g.rotation);
    // R steps every level too, so the hint shows while any orientation is left to turn to.
    const canTurn = turnHint(count, entryFit?.seq.length ?? 0);
    const fits = count === 1 ? "Fits" : `Rotation ${index + 1} of ${count}`;
    const pinned = ghostCovers(g, st.pinned);
    const parts = [text(name), fitPart(fits, "plain")];
    if (g.level > 0) parts.push(fitPart(`Level ${g.level}`, "quiet"));
    if (canTurn) parts.push({ kind: "kbd", key: "R" });
    if (pinned) parts.push(fitPart("Pinned · click to place", "quiet"));
    return {
      cell: g,
      level: g.level,
      sig: `ok|${cellKey(g)}|${g.tile}|${fits}|${g.level}|${canTurn}|${pinned}`,
      bad: false,
      stacked: false,
      parts,
    };
  }
  const clashes = (g.outer_edges ?? g.edges).filter((e) => e === "Incompatible").length;
  const why =
    refusal && sameCell(refusal.cell, g)
      ? refusal.text
      : !ok && f
        ? def?.footprint && entryFit
          ? entryFit.why
          : f.why
        : `${plural(clashes, "edge")} clash`;
  const parts = [icon("alert"), text(name), fitPart(why, "bad")];
  if (ok) parts.push({ kind: "kbd", key: "R" });
  return {
    cell: g,
    level: g.level,
    sig: `bad|${cellKey(g)}|${g.tile}|${why}|${g.level}`,
    bad: true,
    stacked: false,
    parts,
  };
}
