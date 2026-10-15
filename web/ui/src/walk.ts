// First-person walk mode (PRD P8) in the overlay: the hint that replaces the HUD while walking, and
// the page's pointer lock. The engine owns the walker; this layer only sends `walk` commands.
import type { HexWorld } from "./bridge.ts";
import { h } from "./dom.ts";
import { icon } from "./icons.ts";

export class Walk {
  readonly root: HTMLElement;
  private readonly look: HTMLElement;
  private readonly hw: HexWorld;
  private readonly canvas: HTMLCanvasElement | null;
  /** Walk mode as this layer last asked for it, ahead of the engine's next frame. */
  private wanted = false;
  private engine = false;
  private locked = false;

  constructor(hw: HexWorld, canvas: HTMLCanvasElement | null) {
    this.hw = hw;
    this.canvas = canvas;
    this.look = h("span", { class: "walk-hint__look" }, "Click to look around");
    const leave = h("button", { class: "walk-hint__leave", type: "button" }, "Leave ", h("kbd", {}, "Esc"));
    leave.addEventListener("click", () => this.set(false));
    const hint = h(
      "div",
      { class: "walk-hint", role: "status" },
      icon("walk"),
      h(
        "p",
        {},
        this.look,
        h(
          "small",
          {},
          h("kbd", {}, "WASD"),
          " walk · ",
          h("kbd", {}, "Shift"),
          " sprint · ",
          h("kbd", {}, "Space"),
          " hop",
        ),
      ),
      leave,
    );
    this.root = h(
      "div",
      { class: "hud walk-ui is-off", "aria-hidden": "true" },
      h("i", { class: "walk-reticle" }),
      hint,
    );
    canvas?.addEventListener("click", () => {
      if (this.wanted && !this.locked) this.lock();
    });
    document.addEventListener("pointerlockchange", () => {
      const locked = !!this.canvas && document.pointerLockElement === this.canvas;
      if (locked === this.locked) return;
      this.locked = locked;
      this.hw.cmd({ cmd: "walk", on: null, pointer_lock: locked });
      this.look.textContent = locked ? "Mouse looks around" : "Click to look around";
      // The browser takes Esc to release the lock and may not pass the key on: losing it leaves walk mode.
      if (!locked && this.wanted) this.set(false);
    });
  }

  get on(): boolean {
    return this.wanted;
  }

  /** Enters or leaves walk mode. Called inside a click or key handler, so the pointer lock may be taken. */
  set(on: boolean): void {
    if (on === this.wanted) return;
    this.wanted = on;
    this.hw.cmd({ cmd: "walk", on, pointer_lock: null });
    if (on) this.lock();
    else if (this.locked) document.exitPointerLock();
  }

  private lock(): void {
    const c = this.canvas as (HTMLCanvasElement & { requestPointerLock(): Promise<void> | undefined }) | null;
    if (!c?.requestPointerLock) return;
    try {
      // A refusal (no gesture, a headless browser) is fine: dragging still looks around.
      c.requestPointerLock()?.catch(() => {});
    } catch {
      /* Older engines throw instead of rejecting. */
    }
  }

  /** Follows the engine's flag (a script, a load or the F key natively may change it). */
  render(engineOn: boolean): void {
    if (engineOn !== this.engine) {
      this.engine = engineOn;
      this.wanted = engineOn;
      if (!engineOn && this.locked) document.exitPointerLock();
    }
    this.root.classList.toggle("is-off", !engineOn);
    this.root.setAttribute("aria-hidden", String(!engineOn));
  }
}
