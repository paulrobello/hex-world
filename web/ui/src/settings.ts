// Settings (S9, design/ui/legend/settings.html): the Graphics tab (Quality, S9.1, Tilt-shift, S9.4,
// Colour grade and Grass) and the Audio tab (S9.5). The Controls tab is not built. Graphics reads
// `state().tier`, `state().tilt` and `state().grade` every frame while open, so the T key, scripts
// and a loaded save show here too, and writes through the `set_tier`, `tilt` and `set_grade`
// commands. Audio reads and writes `hexWorld.audio`, which keeps the levels per viewer in
// localStorage; the colour grade and the grass density are kept per viewer the same way, outside
// the world save. Grass reads `state().grass` and writes `set_grass`; it draws on High and Ultra only.
// The World tab (settings-world.ts) holds Start over.
import type { AudioApi } from "./audio.ts";
import type {
  GradeName,
  GradeState,
  GrassDensity,
  GrassState,
  HexWorld,
  QualityChoice,
  TierState,
  TiltState,
} from "./bridge.ts";
import { h } from "./dom.ts";
import { icon } from "./icons.ts";
import type { Rebuild } from "./loading.ts";
import type { Persistence } from "./persist.ts";
import {
  autoTier,
  CHANNELS,
  type Channel,
  GRADE_KEY,
  GRASS_KEY,
  gradeCommand,
  gradeView,
  grassCommand,
  grassView,
  percent,
  qualityView,
  roveIndex,
  selectedQuality,
  tierCommand,
} from "./settings-model.ts";
import { WorldPanel } from "./settings-world.ts";

/** The viewer's stored colour grade, or null without one or without storage. */
export function loadGrade(): string | null {
  try {
    return localStorage.getItem(GRADE_KEY);
  } catch {
    return null;
  }
}

function saveGrade(g: GradeName): void {
  try {
    localStorage.setItem(GRADE_KEY, g);
  } catch {
    /* Storage refused: the grade holds until the page closes. */
  }
}

/** The viewer's stored grass density, or null without one or without storage. */
export function loadGrass(): string | null {
  try {
    return localStorage.getItem(GRASS_KEY);
  } catch {
    return null;
  }
}

function saveGrass(d: GrassDensity): void {
  try {
    localStorage.setItem(GRASS_KEY, d);
  } catch {
    /* Storage refused: the density holds until the page closes. */
  }
}

type Tab = "graphics" | "audio" | "world";

/** A tier pick waits this long for `state().tier` to follow before the row shows the engine's answer again. */
const PICK_HOLD_MS = 3000;

interface Slider {
  input: HTMLInputElement;
  out: HTMLOutputElement;
}

export class Settings {
  readonly el: HTMLElement;
  private readonly hw: HexWorld;
  private readonly audio: AudioApi | null;
  private readonly changed: () => void;
  private readonly tabs = new Map<Tab, { btn: HTMLButtonElement; panel: HTMLElement }>();
  private tab: Tab = "graphics";
  private readonly tierList: HTMLUListElement;
  private readonly tierReason: HTMLElement;
  private readonly caution: HTMLElement;
  private readonly cautionTitle: HTMLElement;
  private readonly cautionBody: HTMLElement;
  private readonly gradeHead: HTMLElement;
  private readonly gradeList: HTMLUListElement;
  private readonly toggle: HTMLButtonElement;
  private readonly strength: HTMLInputElement;
  private readonly out: HTMLOutputElement;
  private readonly strengthField: HTMLElement;
  private readonly volumes = new Map<Channel, Slider>();
  private readonly mute: HTMLButtonElement;
  private readonly world = new WorldPanel();
  private shown = false;
  private tiltSig = "";
  private tierSig = "";
  private tier: TierState | undefined;
  private pick: { id: QualityChoice; until: number } | null = null;
  private gradeSig = "";
  private grade: GradeState | undefined;
  private gradePick: { id: GradeName; until: number } | null = null;
  private readonly grassList: HTMLUListElement;
  private readonly grassHint: HTMLElement;
  private grassSig = "";
  private grass: GrassState | undefined;
  private grassOn = false;
  private grassPick: { id: GrassDensity; until: number } | null = null;
  private readonly rebuild: Rebuild;

