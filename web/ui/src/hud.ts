// The play HUD (S2–S7): tool bar, Tiles sheet, the tag beside the ghost, undo/redo toasts, bulldoze,
// first-run callouts and keys. It reads hexWorld.state() and events every frame and acts only
// through bridge commands, so the engine stays the one owner of game state.
import type { Cell, Command, GameEvent, HexWorld, PlayState, Rect, State } from "./bridge.ts";
import { h, isMac, reduceMotion, retrigger, s } from "./dom.ts";
import { icon } from "./icons.ts";
import {
  TILES,
  type TileMap,
  cellKey,
  cutOff,
  diffTiles,
  fit,
  isSlot,
  needsAt,
  rotationReadout,
  tileDef,
  tileMap,
} from "./rules.ts";
import { Photo } from "./photo.ts";
import { Settings } from "./settings.ts";
import { Sheet } from "./sheet.ts";
import { actionText, bulldozeRefusal, plural, tileName } from "./words.ts";

const MOD = isMac() ? "⌘" : "Ctrl+";
const HINTS_KEY = "hexWorld.hints";
const TOAST_MS = 4000;
const PLACED_TAG_MS = 1100;
const ROTATE_REPEAT_MS = 250;

const sameCell = (a: Cell | null | undefined, b: Cell | null | undefined): boolean =>
  !!a && !!b && a.q === b.q && a.r === b.r;

function storeGet(k: string): string | null {
  try {
    return localStorage.getItem(k);
  } catch {
    return null;
  }
}
function storeSet(k: string, v: string): void {
  try {
    localStorage.setItem(k, v);
  } catch {
    /* Private windows may refuse storage; hints then show again next time. */
  }
}

interface Tool {
  btn: HTMLButtonElement;
}

export class Hud {
  private readonly root: HTMLElement;
  private readonly sheet: Sheet;
  private readonly tools = new Map<string, Tool>();
  private readonly tiles: HTMLElement;
  private readonly tag: HTMLElement;
  private readonly toast: HTMLElement;
  private readonly live: HTMLElement;
  private readonly tip: HTMLElement;
  private readonly callout: HTMLElement;
  private readonly leader: SVGElement;
  private readonly canvas: HTMLCanvasElement | null;
  private readonly photo: Photo;
  private readonly settings: Settings;

  private st: PlayState | null = null;
  private pending: GameEvent[] = [];
  private map: TileMap = new Map();
  private tilesRev = 0;
  private sheetSlot: Cell | null = null;
  private turns = 0;
  private sheetSig = "";
  private open = true;
  private synced = false;
  private engineOpen: boolean | null = null;
  private hidden = false;
  private pointerOff = true;
  private pinned = false;
  private lastRotate = 0;
  private placed: { cell: Cell; tile: string; until: number } | null = null;
  private refusal: { cell: Cell; text: string; until: number } | null = null;
  private toastState: { verb: string; count: number } | null = null;
  private toastTimer = 0;
  private tipTimer = 0;
  private hint = -1;
  private hintTimer = 0;

  private readonly hw: HexWorld;

  constructor(host: HTMLElement, hw: HexWorld) {
    this.hw = hw;
    this.root = host;
    this.canvas = document.querySelector<HTMLCanvasElement>("canvas#hex-world");
    this.sheet = new Sheet(
      (id) => this.selectTile(id),
      () => this.toggleSheet(),
      (c) => this.gotoSlot(c),
    );
    this.tiles = h("span", {}, "1 tile");
    this.tag = h("div", { class: "ghost-tag is-gone", "aria-hidden": "true" });
    this.toast = h("div", { class: "toast", "data-region": "toast", role: "status", "aria-live": "polite" });
    this.live = h("p", { class: "sr", "aria-live": "polite" });
    this.tip = h("div", { class: "tip", role: "tooltip" });
    this.callout = h("div", { class: "callout", role: "status" });
    this.leader = s("svg", { class: "leader", "aria-hidden": "true" }, s("path"), s("circle", { r: 4 }));
    const showUi = h("button", { class: "show-ui", type: "button" }, icon("eye"), "Show UI ", h("kbd", {}, "H"));
    showUi.addEventListener("click", () => this.setHidden(false));
    this.root.append(
      this.bar(),
      this.sheet.el,
      this.tag,
      this.toast,
      this.live,
      showUi,
      this.callout,
      this.leader,
      this.tip,
    );
    this.photo = new Photo(hw);
    this.settings = new Settings(hw, () => this.toggleSettings());
    this.root.append(this.settings.el);
    document.body.append(this.photo.root);
    this.wireInput();
    const done = Number(storeGet(HINTS_KEY) ?? 0);
    if (done < HINTS.length) this.hint = done;
  }

