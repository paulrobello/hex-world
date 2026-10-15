// What the Settings sheet (S9) shows, from `state().tier`, `state().grade` and the audio levels, kept
// free of the DOM so Node tests read it directly. The rows describe what each tier changes in the
// renderer today (hex_render: SSAO, the water variant, tilt-shift taps, boid caps), not the PRD §9.2
// targets. The colour grade rows take their descriptions from the engine.
import {
  GRADES,
  GRASS,
  type GradeName,
  type GradeState,
  type GrassAlias,
  type GrassState,
  type QualityChoice,
  type TierName,
  type TierState,
} from "./bridge.ts";

export const TIER_NAME: Record<TierName, string> = { low: "Low", medium: "Medium", high: "High", ultra: "Ultra" };

const TIER_DETAIL: Record<TierName, string> = {
  low: "No SSAO · no water refraction · a quarter of the birds",
  medium: "No SSAO · no water reflection · half the birds",
  high: "SSAO · full water · 1,000 birds and fish",
  ultra: "High plus a 24-sample tilt-shift blur",
};

export interface TierRow {
  id: QualityChoice;
  name: string;
  detail: string;
  /** The right-hand tag: what Auto uses here, or "Detected" on the detected tier's row. */
  tag: string | null;
  detected: boolean;
}

export interface QualityView {
  selected: QualityChoice;
  /** The tier Auto applies on this device (the warm-up probe's answer when it ran); null before detection. */
  autoTier: TierName | null;
  rows: TierRow[];
  /** The detected tier and the detection's reason, in one line; null before detection. */
  reason: string | null;
  /** Shown while Ultra is selected: Ultra is opt-in, above what detection picks. */
  caution: { title: string; body: string } | null;
}

/** Auto is selected while the tier comes from detection (or before it has run). */
export function selectedQuality(t: TierState | undefined): QualityChoice {
  if (!t || t.source === "default" || t.source === "detected" || t.source === "probe") return "auto";
  return t.tier;
}

export function autoTier(t: TierState | undefined): TierName | null {
  return t?.probe?.tier ?? t?.detected?.tier ?? null;
}

/** Ultra is offered only where detection lands on High (it never picks Ultra itself), or while it is on. */
export function ultraAllowed(t: TierState | undefined): boolean {
  return autoTier(t) === "high" || t?.tier === "ultra";
}

const ms = (v: number): string => `${Math.round(v * 10) / 10} ms`;

export function qualityView(t: TierState | undefined): QualityView {
  const auto = autoTier(t);
  const selected = selectedQuality(t);
  const probe = t?.probe ?? null;
  const ids: TierName[] = ultraAllowed(t) ? ["low", "medium", "high", "ultra"] : ["low", "medium", "high"];
  const rows: TierRow[] = [
    {
      id: "auto",
      name: "Auto",
      detail: probe
        ? `Chosen from this GPU and a ${ms(probe.median_ms)} warm-up`
        : t?.detected
          ? "Chosen from this GPU"
          : "Chosen once the GPU has started",
      tag: auto ? `Uses ${TIER_NAME[auto]} here` : "Detecting…",
      detected: false,
    },
    ...ids.map((id) => ({
      id,
      name: TIER_NAME[id],
      detail: TIER_DETAIL[id],
      tag: id === auto ? "Detected" : null,
      detected: id === auto,
    })),
  ];
  let reason: string | null = null;
  if (t?.detected && auto) {
    const stepped = probe && probe.tier !== t.detected.tier;
    reason = `Detected ${TIER_NAME[auto]}: ${t.detected.reason}`;
    if (probe) reason += `. Warm-up ${ms(probe.median_ms)} a frame${stepped ? ", one tier down" : ""}`;
    if (t.source === "explicit") reason += `. The page address sets ${TIER_NAME[t.tier]}`;
  }
  const caution =
    selected === "ultra" && auto
      ? {
          title: `Detection chose ${TIER_NAME[auto]} for this GPU`,
          body: `Ultra adds a finer tilt-shift blur on top of High${probe ? `. ${TIER_NAME[probe.tier]} ran ${ms(probe.median_ms)} a frame in the warm-up` : ""}.`,
        }
      : null;
  return { selected, autoTier: auto, rows, reason, caution };
}

/**
 * Whether the tier-change layer stays up: the request sent after `sentAt` requests is not handled
 * yet, or a change (ours, a loaded save's quality, the warm-up probe) is rebuilt but not yet drawn.
 * A no-op request clears it as soon as it is handled.
 */
export function tierBusy(t: TierState | undefined, sentAt: number | null): boolean {
  if (sentAt != null && (t?.requests ?? 0) <= sentAt) return true;
  return !!t && t.drawn !== t.changes;
}

/** The `set_tier` value for a row: Auto goes back to detection. */
export const tierCommand = (id: QualityChoice): { cmd: "set_tier"; tier: QualityChoice } => ({
  cmd: "set_tier",
  tier: id,
});

/** The viewer's colour grade in localStorage; it is not part of the world save. */
export const GRADE_KEY = "hexWorld.grade";

/** A grade's display name: "aces" is "ACES", "rich-2" is "Rich 2". */
export function gradeLabel(name: string): string {
  if (name === "aces") return "ACES";
  const words = name.replaceAll("-", " ");
  return words.charAt(0).toUpperCase() + words.slice(1);
}

export interface GradeRow {
  id: GradeName;
  name: string;
  detail: string;
  /** "Default" on the engine's default grade, else null. */
  tag: string | null;
}

