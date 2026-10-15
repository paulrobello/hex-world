// Photo mode's click to focus (owner 2026-10-14): a left press and release on the scene that barely
// moves and ends quickly is a click, and Tilt-shift and Manual focus on the surface under it. Any
// longer or further press is the drag that looks around, and leaves the focus alone. The engine
// finds the surface (`photo` command `focus_at`); this module only tells clicks from drags.

/** The most the pointer may travel during a press, in CSS pixels, for it to stay a click. */
export const CLICK_MAX_PX = 4;
/** The longest press, in milliseconds, that is still a click. */
export const CLICK_MAX_MS = 350;
/** The focus slider's range in metres: the engine's whole `FOCUS_M` (photo.rs), since a click on
 * the near ground or the far edge of the map can land anywhere in it. */
export const FOCUS_MIN = 1;
export const FOCUS_MAX = 1000;

export interface PointerSample {
  x: number;
  y: number;
  /** Milliseconds, any fixed origin (`PointerEvent.timeStamp`). */
  t: number;
  /** `PointerEvent.button`: 0 is the primary (left) button. */
  button: number;
}

/** Tells a click from a drag across one press. The travel is measured over the whole press, so a
 * drag that comes back to where it started is still a drag. */
export class ClickDetector {
  private down: { x: number; y: number; t: number } | null = null;
  private far = false;

  press(p: PointerSample): void {
    this.down = p.button === 0 ? { x: p.x, y: p.y, t: p.t } : null;
    this.far = false;
  }

  move(p: Pick<PointerSample, "x" | "y">): void {
    if (this.down && Math.hypot(p.x - this.down.x, p.y - this.down.y) > CLICK_MAX_PX) this.far = true;
  }

  /** True when the press that ends here was a click. */
  release(p: PointerSample): boolean {
    const d = this.down;
    if (!d || p.button !== 0) {
      this.down = null;
      return false;
    }
    this.move(p);
    this.down = null;
    return !this.far && p.t - d.t <= CLICK_MAX_MS;
  }

  cancel(): void {
    this.down = null;
  }
}

/** A client point as `[x, y]` across `rect` from its top left (the `focus_at` value), or null
 * outside it. */
export function focusAt(
  x: number,
  y: number,
  rect: { left: number; top: number; width: number; height: number },
): [number, number] | null {
  if (rect.width <= 0 || rect.height <= 0) return null;
  const u = (x - rect.left) / rect.width;
  const v = (y - rect.top) / rect.height;
  return u >= 0 && u <= 1 && v >= 0 && v <= 1 ? [u, v] : null;
}

/** The slider position for an engine focus distance: whole metres within the slider's range. */
export const focusSliderValue = (m: number): number => Math.round(Math.min(FOCUS_MAX, Math.max(FOCUS_MIN, m)));
