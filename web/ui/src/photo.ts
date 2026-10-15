// Photo mode (S8, PRD §4.6): the photo bar with free-camera keys, the Photo sheet (depth of field,
// camera, world) and PNG export. It lives in its own overlay root, so the play HUD hiding for photo
// mode never takes it along, and H hides only this root. Every change goes to the engine as a
// `photo` or `photo_export` command; the engine owns the camera. A click on the scene focuses Tilt
// and Manual on the surface under it (photo-focus.ts tells it from the drag that looks around).
import type { Dof, GameEvent, HexWorld, PhotoState } from "./bridge.ts";
import { h } from "./dom.ts";
import { icon } from "./icons.ts";
import { ClickDetector, FOCUS_MAX, FOCUS_MIN, focusAt, focusSliderValue } from "./photo-focus.ts";

const STOPS = [1.4, 2, 2.8, 4, 5.6, 8, 11, 16, 22];
const SCALES = [1, 2, 4] as const;
type Scale = (typeof SCALES)[number];
/** The engine refuses 4× on the web until a 4× export there is tested for GPU memory (photo.rs). */
const WEB_MAX_SCALE: Scale = 2;
const OVER_WEB_MAX = "4× is off on the web until it is tested for GPU memory.";
const TOAST_MS = 4000;
const BAND_MS = 1000;
/** Rough PNG bytes per pixel for a lit diorama: the 5120 × 3200 native proof was 20.3 MB, 1.24 B/px. */
const PNG_BYTES_PER_PX = 1.25;

const stopLabel = (f: number): string => `f/${f}`;
const signed = (v: number, digits: number, unit: string): string =>
  `${v > 0 ? "+" : v < 0 ? "−" : ""}${Math.abs(v).toFixed(digits)}${unit}`;