  // ---------- Commands ----------
  private cmd(c: Command | Command[]): void {
    this.hw.cmd(c);
  }

  private selectTile(id: string): void {
    this.cmd({ cmd: "select_tile", id });
  }

  private toggleSheet(): void {
    this.open = !this.open;
    this.sheet.open = this.open;
    this.engineOpen = this.open;
    this.cmd({ cmd: "toggle", what: "inventory" });
    this.fire("inventory");
  }

  private rotate(back: boolean): void {
    const st = this.st;
    if (!st) return;
    this.fire("rotate");
    // One press, one engine step: the engine skips rotations that repeat an orientation (PRD §4.7 Q6).
    this.turns += back ? -1 : 1;
    this.cmd({ cmd: "rotate", dir: back ? "ccw" : "cw" });
    this.advanceHint("rotate");
  }

  private gotoSlot(c: Cell): void {
    this.sheetSlot = c;
    this.pinned = true;
    this.cmd({ cmd: "hover", q: c.q, r: c.r });
  }

  // ---------- Tool bar ----------
  private bar(): HTMLElement {
    const world = h(
      "div",
      {
        class: "bar__world",
        "data-tip": "Worlds||Saving and named worlds are not built yet",
        "aria-label": "World, not saved",
      },
      h("b", {}, "World"),
      h(
        "span",
        {},
        this.tiles,
        " · ",
        h("span", { class: "save-state", "data-state": "off" }, icon("alert", "ic ic--xs"), "Not saved"),
      ),
    );
    const bar = h("nav", { class: "bar", "data-region": "bar", "aria-label": "Tools" }, world, rule());
    const add = (a: string, ic: string, label: string, key: string, tip: string, aria?: string) => {
      const btn = h("button", { class: "tool", type: "button", "data-action": a, "data-tip": tip, "aria-label": aria });
      btn.append(icon(ic));
      if (label) btn.append(h("span", {}, label));
      if (key) btn.append(h("kbd", {}, key));
      btn.addEventListener("click", (e) => this.onTool(a, e.shiftKey));
      this.tools.set(a, { btn });
      bar.append(btn);
    };
    add("place", "place", "Place", "", "Place|Click");
    add("rotate", "rotate", "Rotate", "R", "Rotate|R|Shift+R turns back");
    add("undo", "undo", "Undo", `${MOD}Z`, `Undo|${MOD}Z`);
    add("redo", "redo", "", "", `Redo|${isMac() ? "⇧⌘Z" : "Ctrl+Shift+Z"}`, "Redo");
    add("bulldoze", "bulldoze", "Bulldoze", "B", "Bulldoze|B");
    bar.append(rule());
    add("inventory", "inventory", "Tiles", "Tab", "Tiles|Tab", "Tiles, Tab");
    bar.append(rule());
    add("photo", "photo", "", "", "Photo mode|P", "Photo mode, P");
    add("settings", "settings", "", "", "Settings", "Settings");
    return bar;
  }

  private onTool(a: string, shift: boolean): void {
    const st = this.st;
    if (!st || this.tools.get(a)?.btn.getAttribute("aria-disabled") === "true") return;
    if (a === "place") {
      if (st.tool !== "place") this.cmd({ cmd: "set_tool", mode: "place" });
      this.fire("place");
    } else if (a === "rotate") this.rotate(shift);
    else if (a === "undo") this.history(false);
    else if (a === "redo") this.history(true);
    else if (a === "bulldoze") this.toggleBulldoze();
    else if (a === "inventory") this.toggleSheet();
    else if (a === "photo") this.togglePhoto();
    else if (a === "settings") this.toggleSettings();
  }

