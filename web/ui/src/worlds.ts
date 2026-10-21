// The Worlds sheet (S10, design/ui/legend/saves.html): the current world with rename and export,
// the worlds saved in this browser with load (asking first over unsaved changes, S10.3), rename,
// export and delete, New world and Import, and a drop target for .hexworld files. An imported file
// shows its preview (S10.6) or, when refused, the decoder's checks (S10.8) before anything changes. Every world is keyed by its socket-colour mix strip and a
// socket map thumbnail, the Legend's legend for a whole world.
import type { SocketKind } from "./bridge.ts";
import { h, s } from "./dom.ts";
import { KIND_CLASS } from "./glyph.ts";
import { icon } from "./icons.ts";
import { type Preview, type Refused, dayText, previewOf, refusedOf, unsavedText } from "./import-model.ts";
import { type Persistence, stateWords, whenText } from "./persist.ts";
import { rotated, tileDef } from "./rules.ts";
import { KIND_ORDER, type MapTile, type Mix, type SlotMeta, mixOf } from "./slots.ts";
import { plural } from "./words.ts";

const KIND_WORD: Record<SocketKind, string> = {
  Grass: "grass",
  Forest: "forest",
  Road: "road",
  River: "river",
  Beach: "beach",
  Ocean: "ocean",
  Rail: "rail",
};

/** A 6 px strip of socket-colour segments, each flexed by its edge count. */
export function mixStrip(mix: Mix): HTMLElement {
  const el = h("div", { class: "mix" });
  const parts: string[] = [];
  for (const k of KIND_ORDER) {
    const n = mix[k] ?? 0;
    if (!n) continue;
    const seg = h("i", { class: KIND_CLASS[k] });
    seg.style.flexGrow = String(n);
    el.append(seg);
    parts.push(`${KIND_WORD[k]} ${Math.round(n / 6)}`);
  }
  el.setAttribute("role", "img");
  el.setAttribute("aria-label", parts.length ? `Edges: ${parts.join(", ")} tiles' worth` : "No tiles");
  return el;
}

const SQ3 = Math.sqrt(3);

/** The world as a socket map: every tile a hex of six edge-coloured wedges, like the slot key. */
export function worldMap(map: readonly MapTile[]): SVGElement {
  const paths = new Map<SocketKind, string[]>();
  let [x0, y0, x1, y1] = [0, 0, 0, 0];
  const corner = (cx: number, cy: number, i: number): string => {
    const a = ((-30 + 60 * i) * Math.PI) / 180;
    return `${(cx + Math.cos(a)).toFixed(3)},${(cy + Math.sin(a)).toFixed(3)}`;
  };
  for (const [q, r, id, k] of map) {
    const def = tileDef(id);
    if (!def) continue;
    const cx = SQ3 * (q + r / 2);
    const cy = 1.5 * r;
    [x0, y0, x1, y1] = [Math.min(x0, cx), Math.min(y0, cy), Math.max(x1, cx), Math.max(y1, cy)];
    rotated(def, k).forEach((kind, e) => {
      const list = paths.get(kind) ?? [];
      list.push(`M${cx.toFixed(3)},${cy.toFixed(3)}L${corner(cx, cy, e)}L${corner(cx, cy, e + 1)}Z`);
      paths.set(kind, list);
    });
  }
  const pad = 1.4;
  const w = x1 - x0 + 2 * pad;
  const hgt = y1 - y0 + 2 * pad;
  // Letterbox to the 16:10 frame so a long, thin world keeps its proportions.
  const fw = Math.max(w, hgt * 1.6);
  const fh = fw / 1.6;
  const vb = [x0 - pad - (fw - w) / 2, y0 - pad - (fh - hgt) / 2, fw, fh].map((v) => v.toFixed(2)).join(" ");
  const svg = s("svg", { class: "thumb wmap", viewBox: vb, "aria-hidden": "true" });
  for (const kind of KIND_ORDER) {
    const d = paths.get(kind);
    if (d) svg.append(s("path", { class: KIND_CLASS[kind], d: d.join("") }));
  }
  return svg;
}

