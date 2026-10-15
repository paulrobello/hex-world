// A compact select (owner 2026-10-14: the colour grade as "a nice select box with the name and
// description of the item"): a button showing the current option, opening a listbox of every option
// with its description. The listbox keeps focus while open (aria-activedescendant marks the
// highlighted option) and is fixed to the viewport beside the button, so the sheet's scroll
// container never clips it; it closes when a scroll or a resize moves the button out from under it.
import { h } from "./dom.ts";
import { icon } from "./icons.ts";
import { selectKey, typeAhead, typeBuffer, typesAhead } from "./select-model.ts";

export interface SelectOption {
  id: string;
  name: string;
  detail: string;
  /** A short label after the name ("Default"), or null. */
  tag: string | null;
}

/** Keeps the list clear of the viewport edges, and apart from its button. */
const EDGE = 8;
const GAP = 4;
/** The list overhangs its button this much each side, so a slider's thumb ring never peeks past it. */
const OVERHANG = 2;

export class Select {
  /** The control: the button, and the list beside it while open. */
  readonly el: HTMLElement;
  readonly btn: HTMLButtonElement;
  private readonly list: HTMLUListElement;
  private readonly name: HTMLElement;
  private readonly detail: HTMLElement;
  private readonly key: string;
  private readonly pick: (id: string) => void;
  private options: SelectOption[] = [];
  private optSig = "";
  private selected = -1;
  private active = -1;
  private typed = "";
  private typedAt = 0;
  private readonly outside = (e: Event): void => {
    if (!this.el.contains(e.target as Node)) this.close();
  };
  /** Where the list was placed: the button's corner and the viewport size. */
  private anchor = { x: 0, y: 0, w: 0, h: 0 };
  /**
   * Scroll and resize events arrive a frame late, so one from just before opening must not close the
   * list: only a change to the geometry it was placed against does.
   */
  private readonly moved = (): void => {
    const r = this.btn.getBoundingClientRect();
    const a = this.anchor;
    if (Math.abs(r.left - a.x) > 1 || Math.abs(r.top - a.y) > 1 || innerWidth !== a.w || innerHeight !== a.h)
      this.close();
  };

  /** `key` names the control (`data-select`, element ids); `labelId` is its visible heading. */
  constructor(key: string, labelId: string, pick: (id: string) => void) {
    this.key = key;
    this.pick = pick;
    this.name = h("b", { id: `${key}-select-name` });
    this.detail = h("small", { id: `${key}-select-detail` });
    this.btn = h(
      "button",
      {
        type: "button",
        class: "select__btn",
        id: `${key}-select`,
        "aria-haspopup": "listbox",
        "aria-expanded": "false",
        "aria-controls": `${key}-options`,
        "aria-labelledby": `${labelId} ${key}-select-name`,
        "aria-describedby": `${key}-select-detail`,
      },
      h("span", { class: "select__value" }, this.name, this.detail),
      icon("down", "ic select__chev"),
    );
    this.list = h("ul", {
      class: "select__list",
      id: `${key}-options`,
      role: "listbox",
      tabindex: -1,
      "aria-labelledby": labelId,
      hidden: true,
    });
    this.el = h("div", { class: "select", "data-select": key }, this.btn, this.list);

    // A click from Enter or Space has no pointer (detail 0): that list opens in keyboard mode.
    this.btn.addEventListener("click", (e) => {
      if (this.isOpen) this.close();
      else this.open(this.selected, e.detail === 0);
    });
    this.btn.addEventListener("keydown", (e) => {
      const a = selectKey(false, this.selected, this.options.length, e.key);
      if (a?.kind !== "open") return;
      e.preventDefault();
      this.open(a.active, true);
    });
    this.list.addEventListener("keydown", (e) => this.onKey(e));
    // Hover highlights; a click picks. Pointer events cover mouse, pen and touch alike.
    this.list.addEventListener("pointermove", (e) => {
      this.list.classList.remove("is-keys");
      const i = this.indexAt(e.target);
      if (i >= 0 && i !== this.active) this.highlight(i);
    });
    this.list.addEventListener("click", (e) => {
      const i = this.indexAt(e.target);
      if (i >= 0) this.choose(i);
    });
    // Focus leaving for anything outside the control (a click elsewhere, the browser window) closes it.
    this.list.addEventListener("focusout", (e) => {
      if (this.isOpen && !this.el.contains(e.relatedTarget as Node | null)) this.close(false);
    });
  }

  get isOpen(): boolean {
    return !this.list.hidden;
  }

  /**
   * The options and the current one. The list is rebuilt only when the options change; a new current
   * option (a pick, a script, a loaded save) moves the mark in place, leaving an open list's
   * highlight and scroll where they are.
   */
  update(options: SelectOption[], selectedId: string | null): void {
    const sig = JSON.stringify(options);
    if (sig !== this.optSig) {
      this.optSig = sig;
      const activeId = this.options[this.active]?.id;
      this.options = options;
      this.list.replaceChildren(
        ...options.map((o) =>
          h(
            "li",
            {
              class: "select__opt",
              role: "option",
              id: `${this.key}-opt-${o.id}`,
              "aria-selected": "false",
              "data-value": o.id,
            },
            h("b", {}, o.name),
            o.tag ? h("span", { class: "tag" }, o.tag) : null,
            icon("check", "ic select__mark"),
            h("small", {}, o.detail),
          ),
        ),
      );
      this.selected = -1;
      this.active = -1;
      if (this.isOpen)
        this.highlight(
          Math.max(
            0,
            options.findIndex((o) => o.id === activeId),
          ),
        );
    }
    const sel = options.findIndex((o) => o.id === selectedId);
    if (sel !== this.selected || this.name.textContent !== (options[sel]?.name ?? "")) {
      this.selected = sel;
      for (const [i, li] of [...this.list.children].entries()) li.setAttribute("aria-selected", String(i === sel));
      this.name.textContent = options[sel]?.name ?? "";
      this.detail.textContent = options[sel]?.detail ?? "";
    }
    if (options.length === 0) this.close(false);
  }