  private togglePhoto(): void {
    this.fire("photo");
    if (this.settings.open) this.toggleSettings();
    this.cmd({ cmd: "toggle", what: "photo" });
  }

  private toggleSettings(): void {
    this.settings.open = !this.settings.open;
    this.tools.get("settings")?.btn.classList.toggle("is-on", this.settings.open);
  }

  private history(redo: boolean): void {
    const st = this.st;
    if (!st || !(redo ? st.can_redo : st.can_undo)) return;
    this.fire(redo ? "redo" : "undo");
    this.cmd({ cmd: redo ? "redo" : "undo" });
  }

  private toggleBulldoze(): void {
    this.fire("bulldoze");
    this.cmd({ cmd: "toggle", what: "bulldoze" });
  }

  private fire(a: string): void {
    const b = this.tools.get(a)?.btn;
    if (b && !reduceMotion()) retrigger(b, "is-fired");
  }

  private syncBar(st: PlayState): void {
    const t = (a: string) => this.tools.get(a)!.btn;
    t("place").classList.toggle("is-active", st.tool === "place");
    t("bulldoze").classList.toggle("is-active", st.tool === "bulldoze");
    t("inventory").classList.toggle("is-on", this.open);
    for (const [a, can, none] of [
      ["undo", st.can_undo, "Nothing to undo"],
      ["redo", st.can_redo, "Nothing to redo"],
    ] as const) {
      const b = t(a);
      b.setAttribute("aria-disabled", String(!can));
      b.dataset.tip = can ? (a === "undo" ? `Undo|${MOD}Z` : `Redo|${isMac() ? "⇧⌘Z" : "Ctrl+Shift+Z"}`) : none;
    }
    const n = st.tiles.length;
    const text = plural(n, "tile");
    if (this.tiles.textContent !== text) this.tiles.textContent = text;
  }

  // ---------- Frame ----------
  /** Out of the play scenario: forget the game, so keys pass through to the engine again. */
  sleep(): void {
    this.st = null;
    this.pending = [];
  }

  onEvents(batch: GameEvent[]): void {
    this.pending.push(...batch);
    this.photo.onEvents(batch);
  }

  render(st: PlayState, state: State = {}): void {
    this.photo.render(st.photo_mode, state.photo);
    this.settings.render(state.tilt);
    this.root.classList.toggle("is-photo", st.photo_mode);
    const prev = this.st;
    this.st = st;
    const events = this.pending;
    this.pending = [];
    const now = performance.now();

    if (!this.synced) {
      // The sheet opens by default (owner, 2026-10-13); the engine's flag starts closed.
      this.synced = true;
      this.engineOpen = st.inventory_open;
      if (!st.inventory_open) this.cmd({ cmd: "toggle", what: "inventory" });
      this.sheet.open = true;
    } else if (st.inventory_open !== this.engineOpen) {
      this.engineOpen = st.inventory_open;
      this.open = st.inventory_open;
      this.sheet.open = this.open;
    }

    const changed =
      !prev ||
      prev.tiles.length !== st.tiles.length ||
      events.some(
        (e) => e.type === "tilePlaced" || e.type === "tileBulldozed" || e.type === "undone" || e.type === "redone",
      );
    const before = this.map;
    if (changed) {
      this.map = tileMap(st.tiles);
      this.tilesRev++;
    }
    for (const e of events) this.onEvent(e, before, now);

    // The sheet's slot sticks to the last slot pointed at until another slot, Esc or a placement.
    if (st.tool === "place" && st.hovered_kind === "slot" && st.hovered && !sameCell(st.hovered, this.sheetSlot)) {
      this.sheetSlot = st.hovered;
      this.turns = 0;
      this.pinned = false;
      this.advanceHint("hover");
    }
    if (this.sheetSlot && !isSlot(this.sheetSlot, this.map)) this.sheetSlot = null;
    this.pin(st);

    this.syncBar(st);
    this.renderSheet(st);
    this.renderTag(st, now);
    this.root.classList.toggle("is-hidden", this.hidden || st.photo_mode);
    this.renderHint(st);
  }

