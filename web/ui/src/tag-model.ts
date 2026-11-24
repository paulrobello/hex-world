// What the tag beside the ghost or the bulldoze target says (hud.ts draws it): six states, one
// signature string and the parts of its line as plain descriptors. Free of the DOM so Node tests
// read it directly; the HUD turns `parts` into elements and skips the write while `sig` is unchanged.
import type { Cell, PlayState } from "./bridge.ts";
import {
  type TileMap,
  cellKey,
  cutOff,
  entryOf,
  orientation,
  rotationReadout,
  tileDef,
  turnHint,
  type Turn,
} from "./rules.ts";
import { ghostCovers } from "./sheet-controller.ts";
import { bulldozeRefusal, plural, refusalWord, tileName } from "./words.ts";

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
  if (st.tool === "place" && st.ghost) return ghostTag(st, st.ghost, refused);
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

function ghostTag(st: PlayState, g: NonNullable<PlayState["ghost"]>, refusal: RefusalNote | null): TagModel {
  const def = tileDef(g.tile);
  const entry = entryOf(g.tile);
  // The engine publishes fits for its target slot — the ghost's cell — so the entry's steps and
  // refusal come from there; a patch for the previous slot lags by one frame and falls back to the
  // ghost's own legal rotations for that frame.
  const e = st.fits && sameCell(st.fits.slot, g) ? st.fits.entries.find((x) => x.id === entry?.id) : undefined;
  const steps: Turn[] = e ? e.orientations.map(([id, k, level]) => ({ def: tileDef(id)!, k, level })) : [];
  const ok = e ? steps.length > 0 : g.legal_rotations.length > 0;
  const name = tileName(g.tile);
  if (ok && g.valid) {
    // R turns the ghost's member on its level, T changes the member and E/Q the level (owner
    // 2026-11-24), so a group (the Beach Curve) counts only the current member's steps here.
    const atLevel = e ? e.orientations.filter(([, , l]) => l === g.level) : [];
    const own = atLevel.filter(([id]) => id === g.tile);
    const { index, count } =
      entry && e && entry.tiles.length > 1
        ? groupReadout(own, g.tile, g.rotation)
        : rotationReadout(def!, g.legal_rotations, g.rotation);
    // Each hint shows while its key has somewhere else to go.
    const canTurn = turnHint(count, own.length);
    const canShape = !!entry && entry.tiles.length > 1 && atLevel.some(([id]) => id !== g.tile);
    const canLift = steps.some((s) => s.level !== g.level);
    const fits = count === 1 ? "Fits" : `Rotation ${index + 1} of ${count}`;
    const pinned = ghostCovers(g, st.pinned);
    const parts = [text(name), fitPart(fits, "plain")];
    if (g.level > 0) parts.push(fitPart(`Level ${g.level}`, "quiet"));
    if (canTurn) parts.push({ kind: "kbd", key: "R" });
    if (canShape) parts.push({ kind: "kbd", key: "T" });
    if (canLift) parts.push({ kind: "kbd", key: "E/Q" });
    if (pinned) parts.push(fitPart("Pinned · click to place", "quiet"));
    return {
      cell: g,
      level: g.level,
      sig: `ok|${cellKey(g)}|${g.tile}|${fits}|${g.level}|${canTurn}|${canShape}|${canLift}|${pinned}`,
      bad: false,
      stacked: false,
      parts,
    };
  }
  const clashes = (g.outer_edges ?? g.edges).filter((edge) => edge === "Incompatible").length;
  const why =
    refusal && sameCell(refusal.cell, g)
      ? refusal.text
      : e?.refusal
        ? refusalWord(e.refusal)
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

/** Where the ghost's rotation sits among its member's steps at its level ("Rotation 2 of 3"). */
function groupReadout(
  steps: readonly [member: string, rotation: number, level: number][],
  tile: string,
  k: number,
): { index: number; count: number } {
  const index = steps.findIndex(
    ([id, r]) => id === tile && orientation(tileDef(id)!, r) === orientation(tileDef(tile)!, k),
  );
  return { index: index < 0 ? 0 : index, count: steps.length };
}