  /**
   * Opens the list with option `active` highlighted, and moves focus into it. From the keyboard
   * (`keys`) the highlight carries the focus ring; under a pointer it is a quiet fill.
   */
  open(active: number, keys = false): void {
    if (this.options.length === 0) return;
    this.list.classList.toggle("is-keys", keys);
    this.list.hidden = false;
    this.btn.setAttribute("aria-expanded", "true");
    this.el.classList.add("is-open");
    this.place();
    this.typed = "";
    this.highlight(Math.min(this.options.length - 1, Math.max(0, active)));
    this.list.focus({ preventScroll: true });
    document.addEventListener("pointerdown", this.outside, true);
    addEventListener("scroll", this.moved, true);
    addEventListener("resize", this.moved);
  }

  /**
   * Closes the list without a change; focus inside it returns to the button unless `refocus` is
   * false. Returns whether it was open.
   */
  close(refocus = true): boolean {
    if (!this.isOpen) return false;
    const hadFocus = this.list.contains(document.activeElement);
    this.list.hidden = true;
    this.btn.setAttribute("aria-expanded", "false");
    this.el.classList.remove("is-open", "is-up");
    this.list.removeAttribute("aria-activedescendant");
    document.removeEventListener("pointerdown", this.outside, true);
    removeEventListener("scroll", this.moved, true);
    removeEventListener("resize", this.moved);
    if (refocus && hadFocus) this.btn.focus({ preventScroll: true });
    return true;
  }

  private choose(i: number): void {
    const id = this.options[i]?.id;
    this.close();
    this.btn.focus({ preventScroll: true });
    if (id != null) this.pick(id);
  }

  private onKey(e: KeyboardEvent): void {
    this.list.classList.add("is-keys");
    const now = performance.now();
    if (!e.ctrlKey && !e.metaKey && !e.altKey && typesAhead(e.key, this.typed, this.typedAt, now)) {
      e.preventDefault();
      e.stopPropagation();
      this.typed = typeBuffer(this.typed, this.typedAt, e.key, now);
      this.typedAt = now;
      const i = typeAhead(
        this.options.map((o) => o.name),
        this.active,
        this.typed,
      );
      if (i != null) this.highlight(i);
      return;
    }
    const a = selectKey(true, this.active, this.options.length, e.key);
    if (!a) return;
    // Tab closes and lets the browser move on from the button; everything else stays here.
    if (e.key !== "Tab") {
      e.preventDefault();
      e.stopPropagation();
    }
    if (a.kind === "move") this.highlight(a.active);
    else if (a.kind === "pick") this.choose(a.index);
    else if (a.kind === "close") {
      this.close(false);
      this.btn.focus({ preventScroll: true });
    }
  }

  private highlight(i: number): void {
    const items = this.list.children;
    items[this.active]?.classList.remove("is-active");
    this.active = i;
    const li = items[i] as HTMLElement | undefined;
    if (!li) return;
    li.classList.add("is-active");
    this.list.setAttribute("aria-activedescendant", li.id);
    // By hand, not scrollIntoView: that would also scroll the sheet, which closes the list.
    const top = li.offsetTop;
    const bottom = top + li.offsetHeight;
    const pad = 4;
    if (top - pad < this.list.scrollTop) this.list.scrollTop = top - pad;
    else if (bottom + pad > this.list.scrollTop + this.list.clientHeight)
      this.list.scrollTop = bottom + pad - this.list.clientHeight;
  }

  private indexAt(t: EventTarget | null): number {
    const li = (t as Element | null)?.closest?.(".select__opt");
    return li ? [...this.list.children].indexOf(li) : -1;
  }

  /**
   * Beside the button, as wide as it, inside the viewport: below it when the whole list fits there or
   * there is more room below than above, else above it; taller than the room, it scrolls.
   */
  private place(): void {
    const r = this.btn.getBoundingClientRect();
    this.anchor = { x: r.left, y: r.top, w: innerWidth, h: innerHeight };
    const s = this.list.style;
    const width = Math.min(r.width + 2 * OVERHANG, innerWidth - 2 * EDGE);
    s.setProperty("width", `${width}px`);
    s.setProperty("left", `${Math.max(EDGE, Math.min(r.left - OVERHANG, innerWidth - EDGE - width))}px`);
    s.setProperty("max-height", "none");
    const natural = this.list.offsetHeight;
    const below = innerHeight - r.bottom - GAP - EDGE;
    const above = r.top - GAP - EDGE;
    const up = natural > below && above > below;
    const height = Math.max(0, Math.min(natural, up ? above : below));
    s.setProperty("max-height", `${height}px`);
    s.setProperty("top", `${up ? r.top - GAP - height : r.bottom + GAP}px`);
    this.el.classList.toggle("is-up", up);
  }
}