  /** Keeps the engine hovering the sheet's slot while the pointer is off the canvas, so R works there. */
  private pin(st: PlayState): void {
    if (!this.pointerOff || this.pinned || !this.sheetSlot || st.tool !== "place" || st.hovered) return;
    this.pinned = true;
    this.cmd({ cmd: "hover", q: this.sheetSlot.q, r: this.sheetSlot.r });
  }

  private onEvent(e: GameEvent, before: TileMap, now: number): void {
    switch (e.type) {
      case "tilePlaced":
        this.placed = { cell: e, tile: e.tile, until: now + PLACED_TAG_MS };
        this.refusal = null;
        this.fire("place");
        this.say(`Placed ${tileName(e.tile)}.`);
        this.advanceHint("place");
        break;
      case "placementRefused": {
        const def = tileDef(e.tile);
        const why = def ? fit(def, needsAt(e, this.map)).why : "";
        const text = why || "An edge doesn't match its neighbour.";
        this.refusal = { cell: e, text, until: now + 2000 };
        if (!reduceMotion()) retrigger(this.tag, "is-pulse");
        this.say(`${tileName(e.tile)} doesn't fit here. ${text}`);
        break;
      }
      case "bulldozeRefused":
        this.fire("bulldoze");
        if (!reduceMotion()) retrigger(this.tag, "is-pulse");
        this.say(bulldozeRefusal(e.reason, cutOff(this.map, e)).head);
        break;
      case "tileBulldozed":
        this.showToast({ text: `Removed ${tileName(e.tile)}`, icon: "bulldoze", action: "Undo", redo: false });
        this.toastState = null;
        break;
      case "undone":
      case "redone": {
        const redo = e.type === "redone";
        const verb = redo ? "Redid" : "Undid";
        const visible = this.toast.classList.contains("is-shown");
        const count = visible && this.toastState?.verb === verb ? this.toastState.count + 1 : 1;
        this.toastState = { verb, count };
        const what = actionText(diffTiles(before, this.map), redo);
        const text = count > 1 ? `${verb} ${count} actions` : `${verb}: ${what}`;
        this.showToast({ text, icon: redo ? "redo" : "undo", action: redo ? "Undo" : "Redo", redo: !redo, count });
        this.say(`${verb} ${what}.`);
        if (!redo) this.advanceHint("undo");
        break;
      }
      case "toolChanged":
        if (e.mode === "bulldoze") this.advanceHint("bulldoze");
        break;
      default:
        break;
    }
  }

  private say(msg: string): void {
    this.live.textContent = msg;
  }

  // ---------- Sheet ----------
  private renderSheet(st: PlayState): void {
    const slot = this.sheetSlot;
    const ghostRotation = st.ghost && sameCell(st.ghost, slot) ? st.ghost.rotation : null;
    const sig = [this.tilesRev, slot && cellKey(slot), st.selected, ghostRotation, st.tool, this.turns].join("|");
    if (sig === this.sheetSig) return;
    this.sheetSig = sig;
    const needs = slot ? needsAt(slot, this.map) : null;
    let nearest: { cell: Cell; fits: number } | null = null;
    if (slot && needs && !TILES.some((d) => fit(d, needs).ok)) nearest = this.nearestFittingSlot(st, slot);
    this.sheet.render({
      slot: slot && needs ? { cell: slot, needs } : null,
      selected: st.selected,
      ghostRotation,
      turns: this.turns,
      bulldoze: st.tool === "bulldoze",
      nearest,
    });
  }