/** A file picked or dropped, waiting in the sheet: its preview, or why it was refused. */
type Incoming =
  | { kind: "preview"; seq: number; file: string; bytes: Uint8Array; savedAt: number; preview: Preview }
  | { kind: "refused"; seq: number; refused: Refused };

const btn = (cls: string, act: string, label: string, ic?: string, id?: string): HTMLElement => {
  const b = h("button", { class: cls, type: "button", "data-act": act, "data-focus": act, "data-id": id ?? null });
  if (ic) b.append(icon(ic));
  b.append(label);
  return b;
};

export class Worlds {
  readonly el: HTMLElement;
  private readonly p: Persistence;
  private readonly body: HTMLElement;
  private readonly menu: HTMLElement;
  private readonly picker: HTMLInputElement;
  private openState = false;
  private menuFor: string | null = null;
  private confirmFor: string | null = null;
  /** What the open row confirm asks: delete (S10.4) or load over unsaved changes (S10.3). */
  private confirmKind: "delete" | "load" = "delete";
  private incoming: Incoming | null = null;
  private incomingSeq = 0;
  /** A control to focus after the next render (a panel's first action). */
  private focusNext: string | null = null;
  private readonly dropSub: HTMLElement;
  private renaming: string | null = null;
  private dragDepth = 0;
  private sig = "";

  constructor(p: Persistence, onClose: () => void) {
    this.p = p;
    const close = h("button", {
      class: "sheet__close",
      type: "button",
      "aria-label": "Close worlds",
      "data-tip": "Close|Esc",
    });
    close.append(icon("close"));
    close.addEventListener("click", onClose);
    this.dropSub = h("small", {});
    const drop = h(
      "div",
      { class: "drop", "aria-hidden": "true" },
      icon("import"),
      "Drop a .hexworld file to import it",
      this.dropSub,
    );
    this.body = h("div", { class: "sheet__body" });
    this.picker = h("input", { type: "file", accept: ".hexworld", class: "sr", tabindex: "-1", "aria-hidden": "true" });
    this.picker.addEventListener("change", () => {
      const f = this.picker.files?.[0];
      if (f) void this.importFile(f);
      this.picker.value = "";
    });
    this.el = h(
      "section",
      { class: "sheet sheet--worlds is-closed", "data-region": "worlds", "aria-label": "Worlds" },
      drop,
      h("header", { class: "sheet__head" }, h("h1", {}, "Worlds"), close),
      this.body,
      this.picker,
    );
    this.menu = h("div", { class: "menu", role: "menu" });
    this.el.addEventListener("click", (e) => this.onClick(e));
    this.menu.addEventListener("click", (e) => this.onMenu(e));
    document.addEventListener("pointerdown", (e) => {
      if (this.menuFor && !(e.target as Element).closest?.(".menu, [data-more]")) this.closeMenu();
    });
    this.wireDrop();
  }

  /** The row menu lives at the HUD root, so it can sit over the sheet edge. */
  get menuEl(): HTMLElement {
    return this.menu;
  }

  get open(): boolean {
    return this.openState;
  }

  set open(v: boolean) {
    this.openState = v;
    this.el.classList.toggle("is-closed", !v);
    if (!v) {
      this.closeMenu();
      this.closeConfirm();
      this.incoming = null;
      this.renaming = null;
    } else void this.p.refresh().then(() => this.render(true));
    this.render(true);
  }

  /** Esc: a menu, an import panel, a confirm or a rename first, then the sheet. Returns whether it used the key. */
  escape(): boolean {
    if (this.menuFor) this.closeMenu();
    else if (this.incoming) {
      this.incoming = null;
      this.focusNext = "import";
      this.render(true);
    } else if (this.confirmFor || this.renaming) {
      this.closeConfirm();
      this.renaming = null;
      this.render(true);
    } else return false;
    return true;
  }

  /** Closes the row confirm; a load confirm lets the autosave it held run again. */
  private closeConfirm(): void {
    if (this.confirmFor && this.confirmKind === "load") this.p.releaseAutosave();
    this.confirmFor = null;
  }

  pickFile(): void {
    this.picker.click();
  }

