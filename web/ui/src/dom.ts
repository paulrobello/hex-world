// Tiny DOM helpers. Nothing here writes a style attribute: the CSP drops 'unsafe-inline' for styles
// eventually, so colour goes through classes and positions through CSSOM properties.

type Attrs = Record<string, string | number | boolean | null | undefined>;
type Child = Node | string | null | undefined | false;

export function h<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Attrs = {},
  ...children: Child[]
): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v == null || v === false) continue;
    if (k === "class") el.className = String(v);
    else el.setAttribute(k, v === true ? "" : String(v));
  }
  for (const c of children) if (c != null && c !== false) el.append(c);
  return el;
}

const SVG = "http://www.w3.org/2000/svg";

export function s(tag: string, attrs: Attrs = {}, ...children: (SVGElement | null)[]): SVGElement {
  const el = document.createElementNS(SVG, tag) as SVGElement;
  for (const [k, v] of Object.entries(attrs)) if (v != null && v !== false) el.setAttribute(k, String(v));
  for (const c of children) if (c) el.append(c);
  return el;
}

const REDUCE_QUERY = "(prefers-reduced-motion: reduce)";
let engineReduced = false;

/** Reduced motion: the OS setting, or the engine's own flag (`set_reduced_motion`, `play.reduced_motion`). */
export const reduceMotion = (): boolean => engineReduced || matchMedia(REDUCE_QUERY).matches;

/**
 * Follows the engine's reduced-motion flag. `is-reduced-motion` on the root element carries either
 * source to CSS, so a scripted or Settings override cuts the HUD's motion as the media query does.
 */
export function syncReducedMotion(engine: boolean): void {
  engineReduced = engine;
  const on = reduceMotion();
  const root = document.documentElement;
  if (root.classList.contains("is-reduced-motion") !== on) root.classList.toggle("is-reduced-motion", on);
}

/** A motion token (`--t-fast`, `--t`, `--t-slow`, `--t-toast`) in milliseconds, for Web Animations. */
export function motionMs(token: string, fallback: number): number {
  const v = parseFloat(getComputedStyle(document.documentElement).getPropertyValue(token));
  return Number.isFinite(v) ? v : fallback;
}

/** Restarts a one-shot CSS animation class. */
export function retrigger(el: Element, cls: string): void {
  el.classList.remove(cls);
  void (el as HTMLElement).offsetWidth;
  el.classList.add(cls);
}

export const isMac = (): boolean => /Mac|iPhone|iPad/.test(navigator.platform);
