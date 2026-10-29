// The Tiles sheet (S3): the slot key, "Fits here" and "Doesn't fit here" with reasons, socket codes,
// number keys. Rows keep their identity across slots so a re-rank animates (FLIP) instead of blinking.
import type { Cell } from "./bridge.ts";
import { h, isMac, motionMs, reduceMotion, s } from "./dom.ts";
import { KIND_CLASS, TilePicture, hexPoints, socketHex } from "./glyph.ts";
import { icon } from "./icons.ts";
import {
  type EntryFit,
  type Kinds,
  type Needs,
  PICKER,
  type PickerEntry,
  type Turn,
  entryMeta,
  entrySummary,
  fitEntry,
  needLines,
  nothingFitsWhy,
  turnAngle,
} from "./rules.ts";
import { tileName } from "./words.ts";

/** One member's picture and socket hex; a group row shows the member its current turn is on. */
interface Face {
  pic: TilePicture;
  hex: SVGElement;
  /** The socket hex's turning group, rotated with `pic`. */
  hexRot: SVGGElement;
  /** The angle the socket hex shows, accumulated so a turn always takes the short way. */
  deg: number;
}

interface Row {
  entry: PickerEntry;
  li: HTMLLIElement;
  btn: HTMLButtonElement;
  meta: HTMLElement;
  num: HTMLElement;
  faces: Map<string, Face>;
  shown: string;
}

/** How long after an R press the ghost's rotation may still land and turn the selected row (ms). */
const TURN_FOLLOW_MS = 500;

export interface SheetView {
  /** The slot the filter reads (sticky), with what its edges touch. */
  slot: { cell: Cell; needs: Needs } | null;
  selected: string;
  /** The ghost's tile and rotation when it sits on the sheet's slot. */
  ghostTile: string | null;
  ghostRotation: number | null;
  ghostLevel: number;
  /** R presses at this slot: every other fitting row turns in step, through its own distinct fits. */
  turns: number;
  bulldoze: boolean;
  /** The nearest slot where something fits, offered when nothing fits here. */
  nearest: { cell: Cell; fits: number } | null;
}

export class Sheet {
  readonly el: HTMLElement;
  private readonly key: HTMLElement;
  private readonly fitList: HTMLUListElement;
  private readonly offList: HTMLUListElement;
  private readonly fitGroup: HTMLElement;
  private readonly offGroup: HTMLElement;
  private readonly fitLabel: HTMLElement;
  private readonly fitCount: HTMLElement;
  private readonly offCount: HTMLElement;
  private readonly rows: Row[];
  private order: string[] = [];
  /** The last render's R count and ghost, and when R last moved, so only a turn turns a thumbnail. */
  private turnsSeen = 0;
  private ghostSeen: { tile: string | null; rotation: number | null } = { tile: null, rotation: null };
  private turnDir = 1;
  private turnedAt = Number.NEGATIVE_INFINITY;
  private keySig = "";
  private slotKey = "";
  private readonly body: HTMLElement;
  /** Picker entry ids in "Fits here" order, for the 1–9 keys. */
  numbered: string[] = [];

  private readonly onSelect: (id: string) => void;
  private readonly onClose: () => void;
  private readonly onGoto: (c: Cell) => void;

  /** Owns inventory visibility and numbered selection in build mode. */
  handleKey(e: KeyboardEvent): boolean {
    const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    if (e.code === "Tab") {
      this.onClose();
      return true;
    }
    if (!/^[0-9=-]$/.test(k)) return false;
    const id = /^[1-9]$/.test(k) ? this.numbered[Number(k) - 1] : undefined;
    if (id) this.onSelect(id);
    return true;
  }

