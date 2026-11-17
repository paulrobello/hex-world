// Debug-only ENH-002 audit (spike): compares the engine's published `play.fits` against the
// sheet's TypeScript mirror (`fitEntry`) for the same slot, and measures how many engine frames a
// slot change waits for the matching `fits` patch. It ticks with the debug HUD's performance
// panel, so it costs nothing while the overlay is off.
import type { PlayState } from "./bridge.ts";
import { PICKER, cellKey, fitEntry, needsAt, tileMap } from "./rules.ts";
import { refusalWord } from "./words.ts";

export class FitsAudit {
  private pending: string | null = null;
  private pendingAt = 0;
  private awaiting = false;
  private lastDelta: number | null = null;
  private disagreements = 0;
  private logged = new Set<string>();
  private lastRun = 0;

  /** Ticks the audit and returns its debug-HUD line. */
  update(st: PlayState, frame: number): string {
    // The sheet's slot, by SheetController.sync's rule: the pin, else the hovered slot.
    const slot = st.pinned ?? (st.tool === "place" && st.hovered_kind === "slot" ? st.hovered : null);
    const key = slot ? cellKey(slot) : null;
    if (key !== this.pending) {
      this.pending = key;
      this.pendingAt = frame;
      this.awaiting = true;
    }
    // The patch for the slot counts the frames the sheet would have waited for it, sampled
    // every frame so the delta is exact; only the comparison runs at the panel's 4 Hz.
    if (st.fits && key && `${st.fits.slot.q},${st.fits.slot.r}` === key) {
      if (this.awaiting) {
        this.lastDelta = frame - this.pendingAt;
        this.awaiting = false;
      }
      const now = performance.now();
      if (now - this.lastRun >= 250) {
        this.lastRun = now;
        this.compare(st, slot!);
      }
    }
    return `fits audit: ${this.disagreements} disagree, patch after ${this.lastDelta ?? "–"}f`;
  }

  /** Compares every entry against the mirror for the slot. */
  private compare(st: PlayState, slot: { q: number; r: number }): void {
    const map = tileMap(st.tiles);
    const needs = needsAt(slot, map);
    const mega = { slot, megaSlots: st.mega_slots, tiles: map };
    for (const entry of st.fits!.entries) {
      const pick = PICKER.find((p) => p.id === entry.id);
      if (!pick) {
        this.disagree(`unknown ${entry.id}@${key(slot)}`);
        continue;
      }
      const mirror = fitEntry(pick, needs, mega);
      const fits = entry.orientations.length > 0;
      if (mirror.ok !== fits) {
        this.disagree(`ok ${entry.id}@${key(slot)}: mirror ${mirror.ok} vs engine ${fits}`);
        continue;
      }
      const seq = mirror.seq.map((t) => [t.k, t.level] as [number, number]);
      const got = entry.orientations;
      if (seq.length !== got.length || seq.some(([k, l], i) => got[i]![0] !== k || got[i]![1] !== l)) {
        this.disagree(`seq ${entry.id}@${key(slot)}: mirror ${JSON.stringify(seq)} vs engine ${JSON.stringify(got)}`);
      }
      // A code is published exactly when the mirror words a refusal, and words.ts words it.
      if (entry.refusal === null ? mirror.why !== "" : refusalWord(entry.refusal) === "") {
        this.disagree(`refusal ${entry.id}@${key(slot)}: mirror "${mirror.why}" vs engine ${entry.refusal}`);
      }
    }
  }

  /** Counts a disagreement and logs each distinct one once. */
  private disagree(what: string): void {
    this.disagreements++;
    if (!this.logged.has(what)) {
      this.logged.add(what);
      console.warn(`[fits audit] ${what}`);
      console.warn(`[fits audit] total ${this.disagreements}`);
    }
  }
}

const key = (c: { q: number; r: number }): string => `${c.q},${c.r}`;
