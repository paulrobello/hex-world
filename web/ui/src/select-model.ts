// The compact select's keyboard model (select.ts draws it): which keys open the list, move the
// highlighted option, pick it or close without a change, and type-ahead by option name. Free of the
// DOM so Node tests read it directly.

export type SelectAction =
  /** Open the list with this option highlighted. */
  | { kind: "open"; active: number }
  /** Highlight another option; nothing is picked yet. */
  | { kind: "move"; active: number }
  /** Pick this option and close. */
  | { kind: "pick"; index: number }
  /** Close without a change (Esc, Tab). */
  | { kind: "close" };

/**
 * What a key does to a select of `n` options. Closed, ArrowDown and ArrowUp open it on the current
 * option (`active`), Home and End on the first and last; Enter and Space click the button, which
 * opens it too. Open, the arrows move the highlight without wrapping and without picking, Home,
 * End, PageUp and PageDown jump to the ends, Enter and Space pick the highlighted option, and Esc
 * and Tab close it with nothing changed.
 */
export function selectKey(open: boolean, active: number, n: number, key: string): SelectAction | null {
  if (n <= 0) return null;
  const at = Math.min(n - 1, Math.max(0, active));
  if (!open) {
    if (key === "ArrowDown" || key === "ArrowUp") return { kind: "open", active: at };
    if (key === "Home") return { kind: "open", active: 0 };
    if (key === "End") return { kind: "open", active: n - 1 };
    return null;
  }
  switch (key) {
    case "ArrowDown":
      return { kind: "move", active: active < 0 ? 0 : Math.min(n - 1, active + 1) };
    case "ArrowUp":
      return { kind: "move", active: active < 0 ? n - 1 : Math.max(0, active - 1) };
    case "Home":
    case "PageUp":
      return { kind: "move", active: 0 };
    case "End":
    case "PageDown":
      return { kind: "move", active: n - 1 };
    case "Enter":
    case " ":
      return active < 0 ? null : { kind: "pick", index: at };
    case "Escape":
    case "Tab":
      return { kind: "close" };
    default:
      return null;
  }
}

/** Typed characters further apart than this start a new type-ahead search. */
export const TYPE_AHEAD_MS = 600;

/**
 * The type-ahead buffer after one more typed character: it grows while keys come within
 * `TYPE_AHEAD_MS` of each other and restarts after a pause.
 */
export function typeBuffer(prev: string, prevAt: number, char: string, now: number): string {
  return prev && now - prevAt <= TYPE_AHEAD_MS ? prev + char : char;
}

/** Whether a key extends an open select's type-ahead: any printable character, and Space while a search runs. */
export function typesAhead(key: string, buffer: string, bufferAt: number, now: number): boolean {
  if (key.length !== 1) return false;
  if (key === " ") return buffer !== "" && now - bufferAt <= TYPE_AHEAD_MS;
  return true;
}

/**
 * The option a type-ahead search lands on, or null: the first name after `from` (wrapping) that
 * starts with `query`, ignoring case. A longer query may stay on the current option; one letter, or
 * the same letter repeated, steps to the next name starting with it, so "r r r" cycles Rich 1, 2, 3.
 */
export function typeAhead(names: readonly string[], from: number, query: string): number | null {
  const n = names.length;
  if (n === 0 || query === "") return null;
  const q = query.toLowerCase();
  const repeated = [...q].every((c) => c === q[0]);
  const needle = repeated ? q[0]! : q;
  const start = repeated ? from + 1 : from;
  for (let k = 0; k < n; k++) {
    const i = (((start + k) % n) + n) % n;
    if (names[i]!.toLowerCase().startsWith(needle)) return i;
  }
  return null;
}
