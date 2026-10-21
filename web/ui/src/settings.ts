// Settings (S9, design/ui/legend/settings.html): the Graphics tab (Quality, S9.1, Tilt-shift, S9.4,
// Colour grade and Grass), the Audio tab (S9.5) and the Controls tab (S9.6: Reduce motion, Follow
// system or On, kept per viewer and sent as `set_reduced_motion` with the OS setting). Graphics reads
// `state().tier`, `state().tilt` and `state().grade` every frame while open, so the T key, scripts
// and a loaded save show here too, and writes through the `set_tier`, `tilt` and `set_grade`
// commands. Audio reads and writes `hexWorld.audio`, which keeps the levels per viewer in
// localStorage; the colour grade and the grass density are kept per viewer the same way, outside
// the world save. The Grass density slider reads `state().grass` and writes `set_grass` (at most every
// 150 ms while dragging, and on release); the Grass distance slider (Near, Medium, Far, Very far)
// reads `state().grass.distance` and writes `set_grass_distance`; grass draws on High and Ultra only.
// The World tab (settings-world.ts) holds the walk speed and Start over.
import type { AudioApi } from "./audio.ts";
import { DebugPanel } from "./debug.ts";
import type {
  DebugState,
  GradeName,
  GradeState,
  GrassDistanceName,
  GrassState,
  HexWorld,
  QualityChoice,
  TierState,
  TiltState,
  WalkState,
} from "./bridge.ts";
import { h } from "./dom.ts";
import { icon } from "./icons.ts";
import type { Rebuild } from "./loading.ts";
import type { Persistence } from "./persist.ts";
import { seg } from "./photo.ts";
import { Select } from "./select.ts";
import {
  autoTier,
  CHANNELS,
  type Channel,
  GRADE_KEY,
  GRASS_DISTANCE_KEY,
  GRASS_DISTANCE_STOPS,
  GRASS_KEY,
  GRASS_SLIDER,
  MOTION_KEY,
  type MotionChoice,
  gradeCommand,
  gradeView,
  grassCommand,
  grassDistanceCommand,
  grassDistanceView,
  grassView,
  motionCommand,
  storedGrass,
  storedMotion,
  systemMotionLabel,
  percent,
  qualityView,
  roveIndex,
  selectedQuality,
  throttle,
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

/**
 * The viewer's stored grass density, or null without one or without storage. A stepped choice from
 * before the slider (`off`, `sparse`, `normal`, `lush`) is rewritten as its percent (0, 50, 100, 150).
 */
export function loadGrass(): string | null {
  try {
    const stored = localStorage.getItem(GRASS_KEY);
    const pct = storedGrass(stored);
    if (pct != null && stored !== String(pct)) {
      localStorage.setItem(GRASS_KEY, String(pct));
      return String(pct);
    }
    return stored;
  } catch {
    return null;
  }
}

/** The viewer's stored grass distance stop, or null without one or without storage. */
export function loadGrassDistance(): string | null {
  try {
    return localStorage.getItem(GRASS_DISTANCE_KEY);
  } catch {
    return null;
  }
}

function saveGrassDistance(id: GrassDistanceName): void {
  try {
    localStorage.setItem(GRASS_DISTANCE_KEY, id);
  } catch {
    /* Storage refused: the distance holds until the page closes. */
  }
}

function saveGrass(pct: number): void {
  try {
    localStorage.setItem(GRASS_KEY, String(pct));
  } catch {
    /* Storage refused: the density holds until the page closes. */
  }
}

/** This page's latest Reduce motion pick, so the OS listener keeps it when storage is refused. */
let motionPick: MotionChoice | null = null;

/** The viewer's Reduce motion choice; Follow system without one or without storage. */
export function loadMotion(): MotionChoice {
  if (motionPick) return motionPick;
  try {
    return storedMotion(localStorage.getItem(MOTION_KEY));
  } catch {
    return "system";
  }
}

function saveMotion(c: MotionChoice): void {
  motionPick = c;
  try {
    if (c === "system") localStorage.removeItem(MOTION_KEY);
    else localStorage.setItem(MOTION_KEY, c);
  } catch {
    /* Storage refused: the choice holds until the page closes. */
  }
}

/** The OS reduced-motion query, shared by the start-up sender (main.ts) and the Controls tab. */
export const OS_REDUCE = "(prefers-reduced-motion: reduce)";

type Tab = "graphics" | "audio" | "controls" | "world" | "debug";

/** A tier pick waits this long for `state().tier` to follow before the row shows the engine's answer again. */
const PICK_HOLD_MS = 3000;
/** A grass slider drag sends `set_grass` at most this often, so tile meshes never rebuild every frame. */
const GRASS_SEND_MS = 150;

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
  private readonly gradeSelect: Select;
  private readonly toggle: HTMLButtonElement;
  private readonly strength: HTMLInputElement;
  private readonly out: HTMLOutputElement;
  private readonly strengthField: HTMLElement;
  private readonly volumes = new Map<Channel, Slider>();
  private readonly mute: HTMLButtonElement;
  private readonly motion: ReturnType<typeof seg<MotionChoice>>;
  private motionChoice: MotionChoice = loadMotion();
  private readonly world = new WorldPanel((c) => this.hw.cmd(c));
  private readonly debug: DebugPanel;
  private shown = false;
  private tiltSig = "";
  private tierSig = "";
  private tier: TierState | undefined;
  private pick: { id: QualityChoice; until: number } | null = null;
  private gradeSig = "";
  private grade: GradeState | undefined;
  private gradePick: { id: GradeName; until: number } | null = null;
  private readonly grassInput: HTMLInputElement;
  private readonly grassOut: HTMLOutputElement;
  private readonly grassField: HTMLElement;
  private readonly grassHint: HTMLElement;
  private readonly grassSend: { push: (pct: number) => void; flush: () => void };
  private grassSig = "";
  private grassOn = false;
  private grassPick: { pct: number; until: number } | null = null;
  private readonly distInput: HTMLInputElement;
  private readonly distOut: HTMLOutputElement;
  private readonly distField: HTMLElement;
  private distSig = "";
  private distPick: { scale: number; until: number } | null = null;
  private readonly rebuild: Rebuild;

  constructor(hw: HexWorld, audio: AudioApi | null, onClose: () => void, onChange: () => void, rebuild: Rebuild) {
    this.hw = hw;
    this.audio = audio;
    this.changed = onChange;
    this.rebuild = rebuild;
    this.debug = new DebugPanel(hw);
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
    this.tierList.addEventListener("keydown", (e) => this.radioKey(e, (id) => this.pickTier(id as QualityChoice)));
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
    // One compact select (owner 2026-10-14): the current look closed, every look with its
    // description open.
    this.gradeHead = h("h2", { class: "section-h", id: "grade-h", hidden: true }, "Colour grade");
    this.gradeSelect = new Select("grade", "grade-h", (id) => this.pickGrade(id as GradeName));
    this.gradeSelect.el.hidden = true;

    // ---------- Graphics: Grass ----------
    this.grassInput = h("input", {
      type: "range",
      min: GRASS_SLIDER.min,
      max: GRASS_SLIDER.max,
      step: GRASS_SLIDER.step,
      "aria-label": "Grass density",
    });
    this.grassOut = h("output", { class: "field__value" });
    this.grassSend = throttle((pct) => this.hw.cmd(grassCommand(pct)), GRASS_SEND_MS);
    // A drag swaps the blades live, throttled; the release sends the last value and keeps it for this
    // viewer. It never queues an autosave.
    this.grassInput.addEventListener("input", () => {
      if (!this.grassOn) return;
      const pct = Number(this.grassInput.value);
      paint(this.grassInput, this.grassOut);
      this.grassPick = { pct, until: performance.now() + PICK_HOLD_MS };
      this.grassSend.push(pct);
    });
    this.grassInput.addEventListener("change", () => {
      if (!this.grassOn) return;
      const pct = Number(this.grassInput.value);
      this.grassSend.push(pct);
      this.grassSend.flush();
      saveGrass(pct);
    });
    this.grassField = h(
      "label",
      { class: "field field--slider" },
      h("span", { class: "field__label" }, "Grass density"),
      this.grassOut,
      this.grassInput,
      h("span", { class: "field__hint" }, "100% is the meadow as tuned. 0% turns grass off; 150% costs the most."),
    );
    // Four stops on one slider, its readout the stop's name. Each move sends at once: a stop only moves
    // the shader's fade and the build radius, and new tiles build under the engine's per-frame budget.
    this.distInput = h("input", {
      type: "range",
      min: 0,
      max: GRASS_DISTANCE_STOPS.length - 1,
      step: 1,
      "aria-label": "Grass distance",
    });
    this.distOut = h("output", { class: "field__value" });
    this.distInput.addEventListener("input", () => {
      if (!this.grassOn) return;
      const stop = GRASS_DISTANCE_STOPS[Number(this.distInput.value)]!;
      paintStop(this.distInput, this.distOut);
      this.distPick = { scale: stop.scale, until: performance.now() + PICK_HOLD_MS };
      this.hw.cmd(grassDistanceCommand(stop.id));
    });
    this.distInput.addEventListener("change", () => {
      if (!this.grassOn) return;
      saveGrassDistance(GRASS_DISTANCE_STOPS[Number(this.distInput.value)]!.id);
    });
    this.distField = h(
      "label",
      { class: "field field--slider" },
      h("span", { class: "field__label" }, "Grass distance"),
      this.distOut,
      this.distInput,
      h(
        "span",
        { class: "field__hint" },
        "How far from the camera blades still draw. Near is the meadow as tuned; Very far costs the most.",
      ),
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
      this.gradeSelect.el,
      h("h2", { class: "section-h" }, "Grass"),
      this.grassField,
      this.distField,
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

    // ---------- Controls ----------
    // Reduce motion (S9.6): Follow system or On. Each pick is kept for this viewer and sent at once with
    // the OS setting; the HUD's is-reduced-motion follows the engine flag (dom.ts syncReducedMotion).
    const os = matchMedia(OS_REDUCE);
    this.motion = seg<MotionChoice>(
      "Reduce motion",
      [
        { value: "system", text: "Follow system", sub: systemMotionLabel(os.matches) },
        { value: "on", text: "On" },
      ],
      (c) => {
        this.motionChoice = c;
        saveMotion(c);
        this.hw.cmd(motionCommand(c, os.matches));
      },
    );
    this.motion.set(this.motionChoice);
    os.addEventListener("change", () => {
      const sub = this.motion.subs.get("system");
      if (sub) sub.textContent = systemMotionLabel(os.matches);
    });
    const controls = h(
      "div",
      { class: "panel", role: "tabpanel", hidden: true },
      h("h2", { class: "section-h" }, "Accessibility"),
      h(
        "div",
        { class: "field field--force" },
        h("span", { class: "field__label" }, "Reduce motion"),
        this.motion.el,
        h(
          "span",
          { class: "field__hint" },
          "On turns slides, the tile rise and its dust into instant changes. While your system asks for reduced motion, it stays on.",
        ),
      ),
    );

    // ---------- Tabs ----------
    const strip = h("div", { class: "tabs", role: "tablist", "aria-label": "Settings" });
    for (const [id, label, panel] of [
      ["graphics", "Graphics", graphics],
      ["audio", "Audio", audioPanel],
      ["controls", "Controls", controls],
      ["world", "World", this.world.el],
      ["debug", "Debug", this.debug.el],
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
      h("div", { class: "sheet__body" }, graphics, audioPanel, controls, this.world.el, this.debug.el),
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
    this.gradeSelect.close(false);
    this.el.classList.toggle("is-closed", !v);
    this.el.toggleAttribute("inert", !v);
    this.world.reset();
    if (!v) return;
    this.tiltSig = "";
    this.tierSig = "";
    this.gradeSig = "";
    this.grassSig = "";
    this.distSig = "";
    this.syncAudio();
    this.motion.set(this.motionChoice);
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

  /** Esc: an open colour grade list or Start over confirm closes first. Returns whether it used the key. */
  escape(): boolean {
    return this.shown && (this.gradeSelect.close() || this.world.escape());
  }

  render(
    tilt: TiltState | undefined,
    tier: TierState | undefined,
    grade?: GradeState,
    grass?: GrassState,
    walk?: WalkState,
    debug?: DebugState,
  ): void {
    if (!this.shown) return;
    if (this.tab === "debug") this.debug.render(debug);
    if (this.tab === "world") {
      this.world.render();
      this.world.renderWalk(walk);
    }
    this.renderTier(tier);
    this.renderGrade(grade);
    this.renderGrass(grass, tier);
    this.renderGrassDistance(grass, tier);
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
    this.gradeSelect.close(false);
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
    this.gradeSelect.update(v.rows, v.selected);
    this.gradeHead.hidden = this.gradeSelect.el.hidden = v.rows.length === 0;
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
    const pick = this.grassPick;
    if (pick && (grassView(grass, tier).value === pick.pct || performance.now() > pick.until)) this.grassPick = null;
    const v = grassView(grass, tier, this.grassPick?.pct ?? null);
    this.grassOn = v.enabled;
    const sig = JSON.stringify(v);
    if (sig === this.grassSig) return;
    this.grassSig = sig;
    if (document.activeElement !== this.grassInput || !v.enabled)
      this.grassInput.value = String(v.value ?? GRASS_SLIDER.normal);
    this.grassInput.disabled = !v.enabled;
    this.grassField.classList.toggle("is-disabled", !v.enabled);
    this.grassHint.hidden = v.enabled;
    paint(this.grassInput, this.grassOut);
  }

  private renderGrassDistance(grass: GrassState | undefined, tier: TierState | undefined): void {
    const pick = this.distPick;
    if (pick && (grass?.distance === pick.scale || performance.now() > pick.until)) this.distPick = null;
    const v = grassDistanceView(grass, tier, this.distPick?.scale ?? null);
    const sig = JSON.stringify(v);
    if (sig === this.distSig) return;
    this.distSig = sig;
    if (document.activeElement !== this.distInput || !v.enabled) this.distInput.value = String(v.index);
    this.distInput.disabled = !v.enabled;
    this.distField.classList.toggle("is-disabled", !v.enabled);
    paintStop(this.distInput, this.distOut);
  }

  /** Space and Enter pick the focused quality row; arrows, Home and End move along the radio list and pick. */
  private radioKey(e: KeyboardEvent, pick: (id: string) => void): void {
    const rows = [...this.tierList.querySelectorAll<HTMLElement>(".tier")];
    const i = rows.indexOf(document.activeElement as HTMLElement);
    if (i < 0) return;
    if (e.key === " " || e.key === "Enter") {
      e.preventDefault();
      pick(rows[i]!.dataset.tier!);
      return;
    }
    const n = roveIndex(i, rows.length, e.key, "y");
    if (n == null) return;
    e.preventDefault();
    const id = rows[n]!.dataset.tier!;
    rows[n]!.focus();
    pick(id);
    this.tierList.querySelector<HTMLElement>(`[data-tier="${id}"]`)?.focus();
  }
}

/** The slider's filled track (its share of the range) and its percent readout. */
function paint(input: HTMLInputElement, out: HTMLOutputElement): void {
  const min = Number(input.min || 0);
  const max = Number(input.max || 100);
  input.style.setProperty("--pct", `${((Number(input.value) - min) / (max - min)) * 100}%`);
  out.textContent = `${input.value}%`;
}

/** The grass distance slider's filled track and its stop name, also read out by screen readers. */
function paintStop(input: HTMLInputElement, out: HTMLOutputElement): void {
  const max = Number(input.max);
  const i = Number(input.value);
  input.style.setProperty("--pct", `${(i / max) * 100}%`);
  const name = GRASS_DISTANCE_STOPS[i]?.name ?? "";
  out.textContent = name;
  input.setAttribute("aria-valuetext", name);
}