  /** Rebuilds the body when what it shows changed (or `force`). Cheap to call every frame. */
  render(force = false): void {
    if (!this.openState) return;
    const p = this.p;
    const sig = [
      p.current.id,
      p.current.name,
      p.tilesNow,
      p.state,
      p.savedAt,
      p.storageOn,
      p.slots.map((m) => `${m.id}:${m.name}:${m.updatedAt}`).join(),
      this.confirmFor,
      this.confirmKind,
      this.incoming?.seq,
      this.renaming,
      Math.floor(Date.now() / 10_000),
    ].join("|");
    if (!force && sig === this.sig) return;
    this.sig = sig;
    const focusKey = this.focusNext ?? (document.activeElement as HTMLElement | null)?.dataset?.focus;
    const scrollTo = this.focusNext !== null;
    this.focusNext = null;
    this.dropSub.textContent = `You choose whether it replaces ${p.current.name} or opens as a new world.`;
    this.body.replaceChildren(...this.content());
    if (focusKey) {
      const el = this.body.querySelector<HTMLElement>(`[data-focus="${focusKey}"]`);
      el?.focus({ preventScroll: true });
      if (scrollTo) (el?.closest(".import, .slot-row") ?? el)?.scrollIntoView({ block: "nearest" });
    }
    const input = this.body.querySelector<HTMLInputElement>(".rename");
    if (input && document.activeElement !== input) {
      input.focus();
      input.select();
    }
  }

  private content(): Node[] {
    const p = this.p;
    const out: Node[] = [];
    if (!p.storageOn)
      out.push(
        h(
          "div",
          { class: "note note--caution", role: "status" },
          icon("alert"),
          h(
            "div",
            {},
            h("b", {}, "Autosave is off in this window"),
            "This browser blocks storage here, which private windows often do. Exporting a .hexworld file keeps your world.",
          ),
        ),
      );
    const cur = p.slots.find((m) => m.id === p.current.id);
    const tiles = p.tilesNow;
    const line = !p.storageOn
      ? `${plural(tiles, "tile")} · ${stateWords("off")}`
      : p.state === "saving"
        ? `${plural(tiles, "tile")} · Saving…`
        : p.savedAt
          ? `${plural(tiles, "tile")} · Saved ${whenText(p.savedAt, Date.now())}`
          : `${plural(tiles, "tile")} · ${stateWords(p.state)}`;
    const exportBtn = h("button", {
      class: "btn btn--sm",
      type: "button",
      "data-act": "export",
      "data-focus": "export",
    });
    exportBtn.append(icon("export"), "Export .hexworld");
    const dupBtn = h("button", {
      class: "btn btn--sm btn--quiet",
      type: "button",
      "data-act": "duplicate",
      "data-focus": "duplicate",
    });
    dupBtn.append(icon("copy"), "Duplicate");
    out.push(
      h(
        "div",
        { class: "current" },
        worldMap(p.mapNow),
        h("div", {}, this.nameLine(p.current.id, p.current.name, "h2"), h("p", {}, line), mixStrip(p.mixNow)),
        h("div", { class: "btn-row" }, exportBtn, p.storageOn ? dupBtn : null),
      ),
    );

    const others = p.slots.filter((m) => m.id !== p.current.id);
    out.push(
      h(
        "h2",
        { class: "section-h" },
        "Saved in this browser",
        p.storageOn ? h("span", { class: "section-h__count" }, String(others.length)) : null,
      ),
    );
    if (!p.storageOn)
      out.push(
        h(
          "p",
          { class: "empty-slots" },
          "Saved worlds live in this browser's storage, which is blocked in this window.",
        ),
      );
    else if (!others.length)
      out.push(
        h(
          "p",
          { class: "empty-slots" },
          cur ? "Only this world so far. New world starts another." : "Your world saves here as you build.",
        ),
      );
    else out.push(h("ul", { class: "slots" }, ...others.map((m, i) => this.row(m, i))));

    if (this.incoming?.kind === "preview") out.push(this.previewPanel(this.incoming));
    else if (this.incoming?.kind === "refused") out.push(this.refusedPanel(this.incoming.refused));

    const add = h("button", { class: "btn", type: "button", "data-act": "new", "data-focus": "new" });
    add.append(icon("plus"), "New world");
    const imp = h("button", { class: "btn", type: "button", "data-act": "import", "data-focus": "import" });
    imp.append(icon("import"), "Import .hexworld");
    out.push(h("div", { class: "foot-actions" }, add, imp));
    return out;
  }