  private nearestFittingSlot(st: PlayState, from: Cell): { cell: Cell; fits: number } | null {
    let best: { cell: Cell; fits: number; d: number } | null = null;
    for (const c of st.frontier) {
      if (sameCell(c, from)) continue;
      const d = (Math.abs(c.q - from.q) + Math.abs(c.r - from.r) + Math.abs(c.q + c.r - from.q - from.r)) / 2;
      if (best && d >= best.d) continue;
      const needs = needsAt(c, this.map);
      const fits = TILES.filter((t) => fit(t, needs).ok).length;
      if (fits) best = { cell: c, fits, d };
    }
    return best && { cell: best.cell, fits: best.fits };
  }

  // ---------- Tag beside the ghost or the bulldoze target ----------
  private renderTag(st: PlayState, now: number): void {
    const tag = this.tag;
    let cell: Cell | null = null;
    let sig = "";
    const parts: (Node | string)[] = [];
    let bad = false;
    let stacked = false;
    if (this.placed && now > this.placed.until) this.placed = null;
    if (this.refusal && now > this.refusal.until) this.refusal = null;
    if (st.tool === "bulldoze" && st.bulldoze) {
      const b = st.bulldoze;
      cell = b;
      const t = this.map.get(cellKey(b));
      if (b.allowed) {
        sig = `rm|${cellKey(b)}`;
        parts.push(
          icon("bulldoze"),
          h("span", {}, `Remove ${tileName(t?.tile ?? "")}`),
          h("span", { class: "ghost-tag__fit is-quiet" }, "Click"),
        );
      } else {
        const r = bulldozeRefusal(b.reason, cutOff(this.map, b));
        sig = `no|${cellKey(b)}|${r.sub}`;
        bad = stacked = true;
        parts.push(icon("alert"), h("span", { class: "tag-stack" }, h("span", {}, r.head), h("small", {}, r.sub)));
      }
    } else if (this.placed && st.tool === "place" && (!st.ghost || sameCell(st.ghost, this.placed.cell))) {
      cell = this.placed.cell;
      sig = `placed|${cellKey(cell)}`;
      parts.push(icon("check", "ic ok-ic"), h("span", {}, `Placed ${tileName(this.placed.tile)}`));
    } else if (st.tool === "place" && st.ghost) {
      const g = st.ghost;
      cell = g;
      const def = tileDef(g.tile);
      const f = def ? fit(def, needsAt(g, this.map)) : null;
      const name = tileName(g.tile);
      if (f?.ok && g.valid) {
        const { index, count } = rotationReadout(def!, g.legal_rotations, g.rotation);
        const text = count === 1 ? "Fits" : `Rotation ${index + 1} of ${count}`;
        sig = `ok|${cellKey(g)}|${g.tile}|${text}`;
        parts.push(h("span", {}, name), h("span", { class: "ghost-tag__fit" }, text));
        if (count > 1) parts.push(h("kbd", {}, "R"));
      } else {
        bad = true;
        const clashes = g.edges.filter((e) => e === "Incompatible").length;
        const why =
          this.refusal && sameCell(this.refusal.cell, g)
            ? this.refusal.text
            : f && !f.ok
              ? f.why
              : `${plural(clashes, "edge")} clash`;
        sig = `bad|${cellKey(g)}|${g.tile}|${why}`;
        parts.push(icon("alert"), h("span", {}, name), h("span", { class: "ghost-tag__fit is-bad" }, why));
        if (f?.ok) parts.push(h("kbd", {}, "R"));
      }
    }
    if (!cell) {
      tag.classList.add("is-gone");
      tag.dataset.sig = "";
      return;
    }
    if (tag.dataset.sig !== sig) {
      tag.dataset.sig = sig;
      tag.replaceChildren(...parts);
      tag.classList.toggle("ghost-tag--bad", bad);
      tag.classList.toggle("is-stacked", stacked);
      tag.setAttribute("aria-label", tag.textContent ?? "");
    }
    const rect = this.hw.cellRect(cell.q, cell.r);
    if (!rect) {
      tag.classList.add("is-gone");
      return;
    }
    this.placeTag(rect);
    tag.classList.remove("is-gone");
  }

