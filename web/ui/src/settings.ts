// Settings (S9), first slice: the Graphics tab's Tilt-shift section (S9.4, PRD §9.1), the game
// camera's player toggle and strength slider. Quality, Audio and Controls are not built yet. It reads
// `state().tilt` every frame, so the T key and scripts that change tilt-shift show here too, and
// writes through the `tilt` command.
import type { HexWorld, TiltState } from "./bridge.ts";
import { h } from "./dom.ts";
import { icon } from "./icons.ts";

export class Settings {
  readonly el: HTMLElement;
  private readonly hw: HexWorld;
  private readonly toggle: HTMLButtonElement;
  private readonly strength: HTMLInputElement;
  private readonly out: HTMLOutputElement;
  private readonly strengthField: HTMLElement;
  private shown = false;
  private sig = "";

  constructor(hw: HexWorld, onClose: () => void) {
    this.hw = hw;
    const close = h(
      "button",
      { class: "sheet__close", type: "button", "aria-label": "Close settings", "data-tip": "Close|Esc" },
      icon("close"),
    );
    close.addEventListener("click", onClose);
    this.toggle = h("button", {
      class: "switch",
      type: "button",
      role: "switch",
      "aria-checked": "false",
      "aria-label": "Tilt-shift",
    });
    this.toggle.addEventListener("click", () => {
      const on = this.toggle.getAttribute("aria-checked") !== "true";
      this.hw.cmd({ cmd: "tilt", on, strength: null });
    });
    this.strength = h("input", { type: "range", min: 0, max: 100, step: 1, "aria-label": "Tilt-shift strength" });
    this.out = h("output", { class: "field__value" });
    this.strength.addEventListener("input", () => {
      this.paintStrength();
      this.hw.cmd({ cmd: "tilt", on: null, strength: Number(this.strength.value) / 100 });
    });
    this.strengthField = h(
      "label",
      { class: "field field--slider" },
      h("span", { class: "field__label" }, "Strength"),
      this.out,
      this.strength,
    );
    this.el = h(
      "section",
      { class: "sheet sheet--wide sheet--settings is-closed", "data-region": "settings", "aria-label": "Settings" },
      h("header", { class: "sheet__head" }, h("h1", {}, "Settings"), close),
      h(
        "div",
        { class: "sheet__body" },
        h("h2", { class: "section-h" }, "Tilt-shift"),
        h(
          "div",
          { class: "field" },
          h("span", { class: "field__label" }, "Miniature focus"),
          this.toggle,
          h("span", { class: "field__hint" }, "Blurs near and far like a diorama photo. On from Medium up."),
        ),
        this.strengthField,
      ),
    );
  }

  get open(): boolean {
    return this.shown;
  }

  set open(v: boolean) {
    this.shown = v;
    this.el.classList.toggle("is-closed", !v);
    if (v) this.toggle.focus({ preventScroll: true });
  }

  render(tilt: TiltState | undefined): void {
    if (!tilt || !this.shown) return;
    const sig = `${tilt.enabled}|${tilt.strength}`;
    if (sig === this.sig) return;
    this.sig = sig;
    this.toggle.setAttribute("aria-checked", String(tilt.enabled));
    if (document.activeElement !== this.strength) this.strength.value = String(Math.round(tilt.strength * 100));
    this.strength.disabled = !tilt.enabled;
    this.strengthField.classList.toggle("is-disabled", !tilt.enabled);
    this.paintStrength();
  }

  private paintStrength(): void {
    this.strength.style.setProperty("--pct", `${this.strength.value}%`);
    this.out.textContent = `${this.strength.value}%`;
  }
}
