import type { PlayTile } from "./bridge.ts";
import type { Note } from "./persist.ts";
import { cleanName, fileName, mapOf, mixOf, nameFromFile, nextName } from "./slots.ts";

/** Presentation effects consumed by the save state machine. */
export interface PersistenceUiHost {
  notify?: (note: Note) => void;
  download?: (name: string, bytes: Uint8Array) => void;
}

export class PersistenceUx {
  private readonly host: PersistenceUiHost;

  constructor(host: PersistenceUiHost) {
    this.host = host;
  }

  notify(note: Note): void {
    this.host.notify?.(note);
  }

  exported(world: string, bytes: Uint8Array): void {
    const name = fileName(world);
    this.host.download?.(name, bytes);
    this.notify({
      text: `Exported ${name}`,
      sub: `${kb(bytes.length)}, in your downloads`,
      icon: "export",
      tone: "ok",
    });
  }

  autosaveOff(stopped: boolean, onExport: () => void): void {
    this.notify({
      text: stopped ? "Autosave stopped" : "Autosave is off in this window",
      sub: stopped
        ? "This browser refused to store the world. Export keeps it."
        : "This browser blocks storage here. Export keeps your world.",
      icon: "alert",
      tone: "caution",
      action: "Export",
      onAction: onExport,
    });
  }

  nextName(names: string[]): string {
    return nextName(names);
  }

  cleanName(raw: string): string {
    return cleanName(raw);
  }

  importName(file: string, names: string[]): string {
    return this.unique(nameFromFile(file), names);
  }

  unique(name: string, names: string[]): string {
    const taken = new Set(names.map((n) => n.toLowerCase()));
    if (!taken.has(name.toLowerCase())) return name;
    for (let i = 2; ; i++) {
      const n = `${name} ${i}`;
      if (!taken.has(n.toLowerCase())) return n;
    }
  }

  summary(tiles: PlayTile[]): { tiles: number; mix: ReturnType<typeof mixOf>; map: ReturnType<typeof mapOf> } {
    return { tiles: tiles.length, mix: mixOf(tiles), map: mapOf(tiles) };
  }
}

export const kb = (n: number): string => (n < 1024 ? `${n} B` : `${Math.max(1, Math.round(n / 1024))} KB`);