  private placeTag(rect: Rect): void {
    const [l, t, r, b] = rect;
    const w = this.tag.offsetWidth;
    const hgt = this.tag.offsetHeight;
    const limit = innerWidth - (this.open && !this.hidden ? this.sheet.el.offsetWidth + 40 : 20);
    let x = r + 14;
    let y = (t + b) / 2 - hgt / 2;
    if (x + w > limit) {
      x = Math.max(20, Math.min(limit - w, (l + r) / 2 - w / 2));
      y = b + 12;
    }
    y = Math.max(84, Math.min(innerHeight - hgt - 20, y));
    this.tag.style.transform = `translate(${Math.round(x)}px, ${Math.round(y)}px)`;
  }

  // ---------- Toast ----------
  private showToast(o: { text: string; icon: string; action: string; redo: boolean; count?: number }): void {
    const el = this.toast;
    const btn = h("button", { class: "toast__action", type: "button" }, o.action);
    btn.addEventListener("click", () => this.history(o.redo));
    const line = h("span", {});
    if ((o.count ?? 1) > 1) line.append(h("span", { class: "toast__count" }, String(o.count)));
    line.append(o.text);
    el.replaceChildren(icon(o.icon), h("p", {}, line), btn);
    if (!el.classList.contains("is-shown")) {
      void el.offsetWidth;
      el.classList.add("is-shown");
    }
    this.armToast();
  }

  private armToast(): void {
    clearTimeout(this.toastTimer);
    this.toastTimer = window.setTimeout(() => {
      this.toast.classList.remove("is-shown");
      this.toastState = null;
    }, TOAST_MS);
  }

  // ---------- First-run callouts ----------
  private advanceHint(ev: string): void {
    const h0 = HINTS[this.hint];
    if (!h0?.done.includes(ev)) return;
    this.hint++;
    storeSet(HINTS_KEY, String(this.hint));
    this.callout.classList.remove("is-shown");
    this.leader.classList.remove("is-shown");
    clearTimeout(this.hintTimer);
    this.hintTimer = 0;
  }

  private renderHint(st: PlayState): void {
    const hint = HINTS[this.hint];
    const shown = this.callout.classList.contains("is-shown");
    if (!hint) return;
    const anchor = !this.hidden && hint.when(st) ? hint.anchor(this, st) : null;
    if (!anchor) {
      if (shown) {
        this.callout.classList.remove("is-shown");
        this.leader.classList.remove("is-shown");
      }
      clearTimeout(this.hintTimer);
      this.hintTimer = 0;
      return;
    }
    if (!shown && !this.hintTimer) {
      // A callout appears after 1.5 s of calm, never mid-action.
      this.hintTimer = window.setTimeout(
        () => {
          this.hintTimer = 0;
          if (this.st) this.callout.replaceChildren(...hint.body(this.st));
          this.callout.classList.add("is-shown");
          this.leader.classList.add("is-shown");
        },
        reduceMotion() ? 0 : 1500,
      );
    }
    if (shown) this.drawLeader(anchor, hint.offset);
  }

  private drawLeader(a: [number, number], off: [number, number]): void {
    const box = this.callout;
    const w = box.offsetWidth;
    const hh = box.offsetHeight;
    const x = Math.max(20, Math.min(innerWidth - w - 20, a[0] + off[0]));
    const y = Math.max(84, Math.min(innerHeight - hh - 20, a[1] + off[1]));
    box.style.transform = `translate(${Math.round(x)}px, ${Math.round(y)}px)`;
    const sx = x + w < a[0] ? x + w : x > a[0] ? x : x + w / 2;
    const sy = y + hh < a[1] ? y + hh : y > a[1] ? y : y + hh / 2;
    this.leader
      .querySelector("path")!
      .setAttribute("d", `M${sx},${sy} C${(sx + a[0]) / 2},${sy} ${a[0]},${(sy + a[1]) / 2} ${a[0]},${a[1]}`);
    const c = this.leader.querySelector("circle")!;
    c.setAttribute("cx", String(a[0]));
    c.setAttribute("cy", String(a[1]));
  }

