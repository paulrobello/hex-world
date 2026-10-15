// The photo panel's Effects section as data (owner, 2026-10-14): the values the engine reports in
// `state().photo`, their defaults, and the rules the panel draws from them. The engine owns the
// values (crates/hex-app/src/photo_fx.rs); the panel sends `photo` commands and pulls on entering.
import type { PhotoFilter, PhotoState } from "./bridge.ts";

/** The effect fields of the `photo` command and of `state().photo`; every amount is 0 to 1. */
export interface FxValues {
  filter: PhotoFilter;
  filter_strength: number;
  vignette: number;
  vignette_softness: number;
  grain: number;
  aberration: number;
}

/** The engine's defaults (`PhotoFxSettings::default`): every effect off. */
export const FX_DEFAULTS: Readonly<FxValues> = Object.freeze({
  filter: "normal",
  filter_strength: 1,
  vignette: 0,
  vignette_softness: 0.5,
  grain: 0,
  aberration: 0,
});

export const FX_FIELDS = Object.keys(FX_DEFAULTS) as (keyof FxValues)[];

/** The filter buttons, in panel order. */
export const FILTERS: readonly { value: PhotoFilter; text: string; label: string }[] = [
  { value: "normal", text: "None", label: "No filter" },
  { value: "mono", text: "B&W", label: "Black and white" },
  { value: "sepia", text: "Sepia", label: "Sepia" },
];

/** Amounts at or below this are off, as in the engine (`hex_render::photo_fx`). */
const OFF = 0.005;

/** A 0..1 amount as a whole slider percent, and back. */
export const toPercent = (v: number): number => Math.round(Math.min(1, Math.max(0, v)) * 100);
export const toUnit = (pct: number): number => Math.min(100, Math.max(0, pct)) / 100;
export const pctLabel = (pct: number): string => `${Math.round(pct)}%`;

export function fxFrom(p: PhotoState): FxValues {
  return {
    filter: p.filter,
    filter_strength: p.filter_strength,
    vignette: p.vignette,
    vignette_softness: p.vignette_softness,
    grain: p.grain,
    aberration: p.aberration,
  };
}

/** Whether any effect shows: the engine runs its pass exactly then. */
export function fxAnyOn(v: FxValues): boolean {
  const filter = v.filter !== "normal" && v.filter_strength > OFF;
  return filter || v.vignette > OFF || v.grain > OFF || v.aberration > OFF;
}

/** Whether every value is at its default, so Reset effects has nothing to do. */
export function fxIsDefault(v: FxValues): boolean {
  return FX_FIELDS.every((k) => {
    const a = v[k];
    const b = FX_DEFAULTS[k];
    return typeof a === "number" && typeof b === "number" ? Math.abs(a - b) < OFF : a === b;
  });
}

/** Which controls are inert: the strength without a filter, the softness without a vignette. */
export function fxDisabled(v: FxValues): { strength: boolean; softness: boolean } {
  return { strength: v.filter === "normal", softness: v.vignette <= OFF };
}
