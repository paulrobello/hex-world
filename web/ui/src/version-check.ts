// "New version available" (owner request 2026-10-15): polls version.json (version-check-model.ts) and,
// once a newer build is live, shows the update note, a toast that stays until Reload (DESIGN.md, Toast).
// Reload writes the world first (Persistence.settle); with autosave off the note says the world is lost.
import { h } from "./dom.ts";
import { icon } from "./icons.ts";
import type { Persistence } from "./persist.ts";
import { VersionWatch, pollMs, updateWords } from "./version-check-model.ts";

declare global {
  interface Window {
    /** This page's build id, inlined by scripts/web/version-dist.mjs. */
    __HEX_BUILD?: string;
  }
}

export class UpdateNote {
  private readonly root: HTMLElement;
  private readonly persist: () => Persistence | null;
  private readonly reload: () => void;
  private el: HTMLElement | null = null;
  private readonly sub = h("small", {});
  private readonly btn = h("button", { class: "toast__action", type: "button" }, "Reload");
  private failed = false;
  private saving = false;

  constructor(root: HTMLElement, persist: () => Persistence | null, reload: () => void = () => location.reload()) {
    this.root = root;
    this.persist = persist;
    this.reload = reload;
    this.btn.addEventListener("click", () => void this.go());
  }

  show(): void {
    if (!this.el) {
      this.el = h(
        "div",
        { class: "toast toast--update", "data-region": "update", role: "status", "aria-live": "polite" },
        icon("retry"),
        h("p", {}, h("span", {}, "New version available"), this.sub),
        this.btn,
      );
      this.root.append(this.el);
      void this.el.offsetWidth;
      this.el.classList.add("is-shown");
    }
    this.sync();
  }

  /** Re-reads the save state into the words (persistence's `changed`, and on show). */
  sync(): void {
    const el = this.el;
    if (!el || this.saving) return;
    const p = this.persist();
    const storage = this.failed ? "failed" : p && !p.storageOn ? "off" : "on";
    const w = updateWords(storage);
    this.sub.textContent = w.sub;
    this.btn.textContent = w.action;
    el.classList.toggle("toast--caution", storage !== "on");
  }

  private async go(): Promise<void> {
    if (this.saving) return;
    const p = this.persist();
    if (p?.storageOn && !this.failed) {
      this.saving = true;
      this.btn.textContent = "Saving…";
      this.btn.setAttribute("aria-disabled", "true");
      const saved = await p.settle();
      this.saving = false;
      this.btn.removeAttribute("aria-disabled");
      if (!saved) {
        this.failed = true;
        this.sync();
        return;
      }
    }
    this.reload();
  }
}

/** Checks on an interval (`version_poll` seconds in the address, else 3 minutes), on focus, when the
 *  tab shows again and when the browser comes back online. */
export function watchVersion(note: UpdateNote, search: string = location.search): VersionWatch {
  const watch = new VersionWatch({
    page: typeof window.__HEX_BUILD === "string" ? window.__HEX_BUILD : null,
    fetch: (url, init) => fetch(url, init),
    online: () => navigator.onLine !== false,
    now: Date.now,
    onNewer: () => note.show(),
  });
  window.setInterval(() => void watch.check(true), pollMs(search));
  addEventListener("focus", () => void watch.check());
  addEventListener("online", () => void watch.check());
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") void watch.check();
  });
  return watch;
}