  /** Centre of a cell on screen, for callouts. */
  cellCentre(c: Cell): [number, number] | null {
    const r = this.hw.cellRect(c.q, c.r);
    return r ? [(r[0] + r[2]) / 2, (r[1] + r[3]) / 2] : null;
  }

  toolAnchor(a: string): [number, number] | null {
    const b = this.tools.get(a)?.btn.getBoundingClientRect();
    return b ? [b.left + b.width / 2, b.bottom + 2] : null;
  }

  tagAnchor(): [number, number] | null {
    if (this.tag.classList.contains("is-gone")) return null;
    const k = this.tag.querySelector("kbd")?.getBoundingClientRect();
    return k ? [k.left + k.width / 2, k.bottom + 2] : null;
  }

  // ---------- Input ----------
  private wireInput(): void {
    // Game keys belong to the overlay: caught at the window before the canvas (and Bevy) sees
    // them, then sent as commands, so one press acts once. Camera keys (WASD, arrows) pass through.
    addEventListener("keydown", (e) => this.onKey(e), { capture: true });
    // Keep keyboard focus on the canvas when a HUD control is clicked, so WASD keeps panning.
    this.root.addEventListener("mousedown", (e) => {
      if ((e.target as Element).closest("button, [data-tip]")) e.preventDefault();
    });
    this.root.addEventListener(
      "wheel",
      (e) => {
        if (!e.ctrlKey && !e.metaKey) return;
        e.preventDefault();
        if (this.st?.ghost) this.rotate(e.deltaY > 0);
      },
      { passive: false },
    );
    this.canvas?.addEventListener("pointerleave", () => {
      this.pointerOff = true;
    });
    this.canvas?.addEventListener("pointerenter", () => {
      this.pointerOff = false;
      this.pinned = false;
    });
    // Tooltips: name and key after 350 ms of hover, at once on keyboard focus.
    this.root.addEventListener("pointerover", (e) => {
      const t = (e.target as Element).closest<HTMLElement>("[data-tip]");
      if (!t) return;
      clearTimeout(this.tipTimer);
      this.tipTimer = window.setTimeout(() => this.showTip(t), 350);
    });
    this.root.addEventListener("pointerout", (e) => {
      if ((e.target as Element).closest("[data-tip]")) {
        clearTimeout(this.tipTimer);
        this.tip.classList.remove("is-shown");
      }
    });
    this.root.addEventListener("focusin", (e) => {
      const t = (e.target as Element).closest<HTMLElement>("[data-tip]");
      if (t?.matches(":focus-visible")) this.showTip(t);
    });
    this.root.addEventListener("focusout", () => this.tip.classList.remove("is-shown"));
    this.toast.addEventListener("mouseenter", () => clearTimeout(this.toastTimer));
    this.toast.addEventListener("mouseleave", () => this.armToast());
  }

  showTip(el: HTMLElement): void {
    const [name, key, note] = (el.dataset.tip ?? "").split("|");
    if (!name) return;
    this.tip.replaceChildren(name);
    if (key) this.tip.append(h("kbd", {}, key));
    if (note) this.tip.append(h("small", {}, note));
    const r = el.getBoundingClientRect();
    const x = Math.min(innerWidth - this.tip.offsetWidth - 8, r.left);
    this.tip.style.transform = `translate(${Math.round(x)}px, ${Math.round(r.bottom + 8)}px)`;
    this.tip.classList.add("is-shown");
  }

  private setHidden(v: boolean): void {
    this.hidden = v;
    this.root.classList.toggle("is-hidden", v);
  }

