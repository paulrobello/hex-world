// The import preview (S10.6), the refusal checks (S10.8) and the load confirm's words (S10.3),
// design/ui/legend/saves.html. Pure: the Worlds sheet renders what these return.
import type { InspectResult, LoadRefusalCode, PlayTile, SocketKind } from "./bridge.ts";
import { kb } from "./persist-ux.ts";
import { refusalText } from "./persist.ts";
import { rotated, tileDef } from "./rules.ts";
import { KIND_ORDER } from "./slots.ts";
import { plural } from "./words.ts";

/** The first bytes of every `.hexworld` file (hex-core `SAVE_MAGIC`). */
export const MAGIC = "HEXWORLD";
/** Magic, version and payload length: a file shorter than this has no readable version. */
const PREFIX_LEN = 14;

export type CheckState = "ok" | "bad" | "skip";
export interface Check {
  state: CheckState;
  text: string;
}

/** A refused file: the headline, the checks in the decoder's order, and what to do. */
export interface Refused {
  title: string;
  file: string;
  checks: Check[];
  todo: string;
}

/** A readable file: its name as a new world, the file line, and its legend rows. */
export interface Preview {
  name: string;
  file: string;
  tiles: PlayTile[];
  rows: { kind: SocketKind; tiles: number }[];
  version: number;
  supported: number;
}

/** File signatures a player is likely to pick by mistake, as "found X, which is Y". */
const SIGNATURES: { bytes: number[]; found: string; what: string }[] = [
  { bytes: [0x50, 0x4b, 0x03, 0x04], found: "PK", what: "a .zip archive" },
  { bytes: [0x50, 0x4b, 0x05, 0x06], found: "PK", what: "an empty .zip archive" },
  { bytes: [0x1f, 0x8b], found: "1F 8B", what: "a .gz archive" },
  { bytes: [0x37, 0x7a, 0xbc, 0xaf], found: "7z", what: "a .7z archive" },
  { bytes: [0x52, 0x61, 0x72, 0x21], found: "Rar!", what: "a .rar archive" },
  { bytes: [0x89, 0x50, 0x4e, 0x47], found: "PNG", what: "a PNG image" },
  { bytes: [0xff, 0xd8, 0xff], found: "FF D8 FF", what: "a JPEG image" },
  { bytes: [0x47, 0x49, 0x46, 0x38], found: "GIF8", what: "a GIF image" },
  { bytes: [0x25, 0x50, 0x44, 0x46], found: "%PDF", what: "a PDF document" },
];

/** What a file's first bytes say it is: "PK, which is a .zip archive", or the bytes themselves. */
export function sniff(head: Uint8Array): string {
  for (const sig of SIGNATURES)
    if (sig.bytes.every((b, i) => head[i] === b)) return `${sig.found}, which is ${sig.what}`;
  const first = [...head.subarray(0, MAGIC.length)];
  const text = String.fromCharCode(...first);
  if (first.length && first.every((b) => b >= 0x20 && b < 0x7f)) {
    const trimmed = text.trimStart();
    if (trimmed.startsWith("{") || trimmed.startsWith("[")) return `${text.trim()}, which is JSON text`;
    return text;
  }
  return first
    .slice(0, 4)
    .map((b) => b.toString(16).toUpperCase().padStart(2, "0"))
    .join(" ");
}

/** The decoder's stages in the order it runs them (hex-core `open_envelope`, then decode and validate). */
type Stage = "read" | "header" | "version" | "length" | "tiles";
const STAGES: Stage[] = ["read", "header", "version", "length", "tiles"];
const SKIP: Record<Stage, string> = {
  read: "Read the file, not checked",
  header: "Header, not checked",
  version: "Format version, not checked",
  length: "Length and checksum, not checked",
  tiles: "Tiles and sockets, not checked",
};

function failedStage(code: LoadRefusalCode, size: number): Stage {
  switch (code) {
    case "too_large":
    case "io":
    case "no_file":
      return "read";
    case "bad_magic":
      return "header";
    case "truncated":
      return size < PREFIX_LEN ? "header" : "length";
    case "newer_version":
    case "unsupported_version":
      return "version";
    case "trailing_bytes":
    case "checksum":
      return "length";
    default:
      return "tiles";
  }
}

const sizeText = (n: number): string => (n >= 1024 * 1024 ? `${(n / (1024 * 1024)).toFixed(1)} MB` : kb(n));

