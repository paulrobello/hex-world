// The photo panel's Effects section (owner, 2026-10-14): a colour filter, vignette, film grain and
// chromatic aberration, plus Reset effects. Every change goes to the engine as a `photo` command;
// the engine keeps the values for the session and shows them only in photo mode.
import type { PhotoFilter, PhotoState } from "./bridge.ts";
import { h } from "./dom.ts";
import { seg, setSlider, type Slider, slider } from "./photo.ts";
import {
  FILTERS,
  FX_DEFAULTS,
  type FxValues,
  fxDisabled,
  fxFrom,
  fxIsDefault,
  pctLabel,
  toPercent,
  toUnit,
} from "./photo-fx-model.ts";

type Amount = Exclude<keyof FxValues, "filter">;

export class PhotoFxSection {
  readonly el: HTMLElement;
  private readonly send: (patch: Partial<FxValues>) => void;
  private readonly filter: ReturnType<typeof seg<PhotoFilter>>;
  private readonly sliders: Record<Amount, Slider>;
  private readonly reset: HTMLButtonElement;
  private values: FxValues = { ...FX_DEFAULTS };

  constructor(send: (patch: Partial<FxValues>) => void) {
    this.send = send;
    this.filter = seg<PhotoFilter>(
      "Colour filter",
      FILTERS.map((f) => ({ value: f.value, text: f.text })),
      (v) => this.change({ filter: v }),
    );
    const pct = (label: string) => slider(label, 0, 100, 1, pctLabel);
    this.sliders = {
      filter_strength: pct("Filter strength"),
      vignette: pct("Vignette"),
      vignette_softness: pct("Vignette softness"),
      grain: pct("Film grain"),
      aberration: pct("Chromatic aberration"),
    };
    for (const [key, s] of Object.entries(this.sliders) as [Amount, Slider][]) {
      s.input.addEventListener("input", () => this.change({ [key]: toUnit(Number(s.input.value)) }));
    }
    this.reset = h("button", { class: "btn btn--quiet btn--sm fx-reset", type: "button" }, "Reset effects");
    this.reset.addEventListener("click", () => {
      this.change({ ...FX_DEFAULTS });
      this.paint();
    });
    const s = this.sliders;
    this.el = h(
      "div",
      { class: "photo-fx", role: "group", "aria-label": "Effects" },
      h("div", { class: "fx-head" }, h("h2", { class: "section-h" }, "Effects"), this.reset),
      this.filter.el,
      s.filter_strength.field,
      s.vignette.field,
      s.vignette_softness.field,
      s.grain.field,
      s.aberration.field,
    );
    this.paint();
  }

  /** Controls from the engine's values (on entering photo mode). */
  pull(p: PhotoState): void {
    this.values = fxFrom(p);
    this.paint();
  }

  private change(patch: Partial<FxValues>): void {
    this.values = { ...this.values, ...patch };
    this.send(patch);
    this.paintState();
  }

  private paint(): void {
    this.filter.set(this.values.filter);
    for (const [key, s] of Object.entries(this.sliders) as [Amount, Slider][])
      setSlider(s, toPercent(this.values[key]));
    this.paintState();
  }

  /** Inert controls dim, and Reset effects is off at the defaults. */
  private paintState(): void {
    const off = fxDisabled(this.values);
    for (const [s, inert] of [
      [this.sliders.filter_strength, off.strength],
      [this.sliders.vignette_softness, off.softness],
    ] as const) {
      s.field.classList.toggle("is-disabled", inert);
      s.input.disabled = inert;
    }
    this.reset.disabled = fxIsDefault(this.values);
  }
}
