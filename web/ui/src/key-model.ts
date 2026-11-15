// What a game key does once the HUD has decided the key is its own (hud.ts reads the DOM and the
// sub-panels to decide that, then acts on this). Free of the DOM so Node tests read it directly,
// the Escape precedence chain included.
import type { PlayState } from "./bridge.ts";

/** What the HUD has open or hidden, which decides where Escape goes. */
export interface KeyFlags {
  worldsOpen: boolean;
  settingsOpen: boolean;
  hidden: boolean;
}

/** Who a key belongs to. `leave` is a key the HUD does not own: nothing is prevented or acted on. */
export type KeyAction =
  | { kind: "leave" }
  /** The undo or redo shortcut (Ctrl or Cmd with Z); `redo` with Shift. */
  | { kind: "history"; redo: boolean }
  /** A modifier chord that is not undo: swallowed by nothing, acted on by nothing. */
  | { kind: "ignore" }
  | { kind: "cycle_focus"; back: boolean }
  /** `repeatThrottled`: a held key inside the repeat window rotates once, not every repeat. */
  | { kind: "rotate"; back: boolean; repeat: boolean }
  | { kind: "toggle_bulldoze"; repeat: boolean }
  | { kind: "toggle_photo"; repeat: boolean }
  | { kind: "toggle_hidden"; repeat: boolean }
  | { kind: "enter_walk"; repeat: boolean }
  | { kind: "escape"; step: EscapeStep }
  /** Enter on the slot: places the ghost. `over` is whether Enter is on a row or a button, which keep it. */
  | { kind: "place_ghost"; at: { q: number; r: number } | null };

/** The one thing a press of Escape does, in the order the HUD offers them. */
export type EscapeStep = "unpin" | "close_worlds" | "close_settings" | "leave_bulldoze" | "show_hud" | "clear_slot";

/** The key as the HUD normalises it: a single character lower-cased, a named key as it is. */
export const normalKey = (e: Pick<KeyboardEvent, "key">): string => (e.key.length === 1 ? e.key.toLowerCase() : e.key);

/**
 * Where Escape goes, first match wins: a pinned slot (this press only unpins), the Worlds sheet,
 * Settings, bulldoze mode, a hidden HUD, then the Tiles sheet's selected slot. The Worlds and
 * Settings steps may be absorbed by a panel's own Escape first (its confirm or its list): the HUD
 * asks the panel, and closes it only when the panel did not take the press.
 */
export function escapeStep(st: Pick<PlayState, "pinned" | "tool">, flags: KeyFlags): EscapeStep {
  if (st.pinned) return "unpin";
  if (flags.worldsOpen) return "close_worlds";
  if (flags.settingsOpen) return "close_settings";
  if (st.tool === "bulldoze") return "leave_bulldoze";
  if (flags.hidden) return "show_hud";
  return "clear_slot";
}

/**
 * The action for key press `e` once the HUD's guards have passed (rebuild, typing, Settings and
 * Worlds controls, photo and walk mode, and the Tiles sheet's own keys). `modifier` is Ctrl or
 * Cmd; `onRowOrButton` is whether the key landed on a tile row or a button, which keep Enter.
 */
export function keyAction(
  st: Pick<PlayState, "pinned" | "tool" | "ghost">,
  flags: KeyFlags,
  e: Pick<KeyboardEvent, "key" | "code" | "shiftKey" | "repeat">,
  modifier: boolean,
  onRowOrButton: boolean,
): KeyAction {
  const k = normalKey(e);
  if (modifier) return k === "z" ? { kind: "history", redo: e.shiftKey } : { kind: "ignore" };
  switch (e.code === "Tab" ? "Tab" : k) {
    case "F6":
      return { kind: "cycle_focus", back: e.shiftKey };
    case "r":
      return { kind: "rotate", back: e.shiftKey, repeat: e.repeat };
    case "b":
      return { kind: "toggle_bulldoze", repeat: e.repeat };
    case "p":
      return { kind: "toggle_photo", repeat: e.repeat };
    case "h":
      return { kind: "toggle_hidden", repeat: e.repeat };
    case "f":
      return { kind: "enter_walk", repeat: e.repeat };
    case "Escape":
      return { kind: "escape", step: escapeStep(st, flags) };
    case "Enter":
      if (onRowOrButton) return { kind: "leave" };
      // The ghost sits on the pinned slot when there is one, else on the hovered slot.
      return { kind: "place_ghost", at: st.tool === "place" && st.ghost ? { q: st.ghost.q, r: st.ghost.r } : null };
    default:
      return { kind: "leave" };
  }
}