  constructor(onSelect: (id: string) => void, onClose: () => void, onGoto: (c: Cell) => void) {
    this.onGoto = onGoto;
    this.onSelect = onSelect;
    this.onClose = onClose;
    this.key = h("div", { class: "key" });
    this.fitLabel = h("span", {}, "All tiles");
    this.fitCount = h("span", { class: "group__count" });
    this.offCount = h("span", { class: "group__count" });
    this.fitGroup = h("h2", { class: "group" }, this.fitLabel, this.fitCount);
    this.offGroup = h(
      "h2",
      { class: "group group--off" },
      h("span", {}, "Doesn't fit here"),
      this.offCount,
      icon("down", "ic group__more"),
    );
    this.fitList = h("ul", { class: "list list--fit", "aria-label": "Tiles that fit" });
    this.offList = h("ul", { class: "list list--off", "aria-label": "Tiles that don't fit" });
    const close = h("button", {
      class: "sheet__close",
      type: "button",
      "aria-label": "Close tiles, Tab",
      "data-tip": "Close|Tab",
    });
    close.append(icon("close"));
    close.addEventListener("click", onClose);
    this.offGroup.addEventListener("click", () => this.offList.scrollIntoView({ block: "nearest" }));
    this.body = h("div", { class: "sheet__body" }, this.fitGroup, this.fitList, this.offGroup, this.offList);
    const foot = h(
      "p",
      { class: "sheet__foot" },
      h("kbd", {}, "1–9"),
      " pick ",
      h("kbd", {}, "R"),
      " turn ",
      h("kbd", {}, "F6"),
      " next panel",
    );
    const mods = h(
      "p",
      { class: "sheet__mods" },
      h("kbd", {}, "Shift-click"),
      " place ",
      h("kbd", {}, isMac() ? "⌘-click" : "Ctrl-click"),
      " replace",
    );
    this.el = h(
      "section",
      { class: "sheet sheet--tiles", "data-region": "sheet", "aria-label": "Tile inventory" },
      h("header", { class: "sheet__head" }, h("h1", {}, "Tiles"), h("kbd", {}, "Tab"), close),
      this.key,
      this.body,
      foot,
      mods,
    );
    this.rows = PICKER.map((entry) => {
      const meta = h("span", { class: "row__meta" });
      const num = h("span", { class: "row__num", "aria-hidden": "true" });
      const faces = new Map<string, Face>();
      const first = entry.tiles[0]!.id;
      for (const def of entry.tiles) {
        const pic = new TilePicture(def);
        const hex = socketHex(def);
        const hexRot = hex.querySelector<SVGGElement>(".g-rot")!;
        pic.el.toggleAttribute("hidden", def.id !== first);
        hex.toggleAttribute("hidden", def.id !== first);
        faces.set(def.id, { pic, hex, hexRot, deg: 0 });
      }
      const all = [...faces.values()];
      const btn = h(
        "button",
        { class: "row", type: "button", "data-tile": entry.id, "aria-pressed": "false" },
        ...all.map((f) => f.pic.el),
        h("span", { class: "row__text" }, h("span", { class: "row__name" }, tileName(first)), meta),
        ...all.map((f) => f.hex),
        num,
      );
      btn.addEventListener("click", () => onSelect(entry.id));
      const li = h("li", {}, btn);
      this.fitList.append(li);
      return { entry, li, btn, meta, num, faces, shown: first };
    });
    this.el.addEventListener("keydown", (e) => {
      const row = (e.target as Element).closest?.(".row");
      if (!row || (e.key !== "ArrowDown" && e.key !== "ArrowUp")) return;
      e.preventDefault();
      const all = [...this.el.querySelectorAll<HTMLButtonElement>(".row")];
      const i = all.indexOf(row as HTMLButtonElement);
      all[Math.max(0, Math.min(all.length - 1, i + (e.key === "ArrowDown" ? 1 : -1)))]?.focus();
    });
  }

  set open(v: boolean) {
    this.el.classList.toggle("is-closed", !v);
    this.el.toggleAttribute("inert", !v);
  }

