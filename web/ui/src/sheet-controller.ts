// Drives the tile sheet from engine state. The engine owns the pinned slot, the hovered slot and
// whether the inventory is open: `sync` adopts them from `play` state each frame and sends a command
// only to open the engine's inventory at first sync or to resend a hover.
import type { Cell, Command, Fits, Ghost, PlayState } from "./bridge.ts";
import { type TileMap, cellKey, isSheetSlot } from "./rules.ts";
import { Sheet } from "./sheet.ts";

const sameCell = (a: Cell | null | undefined, b: Cell | null | undefined): boolean =>
  !!a && !!b && a.q === b.q && a.r === b.r;

/** Whether the ghost stands for `slot`: on it, or as a footprint tile whose centre covers it. */
export const ghostCovers = (g: Ghost | null | undefined, slot: Cell | null | undefined): boolean =>
  !!g && (sameCell(g, slot) || !!g.footprint?.some((c) => sameCell(c, slot)));

export class SheetController {
  readonly sheet: Sheet;
  slot: Cell | null = null;
  turns = 0;
  open = true;
  hoverResent = false;
  private synced = false;
  private engineOpen: boolean | null = null;
  private signature = "";
  /** The last `play.fits` that matched the current slot, kept for the frame a patch has not caught up. */
  private lastFits: { key: string; value: Fits } | null = null;
  private readonly cmd: (command: Command | Command[]) => void;
  private readonly fire: () => void;

  constructor(cmd: (command: Command | Command[]) => void, select: (id: string) => void, fire: () => void) {
    this.cmd = cmd;
    this.fire = fire;
    this.sheet = new Sheet(
      select,
      () => this.toggle(),
      (cell) => this.goto(cell),
    );
  }

  toggle(): void {
    this.open = !this.open;
    this.sheet.open = this.open;
    this.engineOpen = this.open;
    this.cmd({ cmd: "toggle", what: "inventory" });
    this.fire();
  }

  goto(cell: Cell): void {
    this.slot = cell;
    this.hoverResent = true;
    this.cmd([
      { cmd: "hover", q: cell.q, r: cell.r },
      { cmd: "pin", q: cell.q, r: cell.r },
    ]);
  }

  reset(): void {
    this.slot = null;
    this.turns = 0;
  }

  /** Returns whether a new hovered slot advanced the first-run hint. */
  sync(st: PlayState, map: TileMap, pointerOff: boolean): boolean {
    if (!this.synced) {
      // The sheet opens by default; the engine's initial inventory flag is closed.
      this.synced = true;
      this.engineOpen = st.inventory_open;
      if (!st.inventory_open) this.cmd({ cmd: "toggle", what: "inventory" });
      this.sheet.open = true;
    } else if (st.inventory_open !== this.engineOpen) {
      this.engineOpen = st.inventory_open;
      this.open = st.inventory_open;
      this.sheet.open = this.open;
    }
    let hovered = false;
    if (st.pinned) {
      if (!sameCell(st.pinned, this.slot)) {
        this.slot = st.pinned;
        this.turns = 0;
      }
    } else if (st.tool === "place" && st.hovered_kind === "slot" && st.hovered && !sameCell(st.hovered, this.slot)) {
      this.slot = st.hovered;
      this.turns = 0;
      this.hoverResent = false;
      hovered = true;
    }
    if (this.slot && !isSheetSlot(this.slot, map, st.mega_slots, st.selected)) this.slot = null;
    if (
      !st.pinned &&
      !st.walk_mode &&
      pointerOff &&
      !this.hoverResent &&
      this.slot &&
      st.tool === "place" &&
      !st.hovered
    ) {
      this.hoverResent = true;
      this.cmd({ cmd: "hover", q: this.slot.q, r: this.slot.r });
    }
    return hovered;
  }

  render(st: PlayState, tilesRevision: number): void {
    const slot = this.slot;
    const onSlot = ghostCovers(st.ghost, slot) ? st.ghost : null;
    const ghostRotation = onSlot ? onSlot.rotation : null;
    const ghostTile = onSlot ? onSlot.tile : null;
    const ghostLevel = onSlot ? onSlot.level : 0;
    // The engine publishes fits for its own target slot; a patch for the previous slot lags the
    // sheet's slot by one frame, so the last matching fits are kept until the new one lands.
    if (st.fits && slot && sameCell(st.fits.slot, slot)) this.lastFits = { key: cellKey(slot), value: st.fits };
    const fits = slot && this.lastFits?.key === cellKey(slot) ? this.lastFits.value : null;
    const signature = [
      tilesRevision,
      slot && cellKey(slot),
      st.selected,
      ghostTile,
      ghostRotation,
      ghostLevel,
      st.tool,
      this.turns,
      fits ? "fits" : "-",
    ].join("|");
    if (signature === this.signature) return;
    this.signature = signature;
    this.sheet.render({
      slot: slot && fits ? { cell: slot, needs: fits.needs, stopped: fits.stopped } : null,
      selected: st.selected,
      ghostTile,
      ghostRotation,
      ghostLevel,
      turns: this.turns,
      bulldoze: st.tool === "bulldoze",
      nearest: fits?.nearest ? { cell: { q: fits.nearest.q, r: fits.nearest.r }, fits: fits.nearest.tiles } : null,
      entries: new Map(fits?.entries.map((e) => [e.id, e])),
    });
  }
}