  constructor(hw: HexWorld, audio: AudioApi | null, onClose: () => void, onChange: () => void, rebuild: Rebuild) {
    this.hw = hw;
    this.audio = audio;
    this.changed = onChange;
    this.rebuild = rebuild;
    const close = h(
      "button",
      { class: "sheet__close", type: "button", "aria-label": "Close settings", "data-tip": "Close|Esc" },
      icon("close"),
    );
    close.addEventListener("click", onClose);

    // ---------- Graphics: Quality ----------
    this.tierList = h("ul", { class: "tiers", role: "radiogroup", "aria-label": "Quality tier" });
    this.tierList.addEventListener("click", (e) => {
      const li = (e.target as Element).closest<HTMLElement>(".tier");
      if (li) this.pickTier(li.dataset.tier as QualityChoice);
    });
    this.tierList.addEventListener("keydown", (e) =>
      this.radioKey(e, this.tierList, "tier", (id) => this.pickTier(id as QualityChoice)),
    );
    this.tierReason = h("p", { class: "tier-reason" });
    this.cautionTitle = h("b");
    this.cautionBody = h("span");
    this.caution = h(
      "div",
      { class: "note note--caution", role: "status", hidden: true },
      icon("alert"),
      h("div", {}, this.cautionTitle, this.cautionBody),
    );

    // ---------- Graphics: Tilt-shift ----------
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
      this.changed();
    });
    this.strength = h("input", { type: "range", min: 0, max: 100, step: 1, "aria-label": "Tilt-shift strength" });
    this.out = h("output", { class: "field__value" });
    this.strength.addEventListener("input", () => {
      paint(this.strength, this.out);
      this.hw.cmd({ cmd: "tilt", on: null, strength: Number(this.strength.value) / 100 });
    });
    this.strength.addEventListener("change", () => this.changed());
    this.strengthField = h(
      "label",
      { class: "field field--slider" },
      h("span", { class: "field__label" }, "Strength"),
      this.out,
      this.strength,
    );

    // ---------- Graphics: Colour grade ----------
    this.gradeHead = h("h2", { class: "section-h", hidden: true }, "Colour grade");
    this.gradeList = h("ul", { class: "tiers", role: "radiogroup", "aria-label": "Colour grade", hidden: true });
    this.gradeList.addEventListener("click", (e) => {
      const li = (e.target as Element).closest<HTMLElement>(".tier");
      if (li) this.pickGrade(li.dataset.grade as GradeName);
    });
    this.gradeList.addEventListener("keydown", (e) =>
      this.radioKey(e, this.gradeList, "grade", (id) => this.pickGrade(id as GradeName)),
    );

    // ---------- Graphics: Grass ----------
    this.grassList = h("ul", { class: "tiers", role: "radiogroup", "aria-label": "Grass" });
    this.grassList.addEventListener("click", (e) => {
      const li = (e.target as Element).closest<HTMLElement>(".tier");
      if (li) this.pickGrass(li.dataset.grass as GrassDensity);
    });
    this.grassList.addEventListener("keydown", (e) =>
      this.radioKey(e, this.grassList, "grass", (id) => this.pickGrass(id as GrassDensity)),
    );
    this.grassHint = h("p", { class: "tier-reason", hidden: true }, "High and Ultra only.");
    const graphics = h(
      "div",
      { class: "panel", role: "tabpanel" },
      h("h2", { class: "section-h" }, "Quality"),
      this.tierList,
      this.tierReason,
      this.caution,
      h("h2", { class: "section-h" }, "Tilt-shift"),
      h(
        "div",
        { class: "field" },
        h("span", { class: "field__label" }, "Miniature focus"),
        this.toggle,
        h("span", { class: "field__hint" }, "Blurs near and far like a diorama photo. Off by default."),
      ),
      this.strengthField,
      this.gradeHead,
      this.gradeList,
      h("h2", { class: "section-h" }, "Grass"),
      this.grassList,
      this.grassHint,
    );

    // ---------- Audio ----------
    const audioPanel = h(
      "div",
      { class: "panel", role: "tabpanel", hidden: true },
      h("h2", { class: "section-h" }, "Volume"),
    );
    for (const c of CHANNELS) {
      const input = h("input", { type: "range", min: 0, max: 100, step: 1, "aria-label": `${c.label} volume` });
      const out = h("output", { class: "field__value" });
      input.addEventListener("input", () => {
        paint(input, out);
        this.audio?.setVolume(Number(input.value) / 100, c.bus);
      });
      this.volumes.set(c.bus, { input, out });
      audioPanel.append(
        h(
          "label",
          { class: "field field--slider" },
          h("span", { class: "field__label" }, c.label),
          out,
          input,
          c.hint ? h("span", { class: "field__hint" }, c.hint) : null,
        ),
      );
    }
    this.mute = h("button", {
      class: "switch",
      type: "button",
      role: "switch",
      "aria-checked": "false",
      "aria-label": "Mute everything",
    });
    this.mute.addEventListener("click", () => {
      const on = this.mute.getAttribute("aria-checked") !== "true";
      this.audio?.setMuted(on);
      this.mute.setAttribute("aria-checked", String(on));
    });
    audioPanel.append(h("div", { class: "field" }, h("span", { class: "field__label" }, "Mute everything"), this.mute));

    // ---------- Tabs ----------
    const strip = h("div", { class: "tabs", role: "tablist", "aria-label": "Settings" });
    for (const [id, label, panel] of [
      ["graphics", "Graphics", graphics],
      ["audio", "Audio", audioPanel],
      ["world", "World", this.world.el],
    ] as const) {
      const btn = h("button", {
        type: "button",
        role: "tab",
        id: `settings-tab-${id}`,
        "aria-controls": `settings-${id}`,
      });
      btn.textContent = label;
      panel.id = `settings-${id}`;
      panel.setAttribute("aria-labelledby", btn.id);
      btn.addEventListener("click", () => this.select(id));
      strip.append(btn);
      this.tabs.set(id, { btn, panel });
    }
    strip.addEventListener("keydown", (e) => {
      const ids = [...this.tabs.keys()];
      const n = roveIndex(ids.indexOf(this.tab), ids.length, e.key, "x");
      if (n == null) return;
      e.preventDefault();
      this.select(ids[n]!);
      this.tabs.get(ids[n]!)!.btn.focus();
    });
    this.select("graphics");

    this.el = h(
      "section",
      { class: "sheet sheet--wide sheet--settings is-closed", "data-region": "settings", "aria-label": "Settings" },
      h("header", { class: "sheet__head" }, h("h1", {}, "Settings"), close),
      strip,
      h("div", { class: "sheet__body" }, graphics, audioPanel, this.world.el),
    );
    this.el.toggleAttribute("inert", true);
    // Tab walks this sheet's own controls (the Hud leaves Tab to it while focus is inside), wrapping.
    this.el.addEventListener("keydown", (e) => {
      if (e.key !== "Tab" || e.altKey || e.ctrlKey || e.metaKey) return;
      const all = [...this.el.querySelectorAll<HTMLElement>("button, input, [tabindex='0']")].filter(
        (x) => !x.closest("[hidden]") && !(x as HTMLInputElement).disabled,
      );
      const i = all.indexOf(document.activeElement as HTMLElement);
      const edge = e.shiftKey ? i <= 0 : i === all.length - 1;
      if (!edge) return;
      e.preventDefault();
      all[e.shiftKey ? all.length - 1 : 0]?.focus();
    });
  }

  get open(): boolean {
    return this.shown;
  }

  set open(v: boolean) {
    this.shown = v;
    this.el.classList.toggle("is-closed", !v);
    this.el.toggleAttribute("inert", !v);
    this.world.reset();
    if (!v) return;
    this.tiltSig = "";
    this.tierSig = "";
    this.gradeSig = "";
    this.grassSig = "";
    this.syncAudio();
    this.tabs.get(this.tab)!.btn.focus({ preventScroll: true });
  }

  /** Whether focus is inside the sheet (the Hud then leaves Tab, Enter, Space and arrows to it). */
  holds(t: EventTarget | null): boolean {
    return this.shown && t instanceof Node && this.el.contains(t);
  }

  /** The World tab's Start over acts on the current world once persistence is up. */
  attachWorld(p: Persistence): void {
    this.world.attach(p);
  }

  /** Esc: an open Start over confirm closes first. Returns whether it used the key. */
  escape(): boolean {
    return this.shown && this.world.escape();
  }

  render(tilt: TiltState | undefined, tier: TierState | undefined, grade?: GradeState, grass?: GrassState): void {
    if (!this.shown) return;
    if (this.tab === "world") this.world.render();
    this.renderTier(tier);
    this.renderGrade(grade);
    this.renderGrass(grass, tier);
    if (!tilt) return;
    const sig = `${tilt.enabled}|${tilt.strength}`;
    if (sig === this.tiltSig) return;
    this.tiltSig = sig;
    this.toggle.setAttribute("aria-checked", String(tilt.enabled));
    if (document.activeElement !== this.strength) this.strength.value = String(Math.round(tilt.strength * 100));
    this.strength.disabled = !tilt.enabled;
    this.strengthField.classList.toggle("is-disabled", !tilt.enabled);
    paint(this.strength, this.out);
  }

  private select(id: Tab): void {
    this.tab = id;
    for (const [k, { btn, panel }] of this.tabs) {
      btn.setAttribute("aria-selected", String(k === id));
      btn.tabIndex = k === id ? 0 : -1;
      panel.hidden = k !== id;
    }
    if (id === "audio") this.syncAudio();
    if (id === "world") this.world.render(true);
    else this.world.reset();
  }

  private syncAudio(): void {
    const a = this.audio?.state();
    if (!a) return;
    for (const [bus, s] of this.volumes) {
      s.input.value = String(percent(a.volumes[bus]));
      paint(s.input, s.out);
    }
    this.mute.setAttribute("aria-checked", String(a.muted));
  }

  private renderTier(tier: TierState | undefined): void {
    this.tier = tier;
    const pick = this.pick;
    if (pick && (selectedQuality(tier) === pick.id || performance.now() > pick.until)) this.pick = null;
    const v = qualityView(tier);
    const selected = this.pick?.id ?? v.selected;
    const sig = JSON.stringify([v, selected]);
    if (sig === this.tierSig) return;
    this.tierSig = sig;
    const focused = (document.activeElement as HTMLElement | null)?.dataset?.tier;
    this.tierList.replaceChildren(
      ...v.rows.map((r) => {
        const on = r.id === selected;
        const li = h(
          "li",
          { class: "tier", role: "radio", tabindex: on ? 0 : -1, "aria-checked": String(on), "data-tier": r.id },
          h("span", { class: "radio", "aria-hidden": "true" }),
          h("b", {}, r.name),
          r.tag ? h("span", { class: r.detected ? "tag tag--detected" : "tag" }, r.tag) : null,
          h("small", {}, r.detail),
        );
        return li;
      }),
    );
    if (focused) this.tierList.querySelector<HTMLElement>(`[data-tier="${focused}"]`)?.focus({ preventScroll: true });
    this.tierReason.textContent = v.reason ?? "";
    this.tierReason.hidden = !v.reason;
    this.caution.hidden = !v.caution;
    this.cautionTitle.textContent = v.caution?.title ?? "";
    this.cautionBody.textContent = v.caution?.body ?? "";
  }

  /** The quality-change layer goes up first and `set_tier` follows once it is painted (`Rebuild`). */
  private pickTier(id: QualityChoice): void {
    if (this.rebuild.active) return;
    const now = this.pick?.id ?? selectedQuality(this.tier);
    if (id === now) return;
    this.pick = { id, until: performance.now() + PICK_HOLD_MS };
    this.rebuild.begin(
      id === "auto" ? autoTier(this.tier) : id,
      () => this.hw.state().tier?.requests ?? 0,
      () => {
        this.hw.cmd(tierCommand(id));
        this.changed();
      },
    );
    this.renderTier(this.tier);
  }

  private renderGrade(grade: GradeState | undefined): void {
    this.grade = grade;
    const pick = this.gradePick;
    if (pick && (grade?.name === pick.id || performance.now() > pick.until)) this.gradePick = null;
    const v = gradeView(grade, this.gradePick?.id ?? null);
    const sig = JSON.stringify(v);
    if (sig === this.gradeSig) return;
    this.gradeSig = sig;
    const focused = (document.activeElement as HTMLElement | null)?.dataset?.grade;
    this.gradeList.replaceChildren(
      ...v.rows.map((r) => {
        const on = r.id === v.selected;
        return h(
          "li",
          { class: "tier", role: "radio", tabindex: on ? 0 : -1, "aria-checked": String(on), "data-grade": r.id },
          h("span", { class: "radio", "aria-hidden": "true" }),
          h("b", {}, r.name),
          r.tag ? h("span", { class: "tag" }, r.tag) : null,
          h("small", {}, r.detail),
        );
      }),
    );
    this.gradeHead.hidden = this.gradeList.hidden = v.rows.length === 0;
    if (focused) this.gradeList.querySelector<HTMLElement>(`[data-grade="${focused}"]`)?.focus({ preventScroll: true });
  }

  /** A grade pick switches the look live and is kept for this viewer; it never queues an autosave. */
  private pickGrade(id: GradeName): void {
    const now = this.gradePick?.id ?? this.grade?.name;
    if (id === now) return;
    this.gradePick = { id, until: performance.now() + PICK_HOLD_MS };
    this.hw.cmd(gradeCommand(id));
    saveGrade(id);
    this.renderGrade(this.grade);
  }

  private renderGrass(grass: GrassState | undefined, tier: TierState | undefined): void {
    this.grass = grass;
    const pick = this.grassPick;
    if (pick && (grass?.density === pick.id || performance.now() > pick.until)) this.grassPick = null;
    const v = grassView(grass, tier, this.grassPick?.id ?? null);
    this.grassOn = v.enabled;
    const sig = JSON.stringify(v);
    if (sig === this.grassSig) return;
    this.grassSig = sig;
    const focused = (document.activeElement as HTMLElement | null)?.dataset?.grass;
    this.grassList.replaceChildren(
      ...v.rows.map((r) => {
        const on = r.id === v.selected;
        return h(
          "li",
          {
            class: "tier",
            role: "radio",
            tabindex: on && v.enabled ? 0 : -1,
            "aria-checked": String(on),
            "aria-disabled": String(!v.enabled),
            "data-grass": r.id,
          },
          h("span", { class: "radio", "aria-hidden": "true" }),
          h("b", {}, r.name),
          h("small", {}, r.detail),
        );
      }),
    );
    this.grassHint.hidden = v.enabled;
    if (focused) this.grassList.querySelector<HTMLElement>(`[data-grass="${focused}"]`)?.focus({ preventScroll: true });
  }

  /** A density pick swaps the blades live and is kept for this viewer; it never queues an autosave. */
  private pickGrass(id: GrassDensity): void {
    if (!this.grassOn) return;
    const now = this.grassPick?.id ?? this.grass?.density;
    if (id === now) return;
    this.grassPick = { id, until: performance.now() + PICK_HOLD_MS };
    this.hw.cmd(grassCommand(id));
    saveGrass(id);
    this.renderGrass(this.grass, this.tier);
  }

  /** Space and Enter pick the focused row; arrows, Home and End move along the radio list and pick. */
  private radioKey(
    e: KeyboardEvent,
    list: HTMLElement,
    attr: "tier" | "grade" | "grass",
    pick: (id: string) => void,
  ): void {
    const rows = [...list.querySelectorAll<HTMLElement>(".tier")];
    const i = rows.indexOf(document.activeElement as HTMLElement);
    if (i < 0) return;
    if (e.key === " " || e.key === "Enter") {
      e.preventDefault();
      pick(rows[i]!.dataset[attr]!);
      return;
    }
    const n = roveIndex(i, rows.length, e.key, "y");
    if (n == null) return;
    e.preventDefault();
    const id = rows[n]!.dataset[attr]!;
    rows[n]!.focus();
    pick(id);
    list.querySelector<HTMLElement>(`[data-${attr}="${id}"]`)?.focus();
  }
}

/** The slider's filled track and its percent readout. */
function paint(input: HTMLInputElement, out: HTMLOutputElement): void {
  input.style.setProperty("--pct", `${input.value}%`);
  out.textContent = `${input.value}%`;
}