const dims = (w: number, hgt: number): string => `${w} × ${hgt}`;
const today = (): string => {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}`;
};

interface Slider {
  input: HTMLInputElement;
  out: HTMLOutputElement;
  field: HTMLElement;
}

/** A labelled range field (Legend slider): label and value on one line, the track below. */
function slider(label: string, min: number, max: number, step: number, fmt: (v: number) => string): Slider {
  const input = h("input", { type: "range", min, max, step, "aria-label": label });
  const out = h("output", { class: "field__value" });
  const field = h("label", { class: "field field--slider" }, h("span", { class: "field__label" }, label), out, input);
  const paint = () => {
    const pct = ((Number(input.value) - min) / (max - min)) * 100;
    input.style.setProperty("--pct", `${pct}%`);
    out.textContent = fmt(Number(input.value));
  };
  input.addEventListener("input", paint);
  (input as HTMLInputElement & { paint?: () => void }).paint = paint;
  return { input, out, field };
}

function setSlider(s: Slider, v: number): void {
  s.input.value = String(v);
  (s.input as HTMLInputElement & { paint?: () => void }).paint?.();
}

/** A segmented radio group (Legend seg); `pick` fires with the chosen value. An option with `off`
 * is disabled, with `off` as its reason, and the arrow keys pass over it. */
function seg<T extends string>(
  label: string,
  options: { value: T; text: string; sub?: string; off?: string | undefined }[],
  pick: (v: T) => void,
): { el: HTMLElement; set: (v: T) => void; subs: Map<T, HTMLElement> } {
  const el = h("div", { class: "seg", role: "radiogroup", "aria-label": label });
  const buttons = new Map<T, HTMLButtonElement>();
  const subs = new Map<T, HTMLElement>();
  for (const o of options) {
    const b = h("button", { type: "button", role: "radio", "aria-checked": "false", "data-value": o.value }, o.text);
    if (o.off !== undefined) {
      b.disabled = true;
      b.title = o.off;
      b.setAttribute("aria-description", o.off);
    }
    if (o.sub !== undefined) {
      const small = h("small", { class: "export-dims" }, o.sub);
      subs.set(o.value, small);
      b.append(small);
    }
    b.addEventListener("click", () => {
      set(o.value);
      pick(o.value);
    });
    buttons.set(o.value, b);
    el.append(b);
  }
  const set = (v: T) => {
    for (const [k, b] of buttons) b.setAttribute("aria-checked", String(k === v));
  };
  el.addEventListener("keydown", (e) => {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    const keys = [...buttons.keys()].filter((k) => !buttons.get(k)?.disabled);
    const i = keys.findIndex((k) => buttons.get(k)?.getAttribute("aria-checked") === "true");
    const n = keys[(i + (e.key === "ArrowRight" ? 1 : keys.length - 1)) % keys.length]!;
    set(n);
    pick(n);
    buttons.get(n)?.focus();
  });
  return { el, set, subs };
}

export class Photo {
  readonly root: HTMLElement;
  private readonly hw: HexWorld;
  private readonly canvas: HTMLCanvasElement | null;
  private readonly band: HTMLElement;
  private readonly bandLabel: HTMLElement;
  private readonly mark: HTMLElement;
  private readonly clicks = new ClickDetector();
  private readonly toast: HTMLElement;
  private readonly dof: ReturnType<typeof seg<Dof>>;
  private readonly scale: ReturnType<typeof seg<`${Scale}`>>;
  private readonly focus: Slider;
  private readonly aperture: Slider;
  private readonly exposure: Slider;
  private readonly fov: Slider;
  private readonly roll: Slider;
  private readonly pause: HTMLButtonElement;
  private readonly file: HTMLElement;
  private readonly size: HTMLElement;
  private readonly exportBox: HTMLElement;
  private readonly progressText: HTMLElement;
  private readonly failText: HTMLElement;
  private readonly retryBtn: HTMLButtonElement;

  private on = false;
  private hidden = false;
  private chosen: Scale = 2;
  private job: { name: string; scale: Scale; file: string; cancelled: boolean } | null = null;
  private failedScale: Scale | null = null;
  private bandTimer = 0;
  private dofNow: Dof = "tilt";
  /** The band's row from the engine, and its count of landed clicks when last seen. */
  private row = 0.5;
  private picks = 0;
  /** Where the last click sent to the engine was, for the marker once it lands. */
  private clickedAt: { x: number; y: number } | null = null;
  private toastTimer = 0;
  private maxTexture = 8192;

  constructor(hw: HexWorld) {
    this.hw = hw;
    this.canvas = document.querySelector<HTMLCanvasElement>("canvas#hex-world");
    this.root = h("div", { class: "hud photo-ui is-off", "aria-hidden": "true" });

    // Photo bar: title, free-camera keys, Hide UI and Exit.
    const hide = h(
      "button",
      { class: "tool", type: "button", "data-tip": "Hide UI|H" },
      icon("eyeoff"),
      h("span", {}, "Hide UI"),
      h("kbd", {}, "H"),
    );
    hide.addEventListener("click", () => this.setHidden(true));
    const exit = h(
      "button",
      { class: "tool", type: "button", "data-tip": "Back to building|P" },
      icon("exit"),
      h("span", {}, "Exit"),
      h("kbd", {}, "P"),
    );
    exit.addEventListener("click", () => this.exit());
    const bar = h(
      "nav",
      { class: "bar pbar", "data-region": "photo-bar", "aria-label": "Photo mode" },
      h("p", { class: "pbar__title" }, icon("photo"), "Photo mode"),
      h("span", { class: "bar__rule", "aria-hidden": "true" }),
      h(
        "p",
        { class: "pbar__keys" },
        h("span", {}, "Drag to look"),
        h("span", {}, "Click to focus"),
        h("span", {}, h("kbd", {}, "W"), h("kbd", {}, "A"), h("kbd", {}, "S"), h("kbd", {}, "D"), " fly"),
        h("span", {}, h("kbd", {}, "Q"), h("kbd", {}, "E"), " down, up"),
        h("span", {}, h("kbd", {}, "C"), " save PNG"),
      ),
      h("span", { class: "bar__rule", "aria-hidden": "true" }),
      hide,
      exit,
    );

    // Depth of field.
    this.dof = seg<Dof>(
      "Depth of field",
      [
        { value: "off", text: "Off" },
        { value: "tilt", text: "Tilt-shift" },
        { value: "manual", text: "Manual" },
      ],
      (v) => {
        this.send({ dof: v });
        this.syncDofFields(v);
      },
    );
    this.focus = slider("Focus distance", FOCUS_MIN, FOCUS_MAX, 1, (v) => `${v} m`);
    this.aperture = slider("Aperture", 0, STOPS.length - 1, 1, (v) => stopLabel(STOPS[v] ?? 2.8));
    this.exposure = slider("Exposure", -2, 2, 0.1, (v) => `${signed(v, 1, "")} EV`);
    this.fov = slider("Field of view", 20, 90, 1, (v) => `${v}°`);
    this.roll = slider("Roll", -15, 15, 1, (v) => signed(v, 0, "°"));
    for (const s of [this.focus, this.aperture]) s.field.dataset.dofField = "";
    this.focus.input.addEventListener("input", () => {
      this.send({ focus: Number(this.focus.input.value) });
      this.showBand();
    });
    this.aperture.input.addEventListener("input", () =>
      this.send({ aperture: STOPS[Number(this.aperture.input.value)] ?? 2.8 }),
    );
    this.exposure.input.addEventListener("input", () => this.send({ exposure: Number(this.exposure.input.value) }));
    this.fov.input.addEventListener("input", () => this.send({ fov: Number(this.fov.input.value) }));
    this.roll.input.addEventListener("input", () => this.send({ roll: Number(this.roll.input.value) }));

    // World.
    this.pause = h("button", {
      class: "switch",
      type: "button",
      role: "switch",
      "aria-checked": "true",
      "aria-label": "Pause cars and birds",
    });
    this.pause.addEventListener("click", () => {
      const on = this.pause.getAttribute("aria-checked") !== "true";
      this.pause.setAttribute("aria-checked", String(on));
      this.send({ pause_agents: on });
    });
    const tod = h("input", { type: "range", min: 0, max: 24, value: 12, disabled: true, "aria-label": "Time of day" });
    tod.style.setProperty("--pct", "50%");

    // Export.
    this.scale = seg<`${Scale}`>(
      "Resolution",
      SCALES.map((s) => ({
        value: `${s}` as `${Scale}`,
        text: `${s}×`,
        sub: "",
        off: s > WEB_MAX_SCALE ? OVER_WEB_MAX : undefined,
      })),
      (v) => {
        this.chosen = Number(v) as Scale;
        this.failedScale = null;
        this.paintExport();
      },
    );
    this.file = h("span", {});
    this.size = h("b", {});
    const go = h("button", { class: "btn btn--ink export-go", type: "button" }, icon("export"), "Save PNG");
    go.addEventListener("click", () => this.export(this.chosen));
    this.progressText = h("span", {});
    const cancel = h("button", { class: "btn btn--sm", type: "button" }, "Cancel");
    cancel.addEventListener("click", () => {
      if (this.job) this.job.cancelled = true;
      this.job = null;
      this.paintExport();
    });
    this.failText = h("div", {});
    this.retryBtn = h("button", { class: "btn btn--ink btn--sm", type: "button" });
    this.retryBtn.addEventListener("click", () => {
      const next = this.failedScale === 4 ? 2 : 1;
      this.chosen = next;
      this.scale.set(`${next}`);
      this.export(next);
    });
    const dismiss = h("button", { class: "btn btn--quiet btn--sm", type: "button" }, "Dismiss");
    dismiss.addEventListener("click", () => {
      this.failedScale = null;
      this.paintExport();
    });
    this.exportBox = h(
      "div",
      { class: "photo-export" },
      h("h2", { class: "section-h" }, "Save a PNG"),
      this.scale.el,
      h("p", { class: "field__hint export-cap" }, OVER_WEB_MAX),
      h("p", { class: "export-out" }, this.file, this.size),
      go,
      h(
        "div",
        { class: "exporting", role: "status" },
        h("p", {}, this.progressText, h("span", {}, "One offscreen pass")),
        h("span", { class: "progress is-indeterminate" }, h("i")),
        cancel,
      ),
      h(
        "div",
        { class: "note note--bad export-fail", role: "alert" },
        icon("alert"),
        h("div", {}, this.failText, h("div", { class: "btn-row" }, this.retryBtn, dismiss)),
      ),
    );

    const sheet = h(
      "section",
      { class: "sheet sheet--photo", "data-region": "photo-sheet", "aria-label": "Photo settings" },
      h("header", { class: "sheet__head" }, h("h1", {}, "Photo"), h("kbd", {}, "P")),
      h(
        "div",
        { class: "sheet__body" },
        h("h2", { class: "section-h" }, "Depth of field"),
        this.dof.el,
        this.focus.field,
        this.aperture.field,
        h("h2", { class: "section-h" }, "Camera"),
        this.exposure.field,
        this.fov.field,
        this.roll.field,
        h("h2", { class: "section-h" }, "World"),
        h("div", { class: "field" }, h("span", { class: "field__label" }, "Pause cars and birds"), this.pause),
        h(
          "label",
          { class: "field field--slider is-disabled" },
          h("span", { class: "field__label" }, "Time of day"),
          h("output", { class: "field__value" }, "Noon"),
          tod,
          h("span", { class: "field__hint" }, "Arrives with day and night."),
        ),
      ),
      this.exportBox,
    );

    this.band = h("div", { class: "focus-band", "aria-hidden": "true" });
    this.bandLabel = h("span", {});
    this.band.append(this.bandLabel);
    this.mark = h("div", { class: "focus-mark", "aria-hidden": "true" });
    this.toast = h("div", { class: "toast", role: "status", "aria-live": "polite" });
    const showUi = h("button", { class: "show-ui", type: "button" }, icon("eye"), "Show UI ", h("kbd", {}, "H"));
    showUi.addEventListener("click", () => this.setHidden(false));
    this.root.append(this.band, this.mark, bar, sheet, this.toast, showUi);

    // Click to focus: only presses on the canvas count, so the panel and bar (which take their own
    // pointer events above it) never refocus. A drag still looks around in the engine.
    const c = this.canvas;
    if (c) {
      const sample = (e: PointerEvent) => ({ x: e.clientX, y: e.clientY, t: e.timeStamp, button: e.button });
      c.addEventListener("pointerdown", (e) => {
        if (this.on) this.clicks.press(sample(e));
      });
      c.addEventListener("pointermove", (e) => this.clicks.move(sample(e)));
      c.addEventListener("pointercancel", () => this.clicks.cancel());
      c.addEventListener("pointerup", (e) => {
        if (!this.clicks.release(sample(e)) || !this.on || this.dofNow === "off") return;
        const uv = focusAt(e.clientX, e.clientY, c.getBoundingClientRect());
        if (!uv) return;
        this.clickedAt = { x: e.clientX, y: e.clientY };
        this.send({ focus_at: uv });
      });
    }

    // A mouse drag on a slider hands keyboard focus back to the canvas, so WASD keeps flying;
    // keyboard users keep focus on the slider and step it with the arrows.
    this.root.addEventListener("pointerup", (e) => {
      if ((e.target as Element).matches?.("input[type=range]") && e.pointerType === "mouse") this.canvas?.focus();
    });
    this.root.addEventListener("mousedown", (e) => {
      if ((e.target as Element).closest("button")) e.preventDefault();
    });
  }

  /** Engine state for this frame: `on` follows `play.photo_mode`; `photo` carries the panel values. */
  render(on: boolean, photo: PhotoState | undefined): void {
    if (photo?.max_texture) this.maxTexture = photo.max_texture;
    if (on && !this.on && photo?.on) {
      this.on = true;
      this.hidden = false;
      this.pull(photo);
      this.paintExport();
      this.picks = photo.focus_picks;
      this.row = photo.focus_row;
      this.root.classList.remove("is-off", "is-hidden");
      this.root.removeAttribute("aria-hidden");
    } else if (!on && this.on) {
      this.on = false;
      this.root.classList.add("is-off");
      this.root.setAttribute("aria-hidden", "true");
      this.band.classList.remove("is-shown");
      this.mark.classList.remove("is-shown");
      this.clicks.cancel();
    } else if (this.on && photo && photo.focus_picks !== this.picks) {
      // A click landed: the slider follows the engine's depth, the band its row.
      this.picks = photo.focus_picks;
      this.row = photo.focus_row;
      setSlider(this.focus, focusSliderValue(photo.focus));
      this.showBand();
      this.showMark();
    }
  }

  get active(): boolean {
    return this.on;
  }

  onEvents(batch: GameEvent[]): void {
    for (const e of batch) {
      if (e.type === "photoExported" && e.name === this.job?.name) void this.finish(e.name, e.width, e.height);
      if (e.type === "photoExportFailed" && e.name === this.job?.name) {
        const scale = this.job.scale;
        this.job = null;
        this.failedScale = scale;
        this.failText.replaceChildren(h("b", {}, `The ${scale}× render did not run`), e.reason);
        this.paintExport();
      }
    }
  }

  /** Keys while photo mode is on. Returns true when the key was taken. */
  key(e: KeyboardEvent): boolean {
    const k = e.key.length === 1 ? e.key.toLowerCase() : e.key;
    if (k === "p" || k === "Escape") {
      if (!e.repeat) this.exit();
      return true;
    }
    if (k === "h") {
      if (!e.repeat) this.setHidden(!this.hidden);
      return true;
    }
    // Saves a PNG at the chosen resolution, also while H hides the photo UI (owner 2026-10-14).
    if (k === "c") {
      if (!e.repeat) this.export(this.chosen);
      return true;
    }
    // Game keys are inert here; WASD, Q and E fly the camera in the engine.
    return k === "r" || k === "b" || k === "Tab" || k === "Enter" || /^[0-9=-]$/.test(k);
  }

  private exit(): void {
    this.hw.cmd({ cmd: "toggle", what: "photo" });
  }

  private setHidden(v: boolean): void {
    this.hidden = v;
    this.root.classList.toggle("is-hidden", v);
  }

  private send(values: Omit<Extract<Parameters<HexWorld["cmd"]>[0], { cmd: "photo" }>, "cmd">): void {
    this.hw.cmd({ cmd: "photo", ...values });
  }

  /** Controls from the engine's values on entering (focus and FOV follow the orbit camera). */
  private pull(p: PhotoState): void {
    this.dof.set(p.dof);
    this.syncDofFields(p.dof);
    setSlider(this.focus, focusSliderValue(p.focus));
    const stop = STOPS.reduce(
      (best, f, i) => (Math.abs(f - p.aperture) < Math.abs(STOPS[best]! - p.aperture) ? i : best),
      0,
    );
    setSlider(this.aperture, stop);
    setSlider(this.exposure, p.exposure);
    setSlider(this.fov, Math.round(p.fov));
    setSlider(this.roll, Math.round(p.roll));
    this.pause.setAttribute("aria-checked", String(p.pause_agents));
  }

  private syncDofFields(v: Dof): void {
    this.dofNow = v;
    for (const s of [this.focus, this.aperture]) {
      s.field.classList.toggle("is-disabled", v === "off");
      s.input.disabled = v === "off";
    }
  }

  private showBand(): void {
    this.bandLabel.textContent = `Focus ${this.focus.input.value} m`;
    // The band preview sits on the engine's band row (the canvas fills the viewport).
    this.band.style.top = `${this.row * 100}%`;
    this.band.classList.add("is-shown");
    clearTimeout(this.bandTimer);
    this.bandTimer = window.setTimeout(() => this.band.classList.remove("is-shown"), BAND_MS);
  }

  /** A ring where the last click focused, fading out. It is overlay DOM, so no PNG export has it. */
  private showMark(): void {
    const at = this.clickedAt;
    this.clickedAt = null;
    if (!at) return;
    this.mark.style.left = `${at.x}px`;
    this.mark.style.top = `${at.y}px`;
    this.mark.classList.remove("is-shown");
    void this.mark.offsetWidth;
    this.mark.classList.add("is-shown");
  }

  private sizeFor(scale: Scale): [number, number] {
    return [Math.round(innerWidth) * scale, Math.round(innerHeight) * scale];
  }

  private paintExport(): void {
    for (const s of SCALES) {
      const [w, hgt] = this.sizeFor(s);
      const sub = this.scale.subs.get(`${s}`);
      if (sub) sub.textContent = dims(w, hgt);
    }
    this.scale.set(`${this.chosen}`);
    const [w, hgt] = this.sizeFor(this.chosen);
    this.file.textContent = `hex-world-${today()}.png`;
    this.size.textContent = `about ${Math.max(1, Math.round((w * hgt * PNG_BYTES_PER_PX) / 1e6))} MB`;
    this.exportBox.classList.toggle("is-exporting", !!this.job);
    this.exportBox.classList.toggle("is-export-failed", !this.job && this.failedScale !== null);
    if (this.job) this.progressText.textContent = `Rendering ${this.job.scale}× offscreen`;
    if (this.failedScale !== null) {
      const next = this.failedScale === 4 ? 2 : 1;
      this.retryBtn.textContent = `Save at ${next}×`;
      this.retryBtn.hidden = this.failedScale === 1;
    }
    // A scale past this GPU's texture limit still runs, and the engine's refusal explains why.
    // Scales over the web cap are disabled as well as struck through.
    for (const s of SCALES) {
      const [sw, sh] = this.sizeFor(s);
      this.scale.subs.get(`${s}`)?.classList.toggle("is-over", s > WEB_MAX_SCALE || Math.max(sw, sh) > this.maxTexture);
    }
  }

  private export(scale: Scale): void {
    if (this.job || scale > WEB_MAX_SCALE) return;
    const stamp = new Date().toISOString().slice(11, 19).replaceAll(":", "");
    const name = `hex-world-${today()}-${stamp}`;
    this.job = { name, scale, file: `hex-world-${today()}.png`, cancelled: false };
    this.failedScale = null;
    this.paintExport();
    this.hw.cmd({ cmd: "photo_export", scale, name });
  }

  private async finish(name: string, w: number, hgt: number): Promise<void> {
    const job = this.job;
    if (!job || job.name !== name) return;
    let url: string | null = null;
    for (let i = 0; i < 100 && !url; i++) {
      url = this.hw.takeShot(name);
      if (!url) await new Promise((r) => setTimeout(r, 50));
    }
    this.job = null;
    this.paintExport();
    if (job.cancelled || !url) return;
    // The page's CSP keeps connect-src to 'self', so the data URL is decoded here, not fetched.
    const bytes = Uint8Array.from(atob(url.slice(url.indexOf(",") + 1)), (c) => c.charCodeAt(0));
    const blob = new Blob([bytes], { type: "image/png" });
    const href = URL.createObjectURL(blob);
    const a = h("a", { href, download: job.file });
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(href), 10_000);
    this.showToast(`Saved ${job.file}`, `${dims(w, hgt)}, in your downloads`);
  }

  private showToast(text: string, sub: string): void {
    this.toast.replaceChildren(icon("check"), h("p", {}, h("span", {}, text), h("small", {}, sub)));
    this.toast.classList.add("toast--ok", "is-shown");
    clearTimeout(this.toastTimer);
    this.toastTimer = window.setTimeout(() => this.toast.classList.remove("is-shown"), TOAST_MS);
  }
}
