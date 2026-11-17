// Start-up resume decisions (ENH-003): whether a start resumes the last world or opens safe.
// Pure helpers, unit-tested without a DOM: the ?safe=1 address parameter and the sessionStorage
// crash flag both turn the resume off for one start; anything else resumes.

/** The start-up modes: `resume` opens the most recent slot; `safe` skips it for the start world. */
export type StartupResume = "resume" | "safe";

/**
 * The start-up mode: safe when the address asks for it (`?safe=1`, or just `?safe`; a value of
 * `0` does not count) or when the last start raised the resuming flag and never got its `loaded`
 * answer — the page crashed, froze or aborted opening that world. Anything else resumes.
 */
export function startupResume(search: string, crashed: boolean): StartupResume {
  if (crashed) return "safe";
  const v = new URLSearchParams(search).get("safe");
  return v !== null && v !== "0" ? "safe" : "resume";
}

/**
 * The sessionStorage flag a start-up resume raises before `hexWorld.load` and the engine's
 * `loaded` event clears. A start that finds it still raised knows the last start never finished
 * opening its world and opens safe instead. Session-scoped: closing the tab retries the resume,
 * and a refusal clears it, so only an unanswered hang or abort keeps skipping the resume.
 */
export const RESUME_FLAG = "hexworld.resuming";

/** Raises the flag for the slot a start-up resume is about to open. No flag without storage. */
export function raiseResumeFlag(id: string): void {
  try {
    sessionStorage.setItem(RESUME_FLAG, id);
  } catch {
    // Private browsing or blocked storage: no flag, so the resume retries like today.
  }
}

export function clearResumeFlag(): void {
  try {
    sessionStorage.removeItem(RESUME_FLAG);
  } catch {
    // Nothing to clear without storage.
  }
}

export function resumeFlagRaised(): boolean {
  try {
    return sessionStorage.getItem(RESUME_FLAG) !== null;
  } catch {
    return false;
  }
}