  /** S10.6: the incoming file's world as a legend of its tiles, before it replaces or joins. */
  private previewPanel(inc: Extract<Incoming, { kind: "preview" }>): HTMLElement {
    const pv = inc.preview;
    const rows = pv.rows.map(({ kind, tiles }) =>
      h("li", {}, h("span", {}, h("i", { class: KIND_CLASS[kind] }), kind), h("span", {}, String(tiles))),
    );
    rows.push(h("li", {}, h("span", {}, "Tiles"), h("span", {}, String(pv.tiles.length))));
    if (inc.savedAt) rows.push(h("li", {}, h("span", {}, "Saved"), h("span", {}, dayText(inc.savedAt))));
    return h(
      "div",
      { class: "import", role: "region", "aria-label": "Import preview", "data-import": "preview" },
      h("h2", {}, pv.name),
      h("p", { class: "import__file" }, icon("file"), h("span", {}, pv.file)),
      mixStrip(mixOf(pv.tiles)),
      h("ul", { class: "legend-rows" }, ...rows),
      h(
        "div",
        { class: "btn-row" },
        btn("btn btn--ink btn--sm", "import-new", "Open as a new world"),
        this.p.ready ? btn("btn btn--sm", "import-replace", `Replace ${this.p.current.name}`) : null,
        btn("btn btn--quiet btn--sm", "import-cancel", "Cancel"),
      ),
    );
  }

  /** S10.8: what the decoder read, what failed and what it never reached, then what to do. */
  private refusedPanel(r: Refused): HTMLElement {
    const ic = { ok: "check", bad: "x", skip: "more" } as const;
    return h(
      "div",
      { class: "import import--error", role: "alert", "data-import": "refused" },
      h("h2", {}, r.title),
      h("p", { class: "import__file" }, icon("file"), h("span", {}, r.file)),
      h(
        "ul",
        { class: "checks" },
        ...r.checks.map((c) =>
          h("li", { class: c.state, "data-check": c.state }, icon(ic[c.state]), h("span", {}, c.text)),
        ),
      ),
      h("p", { class: "import__todo" }, r.todo),
      h(
        "div",
        { class: "btn-row" },
        btn("btn btn--ink btn--sm", "import-again", "Choose another file", "import"),
        btn("btn btn--quiet btn--sm", "import-cancel", "Cancel"),
      ),
    );
  }

