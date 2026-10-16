import type { Cell, Command, PlayState } from "./bridge.ts";
import { type TileMap, TILES, cellKey, fit, isSlot, needsAt } from "./rules.ts";
import { Sheet } from "./sheet.ts";

const sameCell = (a: Cell | null | undefined, b: Cell | null | undefined): boolean =>
  !!a && !!b && a.q === b.q && a.r === b.r;

export class SheetController {
  readonly sheet: Sheet;
  slot: Cell | null = null;
  turns = 0;
  open = true;
  hoverResent = false;
  private synced = false;
  private engineOpen: boolean | null = null;
  private signature = "";
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
    if (this.slot && !isSlot(this.slot, map)) this.slot = null;
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

  render(st: PlayState, map: TileMap, tilesRevision: number): void {
    const slot = this.slot;
    const ghostRotation = st.ghost && sameCell(st.ghost, slot) ? st.ghost.rotation : null;
    const signature = [tilesRevision, slot && cellKey(slot), st.selected, ghostRotation, st.tool, this.turns].join("|");
    if (signature === this.signature) return;
    this.signature = signature;
    const needs = slot ? needsAt(slot, map) : null;
    const nearest = slot && needs && !TILES.some((d) => fit(d, needs).ok) ? this.nearest(st, slot, map) : null;
    this.sheet.render({
      slot: slot && needs ? { cell: slot, needs } : null,
      selected: st.selected,
      ghostRotation,
      turns: this.turns,
      bulldoze: st.tool === "bulldoze",
      nearest,
    });
  }

  private nearest(st: PlayState, from: Cell, map: TileMap): { cell: Cell; fits: number } | null {
    let best: { cell: Cell; fits: number; d: number } | null = null;
    for (const cell of st.frontier) {
      if (sameCell(cell, from)) continue;
      const d =
        (Math.abs(cell.q - from.q) + Math.abs(cell.r - from.r) + Math.abs(cell.q + cell.r - from.q - from.r)) / 2;
      if (best && d >= best.d) continue;
      const needs = needsAt(cell, map);
      const fits = TILES.filter((tile) => fit(tile, needs).ok).length;
      if (fits) best = { cell, fits, d };
    }
    return best && { cell: best.cell, fits: best.fits };
  }
}