  private onKey(e: KeyboardEvent): void {
    const st = this.st;
    if (!st || e.altKey) return;
    const mod = e.ctrlKey || e.metaKey;
    const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    const take = () => {
      e.preventDefault();
      e.stopImmediatePropagation();
    };
    // Photo mode owns P, Esc and H and swallows the game keys; with a modifier nothing acts (no undo).
    if (st.photo_mode || this.photo.active) {
      if (!mod && this.photo.key(e)) take();
      return;
    }
    if (mod) {
      if (k === "z") {
        take();
        this.history(e.shiftKey);
      }
      return;
    }
    const onRow = (e.target as Element).closest?.(".row");
    switch (e.code === "Tab" ? "Tab" : k) {
      case "Tab":
        take();
        this.toggleSheet();
        return;
      case "F6":
        take();
        this.cycleFocus(e.shiftKey);
        return;
      case "r":
        take();
        if (e.repeat && performance.now() - this.lastRotate < ROTATE_REPEAT_MS) return;
        this.lastRotate = performance.now();
        this.rotate(e.shiftKey);
        return;
      case "b":
        take();
        if (!e.repeat) this.toggleBulldoze();
        return;
      case "p":
        take();
        if (!e.repeat) this.togglePhoto();
        return;
      case "h":
        take();
        if (!e.repeat) this.setHidden(!this.hidden);
        return;
      case "Escape":
        take();
        if (this.settings.open) this.toggleSettings();
        else if (st.tool === "bulldoze") this.cmd({ cmd: "set_tool", mode: "place" });
        else if (this.hidden) this.setHidden(false);
        else this.sheetSlot = null;
        return;
      case "Enter":
        if (onRow || (e.target as Element).closest?.("button")) return;
        take();
        if (st.tool === "place" && st.hovered_kind === "slot" && st.hovered)
          this.cmd({ cmd: "place", q: st.hovered.q, r: st.hovered.r });
        return;
      default:
        break;
    }
    if (/^[0-9=-]$/.test(k)) {
      take();
      const id = /^[1-9]$/.test(k) ? this.sheet.numbered[Number(k) - 1] : undefined;
      if (id) this.selectTile(id);
    }
  }

  private cycleFocus(back: boolean): void {
    const regions: HTMLElement[] = [...this.root.querySelectorAll<HTMLElement>("[data-region]")].filter(
      (r) => !r.classList.contains("is-closed") && (r !== this.toast || r.classList.contains("is-shown")),
    );
    if (this.hidden) regions.length = 0;
    const all: HTMLElement[] = this.canvas ? [...regions, this.canvas] : regions;
    if (!all.length) return;
    const i = all.findIndex((r) => r.contains(document.activeElement));
    const next = all[(i + (back ? -1 : 1) + all.length) % all.length]!;
    if (next === this.canvas) next.focus();
    else next.querySelector<HTMLElement>("button:not([aria-disabled=true]), [tabindex='0']")?.focus();
  }
}

function rule(): HTMLElement {
  return h("span", { class: "bar__rule", "aria-hidden": "true" });
}

interface Hint {
  done: string[];
  when: (st: PlayState) => boolean;
  anchor: (hud: Hud, st: PlayState) => [number, number] | null;
  offset: [number, number];
  body: (st: PlayState) => (Node | string)[];
}

/** One callout at a time; each leaves on its action and never returns (stored per browser). */
const HINTS: Hint[] = [
  {
    done: ["hover", "place"],
    when: (st) => st.tiles.length === 1 && st.tool === "place",
    anchor: (hud, st) => {
      const west = st.frontier.find((c) => c.q === -1 && c.r === 0) ?? st.frontier[0];
      return west ? hud.cellCentre(west) : null;
    },
    offset: [-170, 150],
    body: (st) => [icon("place"), `Click a dashed slot to place ${tileName(st.selected)}`],
  },
  {
    done: ["rotate", "place"],
    when: (st) => !!st.ghost,
    anchor: (hud) => hud.tagAnchor(),
    offset: [-60, 60],
    body: () => [h("kbd", {}, "R"), "turns the tile before you place it"],
  },
  {
    done: ["undo", "place"],
    when: (st) => st.can_undo,
    anchor: (hud) => hud.toolAnchor("undo"),
    offset: [-30, 50],
    body: () => [h("kbd", {}, `${MOD}Z`), "takes it back"],
  },
  {
    done: ["bulldoze"],
    when: (st) => st.tiles.length >= 3,
    anchor: (hud) => hud.toolAnchor("bulldoze"),
    offset: [-30, 50],
    body: () => [h("kbd", {}, "B"), "clears a tile away"],
  },
];
