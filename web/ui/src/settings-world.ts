// The Settings sheet's World tab. Walk: the walk speed slider (owner, 2026-10-14), live through
// `set_walk_speed`, following `state().walk` and kept per viewer in localStorage. Start over clears
// the world on screen back to the start tile in place (Persistence.startOver: same name, same slot,
// no undo history), after an inline confirm in the Worlds sheet's delete style. Esc or Cancel closes
// the confirm and focus returns to the button.
import type { WalkState } from "./bridge.ts";
import { h } from "./dom.ts";
import { icon } from "./icons.ts";
import type { Persistence } from "./persist.ts";
import {
  clampWalkSpeed,
  SPRINT_FACTOR,
  speedLabel,
  storedWalkSpeed,
  WALK_SPEED_DEFAULT,
  WALK_SPEED_KEY,
  WALK_SPEED_MAX,
  WALK_SPEED_MIN,
  WALK_SPEED_STEP,
  walkSpeedCommand,
  walkSpeedPct,
} from "./walk-speed.ts";
import { plural } from "./words.ts";

/** The viewer's stored walk speed, or null without one or without storage. */
export function loadWalkSpeed(): number | null {
  try {
    return storedWalkSpeed(localStorage.getItem(WALK_SPEED_KEY));
  } catch {
    return null;
  }
}

function saveWalkSpeed(v: number): void {
  try {
    localStorage.setItem(WALK_SPEED_KEY, String(v));
  } catch {
    /* Storage refused: the speed holds until the page closes. */
  }
}

export class WorldPanel {
  readonly el: HTMLElement;
  private p: Persistence | null = null;
  private readonly name: HTMLElement;
  private readonly hint: HTMLElement;
  private readonly trigger: HTMLButtonElement;
  private readonly confirm: HTMLElement;
  private readonly confirmText: Text;
  private readonly cancel: HTMLButtonElement;
  private readonly speed: HTMLInputElement;
  private readonly speedOut: HTMLOutputElement;
  private readonly sprintHint: HTMLElement;
  private sig = "";
  private walkSig = "";

  /** `send` takes the `set_walk_speed` command for the engine. */
  constructor(send: (c: ReturnType<typeof walkSpeedCommand>) => void) {
    this.speed = h("input", {
      type: "range",
      min: WALK_SPEED_MIN,
      max: WALK_SPEED_MAX,
      step: WALK_SPEED_STEP,
      value: WALK_SPEED_DEFAULT,
      "aria-label": "Walk speed",
    });
    this.speedOut = h("output", { class: "field__value" });
    this.sprintHint = h("span", { class: "field__hint" });
    this.speed.addEventListener("input", () => {
      const v = clampWalkSpeed(Number(this.speed.value));
      this.paintSpeed(v, v * SPRINT_FACTOR);
      send(walkSpeedCommand(v));
      saveWalkSpeed(v);
    });
    this.paintSpeed(WALK_SPEED_DEFAULT, WALK_SPEED_DEFAULT * SPRINT_FACTOR);
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
      h("h2", { class: "section-h" }, "Walk"),
      h(
        "label",
        { class: "field field--slider" },
        h("span", { class: "field__label" }, "Walk speed"),
        this.speedOut,
        this.speed,
        this.sprintHint,
      ),
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

  /** Follows the engine's walk speed (a script or the startup value may set it), except while the
   * slider has focus. Cheap to call every frame. */
  renderWalk(walk: WalkState | undefined): void {
    if (!walk || document.activeElement === this.speed) return;
    const sig = `${walk.walk_speed}|${walk.sprint_speed}`;
    if (sig === this.walkSig) return;
    this.walkSig = sig;
    this.speed.value = String(walk.walk_speed);
    this.paintSpeed(walk.walk_speed, walk.sprint_speed);
  }

  private paintSpeed(walk: number, sprint: number): void {
    this.speed.style.setProperty("--pct", `${walkSpeedPct(walk)}%`);
    this.speedOut.textContent = speedLabel(walk);
    this.sprintHint.textContent = `Shift sprints at twice this: ${speedLabel(sprint)}. Applies at once.`;
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
