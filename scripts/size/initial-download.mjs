#!/usr/bin/env node
// Initial-download gate (PRD §10 "Initial download before first interaction ≤30 MB", §15 P2).
//
// Sums what a Trunk-built dist/ sends before the first interaction, per texture family, and fails
// when the larger of BC7 and ASTC exceeds the budget. PNG (the fallback family) is reported only.
//
// Usage: node scripts/size/initial-download.mjs [dist-dir]
//   INITIAL_DOWNLOAD_BUDGET_BYTES=<positive integer> overrides the 30 MiB budget (gate self-test).
// Exit: 0 pass, 1 over budget, 2 bad input (missing file, malformed dist/, bad override).
//
// Bytes are counted as GitHub Pages serves them. Probed on hex-world.pardev.net 2026-10-13:
// .wasm, .js, .html and .glb came back with content-encoding gzip; .ktx2, .png and .wgsl came
// back without it. So .wasm/.js/.html/.json/.glb count as gzipped, everything else counts raw.
// .wesl was not on the live site; it counts raw (unknown type, so Pages is assumed not to gzip it).
// Gzipped bytes are Node zlib level 9: against the live wasm and GLB it read 0.08% and 0.6% above
// the bytes Pages sent, where macOS `gzip -9` read 1.2% below them.
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, extname, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const BUDGET_DEFAULT = 31_457_280; // 30 MiB, the MiB convention `make web-size` uses for 12 MiB.
const MIB = 1_048_576;
const GZIP_EXT = new Set([".wasm", ".js", ".html", ".json", ".glb"]);
const FAMILIES = { bc7: "bc7.ktx2", astc: "astc.ktx2", png: "png" }; // hex-assets family_suffix
const GATED = ["bc7", "astc"];
// Budget classes (compress-report.json `budgets.class`) that stream after first interaction.
const STREAMED_CLASSES = new Set(["hero"]);
// Not in the manifest: the IBL is hardcoded in crates/hex-render/src/lookdev.rs.
const IBL = ["p1/ibl/noon.diffuse.ktx2", "p1/ibl/noon.specular.ktx2"];
// The default web scenario (Single) loads this tile in crates/hex-render/src/kit_test.rs.
const DEFAULT_TILE = { glb: "p0/hex_test.glb", stem: "p0/hex_test" };
// The Legend inventory sheet is open at first paint and shows every tile's thumbnail (make thumbs).
const THUMBS = ["assets", "thumbs"];

const limit = budget();
const repo = fileURLToPath(new URL("../..", import.meta.url));
const dist = process.argv[2] ?? join(repo, "dist");
const kitDir = join(dist, "assets", "kit");

function die(msg) {
  console.error(`initial-download: ${msg}`);
  process.exit(2);
}

function budget() {
  const v = process.env.INITIAL_DOWNLOAD_BUDGET_BYTES;
  if (v === undefined || v === "") return BUDGET_DEFAULT;
  if (!/^[1-9][0-9]*$/.test(v)) die(`INITIAL_DOWNLOAD_BUDGET_BYTES must be a positive integer, got '${v}'`);
  return Number(v);
}

function filesUnder(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { recursive: true })
    .map((f) => join(dir, f))
    .filter((f) => statSync(f).isFile())
    .sort();
}

const served = new Map();
function size(path) {
  if (!served.has(path)) {
    if (!existsSync(path)) die(`missing ${relative(dist, path)} in ${dist}`);
    const buf = readFileSync(path);
    const gz = GZIP_EXT.has(extname(path));
    served.set(path, { raw: buf.length, wire: gz ? gzipSync(buf, { level: 9 }).length : buf.length });
  }
  return served.get(path);
}

const kit = (p) => join(kitDir, p);
const famFile = (stem, fam) => kit(`${stem}.${FAMILIES[fam]}`);

const manifestPath = join(kitDir, "manifest.json");
if (!existsSync(manifestPath)) die(`no ${manifestPath}; run make build-web first`);
const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));

const wasm = readdirSync(dist).filter((f) => f.endsWith(".wasm"));
if (wasm.length !== 1) die(`expected exactly one .wasm in ${dist}, found ${wasm.length}`);
const glue = readdirSync(dist).filter((f) => f.endsWith(".js"));
if (glue.length === 0) die(`no JS glue (*.js) in ${dist}`);

