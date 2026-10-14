// Builds the HUD overlay into web/ui/dist (shipped as web/ui/ by index.html's Trunk copy-dir):
// hud.js (one IIFE bundle), hud.css (design tokens + styles), fonts/ (Manrope, OFL).
// Usage: node build.mjs (make ui-build).
import { copyFileSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";
import * as esbuild from "esbuild";

const here = dirname(fileURLToPath(import.meta.url));
const repo = join(here, "..", "..");
const out = join(here, "dist");

// Tokens come from the approved Legend design sidecar (design.json): colours, shadows, motion.
function tokensCss() {
  const d = JSON.parse(readFileSync(join(repo, "design/ui/legend/design.json"), "utf8")).extensions;
  const lines = [];
  for (const [name, c] of Object.entries(d.colorMeta)) {
    const v = /^--[\w-]+$/.test(c.cssVar) ? c.cssVar : `--${name}`;
    lines.push(`${v}: ${c.canonical};`);
  }
  for (const s of d.shadows) if (/^(shadow|glow|ring)-/.test(s.name)) lines.push(`--${s.name}: ${s.value};`);
  for (const m of d.motion)
    if (/^(ease|t|t-fast|t-slow|t-toast|toast-dwell|tip-delay)$/.test(m.name)) lines.push(`--${m.name}: ${m.value};`);
  return `:root{${lines.join("")}}\n`;
}

// Per-rotation thumbnails (assets/thumbs/manifest.json) are baked in when present at build time, so
// the page never fetches a manifest that may not exist; without it the rows draw socket glyphs.
function thumbs() {
  const p = join(repo, "assets/thumbs/manifest.json");
  if (!existsSync(p)) return null;
  const m = JSON.parse(readFileSync(p, "utf8"));
  return { tiles: m.tiles, size: m.css_size ?? [48, 48] };
}

rmSync(out, { recursive: true, force: true });
mkdirSync(join(out, "fonts"), { recursive: true });

await esbuild.build({
  entryPoints: [join(here, "src/main.ts")],
  outfile: join(out, "hud.js"),
  bundle: true,
  format: "iife",
  target: "es2022",
  minify: true,
  legalComments: "none",
  define: { HEXW_THUMBS: JSON.stringify(thumbs()) },
  logLevel: "warning",
});

const css = await esbuild.transform(tokensCss() + readFileSync(join(here, "src/hud.css"), "utf8"), {
  loader: "css",
  minify: true,
  target: "chrome113",
});
writeFileSync(join(out, "hud.css"), css.code);
for (const f of ["manrope-latin-wght.woff2", "OFL-manrope.txt"])
  copyFileSync(join(here, "fonts", f), join(out, "fonts", f));

const report = ["hud.js", "hud.css"].map((f) => {
  const b = readFileSync(join(out, f));
  return `${f} ${b.length} B (gzip ${gzipSync(b, { level: 9 }).length} B)`;
});
console.log(`ui-build: ${report.join(", ")}; thumbnails ${thumbs() ? "baked" : "absent (socket glyphs)"}`);
