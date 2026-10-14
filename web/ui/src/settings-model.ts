// What the Settings sheet (S9) shows, from `state().tier` and the audio levels, kept free of the DOM
// so Node tests read it directly. The rows describe what each tier changes in the renderer today
// (hex_render: SSAO, the water variant, tilt-shift taps, boid caps), not the PRD §9.2 targets.
import type { QualityChoice, TierName, TierState } from "./bridge.ts";

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

/** The `set_tier` value for a row: Auto goes back to detection. */
export const tierCommand = (id: QualityChoice): { cmd: "set_tier"; tier: QualityChoice } => ({
  cmd: "set_tier",
  tier: id,
});

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
