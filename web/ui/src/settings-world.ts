// The Settings sheet's World tab: Start over clears the world on screen back to the start tile in
// place (Persistence.startOver: same name, same slot, no undo history), after an inline confirm in
// the Worlds sheet's delete style. Esc or Cancel closes the confirm and focus returns to the button.
import { h } from "./dom.ts";
import { icon } from "./icons.ts";
import type { Persistence } from "./persist.ts";
import { plural } from "./words.ts";

export class WorldPanel {
  readonly el: HTMLElement;
  private p: Persistence | null = null;
  private readonly name: HTMLElement;
  private readonly hint: HTMLElement;
  private readonly trigger: HTMLButtonElement;
  private readonly confirm: HTMLElement;
  private readonly confirmText: Text;
  private readonly cancel: HTMLButtonElement;
  private sig = "";

  constructor() {
    this.name = h("span", { class: "field__label" }, "This world");
    this.hint = h("span", { class: "field__hint" });
    this.trigger = h(
      "button",
      { class: "btn btn--sm", type: "button", "data-act": "start-over", disabled: true },
      icon("retry"),
      "Start over",
    );
    this.trigger.addEventListener("click", () => this.ask(true));
    const go = h(
      "button",
      { class: "btn btn--bad btn--sm", type: "button", "data-act": "start-over-confirm" },
      icon("retry"),
      "Start over",
    );
    go.addEventListener("click", () => {
      this.p?.startOver();
      this.ask(false);
    });
    this.cancel = h(
      "button",
      { class: "btn btn--quiet btn--sm", type: "button", "data-act": "start-over-cancel" },
      "Cancel",
    );
    this.cancel.addEventListener("click", () => this.ask(false));
    this.confirmText = document.createTextNode("");
    this.confirm = h(
      "div",
      { class: "confirm", role: "alert", hidden: true },
      h("b", {}, "Start over?"),
      this.confirmText,
      h("div", { class: "btn-row" }, go, this.cancel),
    );
    this.el = h(
      "div",
      { class: "panel", role: "tabpanel", hidden: true },
      h("h2", { class: "section-h" }, "Start over"),
      h("div", { class: "field" }, this.name, this.trigger, this.hint),
      this.confirm,
    );
  }

  attach(p: Persistence): void {
    this.p = p;
    this.render();
  }

  /** Whether the confirm is showing. */
  get asking(): boolean {
    return !this.confirm.hidden;
  }

  /** Opens or closes the confirm: open moves focus to Cancel, close returns it to Start over. */
  ask(open: boolean): void {
    if (open && (!this.p || this.trigger.disabled)) return;
    const was = this.asking;
    this.confirm.hidden = !open;
    this.render(true);
    if (open) this.cancel.focus({ preventScroll: true });
    else if (was && this.el.contains(document.activeElement)) this.trigger.focus({ preventScroll: true });
  }

  /** Esc: closes the confirm first. Returns whether it used the key. */
  escape(): boolean {
    if (!this.asking) return false;
    this.ask(false);
    return true;
  }

  /** The sheet closed or the tab changed: the confirm never waits hidden. */
  reset(): void {
    this.confirm.hidden = true;
  }

  /** Repaints the name and tile count when they change. Cheap to call every frame. */
  render(force = false): void {
    const p = this.p;
    const tiles = p?.tilesNow ?? 0;
    const sig = `${p?.current.name}|${tiles}|${p?.ready}`;
    if (!force && sig === this.sig) return;
    this.sig = sig;
    this.trigger.disabled = !p?.ready;
    if (!p) return;
    this.name.textContent = p.current.name;
    const what = tiles > 1 ? `Clears ${plural(tiles, "tile")} back to the start tile.` : "Only the start tile so far.";
    this.hint.textContent = `${what} The world keeps its name and its save.`;
    this.confirmText.textContent = `This clears every tile in ${p.current.name}. You can't undo it.`;
  }
}
