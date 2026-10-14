// The audio manifest (assets/audio/manifest.json, written by make audio-master) as the runtime reads it:
// which file format to fetch, which cues are playable, which game events fire which cues, and which
// decoded cues fit the in-memory budget (PRD §8.3, §10). No DOM here, so node --test covers it.

export type Format = "webm" | "m4a";
/** The mixer buses, matching the Legend Settings → Audio sliders (design/ui/legend/settings.html). */
export type Bus = "music" | "ambience" | "effects" | "interface";

export interface CueFile {
  path: string;
  sha256: string;
  bytes: number;
}

export interface LoopPoints {
  startSample: number;
  endSample: number;
  sampleRate: number;
  startSeconds: number;
  endSeconds: number;
}

export interface Cue {
  id: string;
  kind: string;
  class: string;
  status: string;
  events: string[];
  /** The occasional one-shot pool this cue is one variant of (`forest-birds`, `forest-leaves`). */
  pool?: string;
  playback?: "decode" | "stream";
  channels?: number;
  loop?: LoopPoints | null;
  durationSeconds?: number;
  decodedBytes?: number;
  targetLufs?: number;
  files?: Partial<Record<Format, CueFile>>;
}

export interface Manifest {
  version: number;
  sampleRate: number;
  decodedBudgetBytes: number;
  formats: Format[];
  cues: Cue[];
}

/** `canPlayType` strings per format: Opus in WebM is primary, AAC in MP4 the fallback (Safari). */
export const MIME: Record<Format, string> = {
  webm: 'audio/webm; codecs="opus"',
  m4a: 'audio/mp4; codecs="mp4a.40.2"',
};

/** The manifest's first format the browser answers "probably" for, else its first "maybe", else null. */
export function pickFormat(formats: readonly Format[], canPlay: (mime: string) => string): Format | null {
  const known = formats.filter((f) => f in MIME);
  return known.find((f) => canPlay(MIME[f]) === "probably") ?? known.find((f) => canPlay(MIME[f]) === "maybe") ?? null;
}

/** Cues mastered and encoded in `format`; pending Suno sources and missing encodes drop out. */
export const playable = (cues: readonly Cue[], format: Format): Cue[] =>
  cues.filter((c) => c.status === "mastered" && !!c.files?.[format]);

/** Game event type → cue ids, in manifest order, from each cue's `events` list. */
export function eventCues(cues: readonly Cue[]): Map<string, string[]> {
  const m = new Map<string, string[]>();
  for (const c of cues)
    for (const e of c.events) {
      const ids = m.get(e) ?? [];
      ids.push(c.id);
      m.set(e, ids);
    }
  return m;
}

/** Cue class → mixer bus (assets-source/audio/README.md, Classes and targets). */
export function busOf(c: Cue): Bus {
  if (c.class === "music" || c.kind === "music") return "music";
  if (c.class === "loop" || c.kind === "ambience") return "ambience";
  if (c.class === "ui") return "interface";
  return "effects";
}

/** Bytes a cue occupies once decoded: float32 per sample per channel, at the context rate. */
export function decodedSize(c: Cue, manifestRate: number, contextRate: number): number {
  const scale = contextRate / manifestRate;
  if (c.decodedBytes) return Math.ceil(c.decodedBytes * scale);
  return Math.ceil((c.durationSeconds ?? 0) * contextRate * (c.channels ?? 1) * 4);
}

export interface DecodePlan {
  load: Cue[];
  skipped: { id: string; reason: string }[];
  bytes: number;
}

// Feedback first: interface and placement one-shots are small and must answer the first click.
const PRIORITY: Record<string, number> = { ui: 0, sfx: 1, loop: 2 };

/**
 * The decode-playback cues to load, interface and placement one-shots first, then ambience loops,
 * keeping the decoded total within `budget` bytes. Streamed cues (music) never count against it.
 */
export function planDecode(
  cues: readonly Cue[],
  budget: number,
  manifestRate: number,
  contextRate: number,
): DecodePlan {
  const order = cues
    .map((c, i) => ({ c, i }))
    .filter(({ c }) => c.playback !== "stream")
    .sort((a, b) => (PRIORITY[a.c.class] ?? 3) - (PRIORITY[b.c.class] ?? 3) || a.i - b.i);
  const plan: DecodePlan = { load: [], skipped: [], bytes: 0 };
  for (const { c } of order) {
    const size = decodedSize(c, manifestRate, contextRate);
    if (plan.bytes + size > budget) {
      plan.skipped.push({ id: c.id, reason: `decoded budget: ${plan.bytes + size} B > ${budget} B` });
      continue;
    }
    plan.bytes += size;
    plan.load.push(c);
  }
  return plan;
}