  render(v: SheetView): void {
    this.el.classList.toggle("is-inert", v.bulldoze);
    const fits = new Map<string, EntryFit>();
    for (const r of this.rows) if (v.slot) fits.set(r.entry.id, fitEntry(r.entry, v.slot.needs));
    const ok = (r: Row) => !v.slot || fits.get(r.entry.id)!.ok;
    const fitting = this.rows.filter(ok);
    const off = this.rows.filter((r) => !ok(r));
    this.renderKey(v, fitting.length > 0);

    const slotKey = v.slot ? `${v.slot.cell.q},${v.slot.cell.r}` : "";
    // A thumbnail turns only for R at the same slot: its count moved (every fitting row), or the
    // ghost's rotation followed a press on the same tile (the selected row). A new slot, a re-rank or
    // a new selection swaps pictures at once.
    const sameSlot = slotKey === this.slotKey;
    const now = performance.now();
    if (v.turns !== this.turnsSeen) {
      if (sameSlot) {
        this.turnDir = v.turns > this.turnsSeen ? 1 : -1;
        this.turnedAt = now;
      }
      this.turnsSeen = v.turns;
    }
    const pressed = sameSlot && now - this.turnedAt < TURN_FOLLOW_MS;
    const ghostTurned = pressed && v.ghostTile === this.ghostSeen.tile && v.ghostRotation !== this.ghostSeen.rotation;
    const rowsTurned = pressed && this.turnedAt === now;
    this.ghostSeen = { tile: v.ghostTile, rotation: v.ghostRotation };
    if (slotKey !== this.slotKey) {
      // A new slot re-ranks the sheet: start from the top so "Fits here" leads.
      this.slotKey = slotKey;
      this.body.scrollTop = 0;
    }
    const order = [...fitting.map((r) => `${r.entry.id}+`), ...off.map((r) => `${r.entry.id}-`)];
    const moved = order.join() !== this.order.join();
    const before =
      moved && !reduceMotion() ? new Map(this.rows.map((r) => [r, r.li.getBoundingClientRect().top])) : null;
    for (const r of this.rows) {
      const f = fits.get(r.entry.id);
      const selected = r.entry.tiles.some((t) => t.id === v.selected);
      const name = tileName(r.entry.tiles[0]!.id);
      const meta = !v.slot ? entrySummary(r.entry) : f!.ok ? entryMeta(r.entry, f!) : f!.why;
      if (r.meta.textContent !== meta) r.meta.textContent = meta;
      r.btn.setAttribute("aria-pressed", String(selected));
      r.btn.setAttribute("aria-label", `${name}. ${v.slot && !f!.ok ? `Doesn't fit here. ${meta}` : meta}`);
      // Fitting rows show the tile and rotation they would take here; the selected one follows the
      // ghost. A group row shows the member its turn is on.
      const ghost = r.entry.tiles.find((t) => t.id === v.ghostTile);
      const n = f?.seq.length ?? 0;
      const turn: Turn = !f
        ? { def: r.entry.tiles[0]!, k: 0, level: 0 }
        : !f.ok
          ? f.best
          : selected && ghost && v.ghostRotation != null
            ? { def: ghost, k: v.ghostRotation, level: v.ghostLevel }
            : f.seq[((v.turns % n) + n) % n]!;
      const follows = selected && !!ghost && v.ghostRotation != null;
      this.showFace(r, turn, (follows ? ghostTurned : rowsTurned) && !moved ? this.turnDir : 0);
    }
    if (moved) {
      for (const r of fitting) this.fitList.append(r.li);
      for (const r of off) this.offList.append(r.li);
      this.order = order;
    }
    this.numbered = fitting.map((r) => r.entry.id);
    fitting.forEach((r, i) => {
      r.num.textContent = i < 9 ? String(i + 1) : "";
    });
    for (const r of off) r.num.textContent = "";
    this.fitLabel.textContent = v.slot ? "Fits here" : "All tiles";
    this.fitCount.textContent = String(fitting.length);
    this.offCount.textContent = String(off.length);
    this.fitGroup.hidden = !fitting.length;
    this.offGroup.hidden = !off.length;
    if (before) {
      for (const [r, top] of before) {
        const dy = top - r.li.getBoundingClientRect().top;
        if (dy)
          r.li.animate([{ transform: `translateY(${dy}px)` }, { transform: "none" }], {
            duration: motionMs("--t-slow", 240),
            easing: "cubic-bezier(.22,.9,.3,1)",
          });
      }
    }
  }

  private showFace(r: Row, t: Turn, dir: number): void {
    let prev: string | null = null;
    if (r.shown !== t.def.id) {
      // R moved a group row to another member: its picture turns in from the one on screen.
      prev = r.faces.get(r.shown)!.pic.src;
      for (const [id, f] of r.faces) {
        f.pic.el.toggleAttribute("hidden", id !== t.def.id);
        f.hex.toggleAttribute("hidden", id !== t.def.id);
      }
      r.shown = t.def.id;
    }
    const face = r.faces.get(t.def.id)!;
    face.pic.show(t.k, dir, dir ? prev : null);
    const deg = turnAngle(face.deg, t.k);
    if (deg !== face.deg || !face.hexRot.style.transform) {
      face.deg = deg;
      face.hexRot.style.transform = `rotate(${deg}deg)`;
    }
  }

  private renderKey(v: SheetView, anyFit: boolean): void {
    const sig = JSON.stringify([v.bulldoze, v.slot?.needs ?? null, anyFit, v.nearest]);
    if (sig === this.keySig) return;
    this.keySig = sig;
    const k = this.key;
    k.replaceChildren();
    k.className = "key";
    if (v.bulldoze) {
      const hex = s(
        "svg",
        { class: "key__hex", viewBox: "0 0 100 100", "aria-hidden": "true" },
        s("polygon", { class: "key__dozer", points: hexPoints(34) }),
        s("path", { class: "key__x", d: "M40 40l20 20M60 40 40 60" }),
      );
      const li = (cls: string, text: string) => h("li", {}, h("span", { class: `key__swatch ${cls}` }), text);
      k.append(
        hex,
        h(
          "div",
          {},
          h("p", { class: "key__title" }, "Bulldoze"),
          h(
            "ul",
            { class: "key__list" },
            li("key__swatch--remove", "Click to remove"),
            li("key__swatch--blocked", "Would split the world"),
            li("key__swatch--cut", "The part it would cut off"),
          ),
        ),
      );
      return;
    }
    if (!v.slot) {
      k.append(
        keyHex([null, null, null, null, null, null], false),
        h(
          "div",
          {},
          h("p", { class: "key__title" }, "Point at a dashed slot"),
          h("p", { class: "empty-key" }, "The sheet sorts every tile by whether it fits there."),
        ),
      );
      return;
    }
    if (!anyFit) k.classList.add("key--none");
    const list = h("ul", { class: "key__list" });
    for (const l of needLines(v.slot.needs.kinds, v.slot.needs.stopped))
      list.append(h("li", {}, h("i", { class: KIND_CLASS[l.kind] }), l.text));
    const body = h(
      "div",
      {},
      h("p", { class: "key__title" }, anyFit ? "This slot needs" : "Nothing fits this slot"),
      list,
    );
    if (!anyFit) {
      body.append(h("p", { class: "empty-key" }, nothingFitsWhy(v.slot.needs.kinds)));
      if (v.nearest) {
        const go = h("button", { class: "btn btn--sm", type: "button" }, icon("focus"), "Show the nearest slot");
        const near = v.nearest;
        go.addEventListener("click", () => this.onGoto(near.cell));
        body.append(
          h(
            "div",
            { class: "key__more btn-row" },
            go,
            h("span", { class: "empty-key" }, `${near.fits} tile${near.fits === 1 ? "" : "s"} fit there`),
          ),
        );
      }
    }
    k.append(keyHex(v.slot.needs.kinds, !anyFit), body);
  }
}

/** The signature move: a hex that draws exactly which edges of the slot are touched, and by what. */
function keyHex(needs: Kinds, none: boolean): SVGElement {
  const v = (i: number): [number, number] => {
    const a = ((-30 + 60 * i) * Math.PI) / 180;
    return [50 + 34 * Math.cos(a), 50 + 34 * Math.sin(a)];
  };
  const svg = s(
    "svg",
    { class: "key__hex", viewBox: "0 0 100 100", "aria-hidden": "true" },
    s("polygon", { class: "key__slot", points: hexPoints(34) }),
  );
  needs.forEach((k, e) => {
    if (!k) return;
    const [x1, y1] = v(e);
    const [x2, y2] = v(e + 1);
    svg.append(s("line", { class: `key__edge ${KIND_CLASS[k]}`, x1, y1, x2, y2 }));
  });
  if (none) svg.append(s("path", { class: "key__x", d: "M42 42l16 16M58 42 42 58" }));
  return svg;
}