export interface GradeView {
  /** The checked row: a pending pick until `state().grade.name` follows, else the engine's grade. */
  selected: GradeName | null;
  rows: GradeRow[];
}

export function gradeView(g: GradeState | undefined, pick: GradeName | null = null): GradeView {
  return {
    selected: pick ?? g?.name ?? null,
    rows: (g?.presets ?? []).map((p) => ({
      id: p.name,
      name: gradeLabel(p.name),
      detail: p.detail,
      tag: p.name === g?.default ? "Default" : null,
    })),
  };
}

export const gradeCommand = (grade: GradeName): { cmd: "set_grade"; grade: GradeName } => ({
  cmd: "set_grade",
  grade,
});

export const isGrade = (v: string | null | undefined): v is GradeName =>
  (GRADES as readonly string[]).includes(v ?? "");

/**
 * The grade to send at startup: the stored choice, unless the page address names one (`?grade=` is
 * read by the engine itself and wins) or the stored value is not a known grade.
 */
export function startupGrade(search: string, stored: string | null): GradeName | null {
  if (new URLSearchParams(search).has("grade")) return null;
  return isGrade(stored) ? stored : null;
}

/** The viewer's grass density in localStorage (whole percent of Normal); it is not part of the world save. */
export const GRASS_KEY = "hexWorld.grass";

/** The Grass density slider: percent of the Normal density (28 blades per m²), 0 is off. */
export const GRASS_SLIDER = { min: 0, max: 150, step: 5, normal: 100 } as const;

/** The stepped choice the slider replaced, as the percent a stored value migrates to. */
export const GRASS_ALIAS_PERCENT: Record<GrassAlias, number> = { off: 0, sparse: 50, normal: 100, lush: 150 };

/** A slider percent clamped to the range and snapped to its step. */
export function grassPercent(v: number): number {
  const { min, max, step } = GRASS_SLIDER;
  return Math.min(max, Math.max(min, Math.round(v / step) * step));
}

/**
 * The stored density as a slider percent: a whole number, or a stepped choice from before the
 * slider (off 0, sparse 50, normal 100, lush 150). Null without one, or for anything else.
 */
export function storedGrass(stored: string | null): number | null {
  if (stored == null) return null;
  if ((GRASS as readonly string[]).includes(stored)) return GRASS_ALIAS_PERCENT[stored as GrassAlias];
  if (!/^\d+(\.\d+)?$/.test(stored.trim())) return null;
  return grassPercent(Number(stored));
}

export interface GrassView {
  /** The slider percent: a pending value until `state().grass.density` follows, else the engine's. */
  value: number | null;
  /** Grass draws only on High and Ultra: the slider is disabled below. */
  enabled: boolean;
}

export function grassView(
  g: GrassState | undefined,
  tier: TierState | undefined,
  pick: number | null = null,
): GrassView {
  return {
    value: pick ?? (g ? Math.round(g.density * 100) : null),
    enabled: tier?.tier === "high" || tier?.tier === "ultra",
  };
}

/** `set_grass` for a slider percent: the engine takes the share of Normal. */
export const grassCommand = (pct: number): { cmd: "set_grass"; density: number } => ({
  cmd: "set_grass",
  density: grassPercent(pct) / 100,
});

/**
 * The density to send at startup, as a slider percent: the stored choice (a stepped one migrates),
 * unless the page address names one (`?grass=` is read by the engine itself and wins) or the
 * stored value is unreadable.
 */
export function startupGrass(search: string, stored: string | null): number | null {
  if (new URLSearchParams(search).has("grass")) return null;
  return storedGrass(stored);
}

/** Sends at most once per `ms` while a slider drags; `flush` sends the last value at once (on release). */
export function throttle(
  send: (v: number) => void,
  ms: number,
  now: () => number = () => performance.now(),
): { push: (v: number) => void; flush: () => void } {
  let last = Number.NEGATIVE_INFINITY;
  let pending: number | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  const fire = (): void => {
    if (timer) clearTimeout(timer);
    timer = null;
    if (pending == null) return;
    const v = pending;
    pending = null;
    last = now();
    send(v);
  };
  return {
    push(v: number): void {
      pending = v;
      const wait = last + ms - now();
      if (wait <= 0) fire();
      else timer ??= setTimeout(fire, wait);
    },
    flush: fire,
  };
}

/** Arrow keys over a roving group of `n` items (tabs, tier radios): the index to move to, or null. */
export function roveIndex(i: number, n: number, key: string, axis: "x" | "y"): number | null {
  const fwd = axis === "x" ? "ArrowRight" : "ArrowDown";
  const back = axis === "x" ? "ArrowLeft" : "ArrowUp";
  if (key === fwd) return (i + 1) % n;
  if (key === back) return (i - 1 + n) % n;
  if (key === "Home") return 0;
  if (key === "End") return n - 1;
  return null;
}

/** The audio layer's buses in the S9.5 mockup's order, with its hint. */
export type Channel = "master" | "music" | "ambience" | "effects" | "interface";
export const CHANNELS: { bus: Channel; label: string; hint?: string }[] = [
  { bus: "master", label: "Master" },
  { bus: "music", label: "Music" },
  { bus: "ambience", label: "Ambience", hint: "Rivers, surf, birds and town, loudest where you zoom in." },
  { bus: "effects", label: "Effects" },
  { bus: "interface", label: "Interface" },
];

/** A 0–1 level as the slider's whole percent. */
export const percent = (v: number): number => Math.round(Math.min(1, Math.max(0, v)) * 100);
