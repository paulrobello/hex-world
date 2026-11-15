// The viewer's preferences in `localStorage`, outside the world save. Site data may be blocked or
// full (a private window, a quota), so every access can throw: these wrappers turn that into a
// result the caller can ignore, and nothing here throws.

/** The stored value for `key`, or null when there is none or storage refuses the read. */
export function readStored(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

/** Stores `value` under `key`; false when storage refuses (the choice then holds until the page closes). */
export function writeStored(key: string, value: string): boolean {
  try {
    localStorage.setItem(key, value);
    return true;
  } catch {
    return false;
  }
}

/** Forgets `key`; a refusing storage leaves it as it is. */
export function removeStored(key: string): void {
  try {
    localStorage.removeItem(key);
  } catch {
    /* Storage refused: nothing to forget that was ever kept. */
  }
}