function badText(
  stage: Stage,
  code: LoadRefusalCode,
  reason: string,
  version: number | null,
  supported: number,
  head: Uint8Array,
  size: number,
): string {
  switch (stage) {
    case "read":
      return code === "too_large"
        ? `Read the file: ${sizeText(size)}, over the 16 MB a world file can be`
        : `Read the file: ${reason}`;
    case "header":
      if (code === "truncated")
        return size === 0 ? "Header: the file is empty" : `Header: the file ends after ${plural(size, "byte")}`;
      return `Header: expected ${MAGIC}, found ${sniff(head)}`;
    case "version":
      return code === "newer_version"
        ? `Format v${version}. This version reads up to v${supported}.`
        : `Format v${version}. Hex World reads v1 to v${supported}.`;
    case "length": {
      if (code === "truncated") {
        const m = /(\d+) of (\d+) bytes/.exec(reason);
        return m
          ? `Length: ${m[1]} of the ${m[2]} bytes its header names, so the file is cut short`
          : `Length: ${reason}`;
      }
      if (code === "trailing_bytes") {
        const n = Number(/(\d+)/.exec(reason)?.[1] ?? 0);
        return `Length: ${plural(n, "extra byte")} after the end of the world`;
      }
      return "Checksum: doesn't match the contents, so the file changed after it was saved";
    }
    default:
      return `Tiles and sockets: ${reason}`;
  }
}

function okText(stage: Stage, size: number, version: number | null, supported: number): string {
  switch (stage) {
    case "read":
      return `Read the file, ${sizeText(size)}`;
    case "header":
      return `Header: ${MAGIC}`;
    case "version":
      return version != null && version < supported
        ? `Format v${version}, updated to v${supported} when it opens`
        : `Format v${version ?? supported}`;
    case "length":
      return "Length and checksum match";
    default:
      return "Tiles and sockets";
  }
}

/** What to do about a refused file, after the checks. */
function todo(code: LoadRefusalCode, head: Uint8Array, reason: string): string {
  if (code === "bad_magic")
    return head[0] === 0x50 && head[1] === 0x4b
      ? "Unzip it first, then import the .hexworld file inside."
      : "Choose a file exported from Hex World.";
  if (code === "newer_version")
    return "Reload the page to get the newest Hex World, then import it again. Your worlds stay in this browser.";
  if (code === "unsupported_version") return "Choose a file exported from Hex World.";
  if (code === "decode" || code === "invalid_value" || code === "invalid_world" || code === "invalid_history")
    return "Export it again from the Hex World that made it.";
  if (code === "io" || code === "no_file") return "Choose the file again.";
  return refusalText(code, reason).sub;
}

/** The refusal checks panel for a file `inspect` refused, from the decoder's own code and reason. */
export function refusedOf(
  name: string,
  head: Uint8Array,
  size: number,
  r: Extract<InspectResult, { ok: false }>,
): Refused {
  const failed = failedStage(r.code, size);
  const at = STAGES.indexOf(failed);
  // The bridge names the version only for version refusals; past the header it is bytes 8 and 9.
  const version = r.version ?? (head.length >= 10 ? (head[8] ?? 0) | ((head[9] ?? 0) << 8) : null);
  const checks = STAGES.map((stage, i): Check => {
    if (i < at) return { state: "ok", text: okText(stage, size, version, r.supported) };
    if (i === at) return { state: "bad", text: badText(stage, r.code, r.reason, version, r.supported, head, size) };
    return { state: "skip", text: SKIP[stage] };
  });
  return {
    title: refusalText(r.code, r.reason).text,
    file: `${name} · ${sizeText(size)}`,
    checks,
    todo: todo(r.code, head, r.reason),
  };
}

/** Per socket kind, how many tiles carry it on at least one edge (the preview's legend rows). */
export function kindRows(tiles: readonly PlayTile[]): { kind: SocketKind; tiles: number }[] {
  const n = new Map<SocketKind, number>();
  for (const t of tiles) {
    const def = tileDef(t.tile);
    if (!def) continue;
    for (const k of new Set(rotated(def, t.rotation))) n.set(k, (n.get(k) ?? 0) + 1);
  }
  return KIND_ORDER.filter((k) => n.get(k)).map((kind) => ({ kind, tiles: n.get(kind) ?? 0 }));
}

/** The import preview for a file `inspect` read, under the world name it would open as. */
export function previewOf(
  file: string,
  size: number,
  worldName: string,
  r: Extract<InspectResult, { ok: true }>,
): Preview {
  return {
    name: worldName,
    file: `${file} · ${sizeText(size)} · format v${r.version}`,
    tiles: r.tiles,
    rows: kindRows(r.tiles),
    version: r.version,
    supported: r.supported,
  };
}

/** "11 Oct 2026": the preview's Saved row, from the file's own date. */
export function dayText(t: number): string {
  const d = new Date(t);
  return `${d.getDate()} ${d.toLocaleString("en-GB", { month: "short" })} ${d.getFullYear()}`;
}

/** The load confirm's line: "Riverside has changes from the last 2 minutes that aren't saved yet." */
export function unsavedText(name: string, since: number, now: number): string {
  const s = Math.max(0, Math.round((now - since) / 1000));
  const span = s < 60 ? "the last few seconds" : s < 120 ? "the last minute" : `the last ${Math.floor(s / 60)} minutes`;
  return `${name} has changes from ${span} that aren't saved yet.`;
}