function budgetClass(piece) {
  const report = join(repo, dirname(piece.provenance), "compress-report.json");
  if (!existsSync(report)) return "unknown"; // No class recorded: counted as core.
  return JSON.parse(readFileSync(report, "utf8")).budgets?.class ?? "unknown";
}
const pieces = manifest.pieces.map((p) => ({ ...p, cls: budgetClass(p) }));
const core = pieces.filter((p) => !STREAMED_CLASSES.has(p.cls));
const streamed = pieces.filter((p) => STREAMED_CLASSES.has(p.cls));
const pieceMaps = (ps, fam) => ps.flatMap((p) => Object.values(p.maps ?? {}).map((s) => famFile(s, fam)));

function categories(fam) {
  return [
    ["wasm", [join(dist, wasm[0])]],
    ["js glue", [...glue.map((f) => join(dist, f)), ...filesUnder(join(dist, "snippets"))]],
    ["html + web shell", [join(dist, "index.html"), ...filesUnder(join(dist, "web"))]],
    ["shaders", filesUnder(join(dist, "assets", "shaders"))],
    ["kit manifest", [manifestPath]],
    ["ibl", IBL.map(kit)],
    ["default tile (p0)", [kit(DEFAULT_TILE.glb), famFile(DEFAULT_TILE.stem, fam)]],
    ["core piece glb", core.map((p) => kit(p.glb))],
    ["core piece maps", pieceMaps(core, fam)],
    ["material sets", Object.values(manifest.materials).flatMap((s) => Object.values(s.maps).map((m) => famFile(m, fam)))],
    ["tile thumbnails", [join(dist, ...THUMBS, "manifest.json"), ...filesUnder(join(dist, ...THUMBS))]],
  ];
}

function sum(paths, seen) {
  const t = { files: 0, raw: 0, wire: 0 };
  for (const p of paths) {
    if (seen.has(p)) continue;
    seen.add(p);
    const s = size(p);
    t.files += 1;
    t.raw += s.raw;
    t.wire += s.wire;
  }
  return t;
}

const n = (v) => v.toLocaleString("en-US");
const mib = (v) => (v / MIB).toFixed(2);
const row = (label, t) =>
  `  ${label.padEnd(30)} ${String(t.files).padStart(5)} ${n(t.raw).padStart(14)} ${n(t.wire).padStart(14)} ${mib(t.wire).padStart(8)}`;

const counted = new Set();
const totals = {};
console.log(`initial download of ${dist} (wire = zlib gzip level 9 for ${[...GZIP_EXT].join(" ")}; raw otherwise)`);
for (const fam of Object.keys(FAMILIES)) {
  const seen = new Set();
  console.log(`\nfamily ${fam}${GATED.includes(fam) ? " (gated)" : " (fallback, report only)"}`);
  console.log(`  ${"category".padEnd(30)} ${"files".padStart(5)} ${"raw B".padStart(14)} ${"wire B".padStart(14)} ${"wire MiB".padStart(8)}`);
  const total = { files: 0, raw: 0, wire: 0 };
  for (const [label, paths] of categories(fam)) {
    const t = sum(paths, seen);
    for (const k of Object.keys(total)) total[k] += t[k];
    console.log(row(label, t));
  }
  console.log(row("TOTAL initial download", total));
  const later = sum([...streamed.map((p) => kit(p.glb)), ...pieceMaps(streamed, fam)], seen);
  console.log(row("streamed (not counted)", later));
  console.log(`  all-as-core total: ${n(total.wire + later.wire)} B (${mib(total.wire + later.wire)} MiB)`);
  for (const p of seen) counted.add(p);
  totals[fam] = total.wire;
}

console.log("\npieces:");
for (const p of pieces) {
  console.log(`  ${p.id.padEnd(24)} class ${p.cls.padEnd(9)} ${STREAMED_CLASSES.has(p.cls) ? "streamed after first interaction" : "core"}`);
}
const skipped = new Map();
for (const f of filesUnder(dist)) {
  if (counted.has(f)) continue;
  const d = relative(dist, dirname(f)).split(sep).join("/") || ".";
  const e = skipped.get(d) ?? { files: 0, raw: 0 };
  e.files += 1;
  e.raw += statSync(f).size;
  skipped.set(d, e);
}
console.log("\ndist files not counted in any family:");
for (const [d, e] of skipped) console.log(`  ${d.padEnd(40)} ${String(e.files).padStart(4)} files ${n(e.raw).padStart(14)} B raw`);

const gated = GATED.reduce((a, b) => (totals[b] > totals[a] ? b : a));
const verdict = `initial download ${n(totals[gated])} B (${mib(totals[gated])} MiB, family ${gated}) vs budget ${n(limit)} B (${mib(limit)} MiB)`;
if (totals[gated] > limit) {
  console.error(`\nFAIL: ${verdict}`);
  process.exit(1);
}
console.log(`\nPASS: ${verdict}`);
