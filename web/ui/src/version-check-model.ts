// The new-version check (owner request 2026-10-15). scripts/web/version-dist.mjs inlines the build id
// into index.html as window.__HEX_BUILD and writes the same id to version.json; this polls that file
// and reports an id that differs from the page's. version-check.ts runs it in the page.

/** How often the live build is checked: every 3 minutes, plus on focus and when the tab shows. */
export const POLL_MS = 180_000;
/** A focus or visibility check this soon after the last one is skipped. */
export const MIN_GAP_MS = 15_000;

/** The poll interval: `version_poll=<seconds>` in the address (scripts and tests), else POLL_MS. */
export function pollMs(search: string): number {
  const raw = new URLSearchParams(search).get("version_poll");
  const s = raw == null || raw === "" ? Number.NaN : Number(raw);
  return Number.isFinite(s) && s > 0 ? Math.max(1000, Math.round(s * 1000)) : POLL_MS;
}

/** The id a version.json body names, or null when the body is not one. */
export function liveId(body: unknown): string | null {
  const id = (body as { id?: unknown } | null)?.id;
  return typeof id === "string" && /^[0-9a-f]{6,64}$/.test(id) ? id : null;
}

export interface WatchDeps {
  /** The page's own build id (window.__HEX_BUILD); with none the check never runs. */
  page: string | null;
  fetch: (url: string, init: RequestInit) => Promise<Response>;
  online: () => boolean;
  now: () => number;
  /** A live build other than the page's: called once per new id. */
  onNewer: (id: string) => void;
}

export class VersionWatch {
  private readonly deps: WatchDeps;
  private busy = false;
  private last = Number.NEGATIVE_INFINITY;
  private found: string | null = null;

  constructor(deps: WatchDeps) {
    this.deps = deps;
  }

  /**
   * One check of version.json, fetched past every cache. A timed check (`timed`) always runs; a focus
   * or visibility check is skipped within MIN_GAP_MS of the last. Offline, a failed request, a bad
   * status or body are silent: the next check retries.
   */
  async check(timed = false): Promise<void> {
    const d = this.deps;
    const now = d.now();
    if (!d.page || this.busy || (!timed && now - this.last < MIN_GAP_MS) || !d.online()) return;
    this.busy = true;
    this.last = now;
    try {
      const res = await d.fetch(`version.json?t=${now}`, { cache: "no-store" });
      if (!res.ok) return;
      const id = liveId(await res.json());
      if (id && id !== d.page && id !== this.found) {
        this.found = id;
        d.onNewer(id);
      }
    } catch {
      // Silent by design: the next check retries.
    } finally {
      this.busy = false;
    }
  }
}

/** The update note's words: the world is saved first, or plainly not saved (autosave off). */
export function updateWords(storage: "on" | "off" | "failed"): { sub: string; action: string } {
  switch (storage) {
    case "off":
      return { sub: "Autosave is off, so reloading loses this world. Export it from Worlds first.", action: "Reload" };
    case "failed":
      return { sub: "This world could not be saved. Export it from Worlds first.", action: "Reload anyway" };
    default:
      return { sub: "Your world is saved before the page reloads.", action: "Reload" };
  }
}