  private nameLine(id: string, name: string, tag: "h2" | "b"): HTMLElement {
    if (this.renaming === id) {
      const input = h("input", {
        class: "rename",
        type: "text",
        value: name,
        maxlength: 40,
        "aria-label": `Rename ${name}`,
        "data-id": id,
        spellcheck: "false",
      });
      input.addEventListener("keydown", (e) => {
        if (e.key === "Enter") {
          e.preventDefault();
          void this.commitRename(input);
        } else if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          this.renaming = null;
          this.render(true);
        }
      });
      input.addEventListener("blur", () => {
        if (this.renaming === id) void this.commitRename(input);
      });
      return h(tag === "h2" ? "h2" : "div", { class: "name-edit" }, input);
    }
    if (tag === "b") return h("b", {}, name);
    const pencil = h("button", {
      class: "btn btn--quiet icon-btn",
      type: "button",
      "aria-label": `Rename ${name}`,
      "data-tip": "Rename",
      "data-act": "rename",
      "data-id": id,
      "data-focus": `rename-${id}`,
    });
    pencil.append(icon("pencil"));
    return h("h2", {}, h("span", {}, name), pencil);
  }

  private row(m: SlotMeta, i: number): HTMLElement {
    const load = h(
      "button",
      { class: "btn btn--sm", type: "button", "data-act": "load", "data-id": m.id, "data-focus": `load-${m.id}` },
      "Load",
    );
    const more = h("button", {
      class: "btn btn--quiet icon-btn",
      type: "button",
      "aria-label": `More for ${m.name}`,
      "aria-haspopup": "menu",
      "data-more": m.id,
      "data-focus": `more-${m.id}`,
    });
    more.append(icon("more"));
    const li = h(
      "li",
      { class: `slot-row${this.confirmFor === m.id ? " is-open" : ""}`, "data-slot": m.id },
      worldMap(m.map),
      h(
        "div",
        { class: "slot-row__text" },
        this.nameLine(m.id, m.name, "b"),
        h("small", {}, `${plural(m.tiles, "tile")} · ${whenText(m.updatedAt, Date.now())}`),
        mixStrip(m.mix),
      ),
      load,
      more,
    );
    li.style.animationDelay = `${Math.min(i, 8) * 30}ms`;
    if (this.confirmFor === m.id && this.confirmKind === "load") {
      const cur = this.p.current.name;
      li.append(
        h(
          "div",
          { class: "confirm", role: "alert" },
          h("b", {}, `Load ${m.name}?`),
          unsavedText(cur, this.p.unsavedSince ?? Date.now(), Date.now()),
          h(
            "div",
            { class: "btn-row" },
            btn("btn btn--ink btn--sm", "load-save", `Save ${cur}, then load`, undefined, m.id),
            btn("btn btn--sm", "load-discard", "Load without saving", undefined, m.id),
            btn("btn btn--quiet btn--sm", "load-cancel", "Cancel"),
          ),
        ),
      );
    } else if (this.confirmFor === m.id) {
      const del = h("button", {
        class: "btn btn--bad btn--sm",
        type: "button",
        "data-act": "delete",
        "data-id": m.id,
        "data-focus": `delete-${m.id}`,
      });
      del.append(icon("trash"), `Delete ${m.name}`);
      li.append(
        h(
          "div",
          { class: "confirm", role: "alert" },
          h("b", {}, `Delete ${m.name}?`),
          `Its ${plural(m.tiles, "tile")} and undo history are removed from this browser. Files you exported stay where they are.`,
          h(
            "div",
            { class: "btn-row" },
            del,
            h("button", { class: "btn btn--quiet btn--sm", type: "button", "data-act": "keep" }, "Keep it"),
          ),
        ),
      );
    }
    return li;
  }

  private onClick(e: MouseEvent): void {
    const t = e.target as Element;
    const more = t.closest<HTMLElement>("[data-more]");
    if (more) {
      e.stopPropagation();
      this.toggleMenu(more);
      return;
    }
    const b = t.closest<HTMLElement>("[data-act]");
    if (!b) return;
    const id = b.dataset.id ?? "";
    switch (b.dataset.act) {
      case "export":
        this.p.exportCurrent();
        break;
      case "rename":
        this.renaming = id;
        this.render(true);
        break;
      case "duplicate":
        void this.p.duplicate(this.p.current.id);
        break;
      case "load":
        if (this.p.unsaved) {
          // S10.3: ask first, holding the autosave so the question stays true while it is open.
          this.closeConfirm();
          this.p.holdAutosave();
          this.confirmFor = id;
          this.confirmKind = "load";
          this.focusNext = "load-save";
          this.render(true);
        } else this.load(id, "save");
        break;
      case "load-save":
      case "load-discard":
        this.confirmFor = null;
        this.load(id, b.dataset.act === "load-save" ? "save" : "discard");
        this.render(true);
        break;
      case "load-cancel":
        this.closeConfirm();
        this.render(true);
        break;
      case "import-new":
      case "import-replace": {
        const inc = this.incoming;
        this.incoming = null;
        if (inc?.kind === "preview")
          this.p.importFile(inc.file, inc.bytes, b.dataset.act === "import-new" ? "new" : "replace");
        this.render(true);
        break;
      }
      case "import-again":
        this.pickFile();
        break;
      case "import-cancel":
        this.incoming = null;
        this.focusNext = "import";
        this.render(true);
        break;
      case "delete":
        void this.p.remove(id).then(() => {
          this.confirmFor = null;
          this.render(true);
        });
        break;
      case "keep":
        this.closeConfirm();
        this.render(true);
        break;
      case "new":
        this.p.newWorld();
        break;
      case "import":
        this.pickFile();
        break;
      default:
        break;
    }
  }

  private toggleMenu(btn: HTMLElement): void {
    const id = btn.dataset.more ?? null;
    if (this.menuFor === id) {
      this.closeMenu();
      return;
    }
    this.menuFor = id;
    const m = this.p.slots.find((x) => x.id === id);
    if (!m) return;
    const item = (act: string, ic: string, label: string, bad = false) => {
      const b = h("button", { role: "menuitem", type: "button", "data-menu-act": act, class: bad ? "is-bad" : null });
      b.append(icon(ic), label);
      return b;
    };
    this.menu.replaceChildren(
      item("rename", "pencil", "Rename"),
      item("duplicate", "copy", "Duplicate"),
      item("export", "export", "Export .hexworld"),
      item("delete", "trash", "Delete", true),
    );
    const r = btn.getBoundingClientRect();
    this.menu.style.left = `${Math.round(r.right - 188)}px`;
    this.menu.style.top = `${Math.round(r.bottom + 4)}px`;
    this.menu.classList.add("is-open");
    this.menu.querySelector<HTMLElement>("button")?.focus();
  }

  private closeMenu(): void {
    this.menuFor = null;
    this.menu.classList.remove("is-open");
  }

  private onMenu(e: MouseEvent): void {
    const b = (e.target as Element).closest<HTMLElement>("[data-menu-act]");
    const id = this.menuFor;
    if (!b || !id) return;
    this.closeMenu();
    switch (b.dataset.menuAct) {
      case "rename":
        this.renaming = id;
        break;
      case "export":
        void this.p.exportSlot(id);
        break;
      case "duplicate":
        void this.p.duplicate(id);
        break;
      case "delete":
        this.closeConfirm();
        this.confirmFor = id;
        this.confirmKind = "delete";
        break;
      default:
        break;
    }
    this.render(true);
  }

  private async commitRename(input: HTMLInputElement): Promise<void> {
    const id = input.dataset.id ?? "";
    this.renaming = null;
    await this.p.rename(id, input.value);
    this.render(true);
  }

  private load(id: string, changes: "save" | "discard"): void {
    const name = this.p.slots.find((m) => m.id === id)?.name ?? "";
    void this.p.loadSlot(id, changes).then((ok) => {
      if (ok) this.p.notifyLoaded(name);
    });
  }

  /** Reads a picked or dropped file and shows its preview or its refusal; nothing loads yet. */
  private async importFile(f: File): Promise<void> {
    const bytes = new Uint8Array(await f.arrayBuffer());
    const r = this.p.inspect(bytes);
    const seq = ++this.incomingSeq;
    if (r.ok) {
      const preview = previewOf(f.name, bytes.length, this.p.importName(f.name), r);
      this.incoming = { kind: "preview", seq, file: f.name, bytes, savedAt: f.lastModified, preview };
      this.focusNext = "import-new";
    } else {
      this.incoming = { kind: "refused", seq, refused: refusedOf(f.name, bytes.subarray(0, 16), bytes.length, r) };
      this.focusNext = "import-again";
    }
    if (!this.openState) this.onWantOpen?.();
    this.render(true);
  }

  private wireDrop(): void {
    const hasFiles = (e: DragEvent) => !!e.dataTransfer && [...e.dataTransfer.types].includes("Files");
    // At the window: Chromium navigates to a file dropped anywhere the page does not claim, which
    // would unload the game.
    addEventListener("dragenter", (e) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      this.dragDepth++;
      if (!this.openState) this.onWantOpen?.();
      this.el.classList.add("is-drop");
    });
    addEventListener("dragleave", () => {
      if (--this.dragDepth <= 0) {
        this.dragDepth = 0;
        this.el.classList.remove("is-drop");
      }
    });
    addEventListener("dragover", (e) => {
      if (hasFiles(e)) e.preventDefault();
    });
    addEventListener("drop", (e) => {
      if (!hasFiles(e)) return;
      e.preventDefault();
      this.dragDepth = 0;
      this.el.classList.remove("is-drop");
      const f = e.dataTransfer?.files[0];
      if (f) void this.importFile(f);
    });
  }

  /** Set by the HUD: opens the sheet (a file dragged over the page). */
  onWantOpen: (() => void) | null = null;
}
