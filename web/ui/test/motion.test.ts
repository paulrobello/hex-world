// UI card criterion 3: every HUD state change has short eased feedback on the motion tokens, and
// reduced motion (the OS media query or the engine's own flag) cuts all of it.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";

const css = (f: string): string => readFileSync(new URL(`../src/${f}`, import.meta.url), "utf8");
const sheets = ["hud.css", "photo.css", "photo-fx.css"].map((f) => [f, css(f)] as const);
const design = JSON.parse(readFileSync(new URL("../../../.impeccable/design.json", import.meta.url), "utf8"))
  .extensions as { motion: { name: string; value: string }[] };
const token = (name: string): string => design.motion.find((m) => m.name === name)!.value;

/** Loops and long one-shots that are not state-change feedback, with why. */
const LONG = new Set([
  "blink 900ms ease-in-out infinite", // the "Saving…" dot while a write is pending
  "sweep 1.2s cubic-bezier(.45, 0, .55, 1) infinite", // indeterminate progress
  "focus-mark 1.2s var(--ease) forwards", // photo focus ring: settles in 180 ms, then holds and fades
  "focus-mark-still 1.2s linear forwards !important", // the same ring under reduced motion: opacity only
]);

test("motion tokens: one curve and short durations (120 to 250 ms)", () => {
  assert.equal(token("ease"), "cubic-bezier(.22, .9, .3, 1)");
  for (const t of ["t-fast", "t", "t-slow", "t-toast"]) {
    const ms = parseFloat(token(t));
    assert.ok(ms >= 120 && ms <= 250, `--${t} is ${ms} ms`);
  }
});

test("every HUD transition and animation runs on the motion tokens", () => {
  for (const [f, src] of sheets)
    for (const m of src.matchAll(/(?:transition|animation)(?:-duration)?:\s*([^;}]*)/g)) {
      const v = m[1]!.trim();
      if (LONG.has(v) || /^(0s|none)\b/.test(v)) continue;
      // Each comma-separated part names a token duration (a 0s visibility flip may ride along).
      for (const part of v.split(/,(?![^(]*\))/)) {
        const p = part.trim();
        if (/^visibility 0s linear var\(--t[\w-]*\)$/.test(p)) continue;
        assert.match(p, /var\(--t(-fast|-slow|-toast)?\)/, `${f}: "${p}" has no motion token`);
        assert.doesNotMatch(p, /\b\d+m?s\b/, `${f}: "${p}" carries a literal duration`);
        if (
          /^(opacity|transform|background|color|box-shadow|filter|width|stroke-dashoffset)\b/.test(p) &&
          !/linear/.test(p)
        )
          assert.match(p, /var\(--ease\)/, `${f}: "${p}" is not on the one curve`);
      }
    }
});

/** `selector { declarations }` pairs inside the first `@media (prefers-reduced-motion: reduce)` block at `from`. */
function mediaRules(src: string, from: number): [string, string][] {
  const open = src.indexOf("{", from);
  let depth = 1;
  let i = open + 1;
  while (depth) {
    if (src[i] === "{") depth++;
    else if (src[i] === "}") depth--;
    i++;
  }
  const body = src.slice(open + 1, i - 1).replace(/\/\*[\s\S]*?\*\//g, "");
  return [...body.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => [m[1]!.trim(), m[2]!.trim()]);
}

test("reduced motion: the engine flag's rules repeat the media query's, selector for selector", () => {
  for (const [f, src] of sheets) {
    for (const at of src.matchAll(/@media \(prefers-reduced-motion: reduce\)/g)) {
      for (const [sel, decl] of mediaRules(src, at.index!)) {
        if (sel.startsWith("@")) continue;
        const scoped = sel
          .split(",")
          .map((x) => `:root.is-reduced-motion ${x.trim()}`)
          .join(", ");
        const flat = src.replace(/\s+/g, " ");
        assert.ok(
          flat.includes(`${scoped} { ${decl.replace(/\s+/g, " ")} }`),
          `${f}: no :root.is-reduced-motion twin for "${sel}"`,
        );
      }
    }
  }
});

test("reduced motion: the blanket cut covers the HUD roots and every element under them", () => {
  const hud = css("hud.css");
  for (const scope of ["", ":root.is-reduced-motion "])
    assert.ok(
      hud.includes(
        `${scope}.hud, ${scope}.hud *, ${scope}.hud *::before, ${scope}.hud *::after { transition-duration: 0s !important; transition-delay: 0s !important; animation: none !important; }`,
      ),
      `blanket cut missing for scope "${scope}"`,
    );
});

test("motionMs reads a token whatever unit the minified sheet writes it in", async () => {
  const { cssMs } = await import("../src/dom.ts");
  assert.equal(cssMs("180ms", 0), 180);
  assert.equal(cssMs(" .18s", 0), 180, "the built hud.css writes 180ms as .18s");
  assert.equal(cssMs("0.24s", 0), 240);
  assert.equal(cssMs("", 240), 240);
});
