#!/usr/bin/env node
// Initial-download gate (PRD §10 "Initial download before first interaction ≤30 MB", §15 P2).
//
// Sums what a Trunk-built dist/ sends before the first interaction, per texture family, and fails
// when the larger of BC7 and ASTC exceeds the budget. PNG (the fallback family) is reported only, with a warning within 1 MiB of the budget or over it.
// Kit content counts only the `core` pack of assets/kit/manifest.json; the other packs stream after
// the first interaction (crates/hex-play/src/play/packs.rs) and are printed per pack, not counted.
// A hero-class piece (compress-report budgets.class) must stream: one in `core` is bad input.
// `make web-smoke` (e2e/initial-download.mjs) checks this accounting against the files the play
// scenario requests before the first interaction, per family, through `account`.
//
// Usage: node scripts/size/initial-download.mjs [dist-dir]
//   INITIAL_DOWNLOAD_BUDGET_BYTES=<positive integer> overrides the 30 MiB budget (gate self-test).
// Exit: 0 pass, 1 over budget, 2 bad input (missing file, malformed dist/, a manifest entry with no
// pack, a core piece sampling a non-core set, a hero-class piece in core, bad override).
//
// Bytes are counted as GitHub Pages serves them. Probed on hex-world.pardev.net 2026-10-13:
// .wasm, .js, .html and .glb came back with content-encoding gzip; .ktx2, .png and .wgsl came
// back without it. So .wasm/.js/.html/.json/.glb count as gzipped, everything else counts raw.
// .wesl was not on the live site; it counts raw (unknown type, so Pages is assumed not to gzip it).
// .css is not in the probe either and counts raw (an overestimate if Pages gzips it).
// Gzipped bytes are Node zlib level 9: against the live wasm and GLB it read 0.08% and 0.6% above
// the bytes Pages sent, where macOS `gzip -9` read 1.2% below them.
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, extname, join, relative, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const BUDGET_DEFAULT = 31_457_280; // 30 MiB, the MiB convention `make web-size` uses for 12 MiB.
const WARN_HEADROOM = 1_048_576; // an ungated family within 1 MiB of the budget warns.
const MIB = 1_048_576;
const GZIP_EXT = new Set([".wasm", ".js", ".html", ".json", ".glb"]);
export const FAMILIES = { bc7: "bc7.ktx2", astc: "astc.ktx2", png: "png" }; // hex-assets family_suffix
const GATED = ["bc7", "astc"];
const CORE = "core"; // hex-assets CORE_PACK
// Not in the manifest: the IBL is hardcoded in crates/hex-render/src/lookdev.rs.
const IBL = ["p1/ibl/noon.diffuse.ktx2", "p1/ibl/noon.specular.ktx2"];
// The Legend inventory sheet is open at first paint and shows every tile's thumbnail (make thumbs).
const THUMBS = ["assets", "thumbs"];
// The TypeScript HUD overlay (web/ui/dist, copied to dist/web/ui by Trunk) when it is built.
const OVERLAY = ["web", "ui"];
// Shell files at the dist root besides index.html, counted when present: the browser's favicon
// request and the version check's poll (scripts/web/version-dist.mjs writes version.json).
const ROOT_SHELL = ["favicon.ico", "version.json"];
// The social share image is fetched only by link-preview crawlers, never by the page.
const NOT_FETCHED = [["web", "brand", "og-image.png"]];
const REPO = fileURLToPath(new URL("../..", import.meta.url));

/** Bad input (exit 2). */
export class AccountError extends Error {}
const die = (msg) => {
  throw new AccountError(msg);
};

function filesUnder(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { recursive: true })
    .map((f) => join(dir, f))
    .filter((f) => statSync(f).isFile())
    .sort();
}

/**
 * The initial download of `dist` per family: `categories` ([label, totals]), `total`, `initial`
 * (the counted files, dist-relative with "/" separators) and `packs` ([pack, totals, sets it
 * loads from another non-core pack]) for the streamed packs.
 */
export function account(dist) {
  dist = resolve(dist);
  const kitDir = join(dist, "assets", "kit");
  const served = new Map();
  const size = (path) => {
    if (!served.has(path)) {
      if (!existsSync(path)) die(`missing ${relative(dist, path)} in ${dist}`);
      const buf = readFileSync(path);
      const gz = GZIP_EXT.has(extname(path));
      served.set(path, { raw: buf.length, wire: gz ? gzipSync(buf, { level: 9 }).length : buf.length });
    }
    return served.get(path);
  };
  const kit = (p) => join(kitDir, p);
  const famFile = (stem, fam) => kit(`${stem}.${FAMILIES[fam]}`);

  // Read here, never fetched: the wasm embeds it (hex-render kit_scene::MANIFEST, include_str!).
  const manifestPath = join(kitDir, "manifest.json");
  if (!existsSync(manifestPath)) die(`no ${manifestPath}; run make build-web first`);
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"));

  const wasm = readdirSync(dist).filter((f) => f.endsWith(".wasm"));
  if (wasm.length !== 1) die(`expected exactly one .wasm in ${dist}, found ${wasm.length}`);
  const glue = readdirSync(dist).filter((f) => f.endsWith(".js"));
  if (glue.length === 0) die(`no JS glue (*.js) in ${dist}`);

  const budgetClass = (piece) => {
    const report = join(REPO, dirname(piece.provenance), "compress-report.json");
    if (!existsSync(report)) return "unknown";
    return JSON.parse(readFileSync(report, "utf8")).budgets?.class ?? "unknown";
  };

  // The runtime manifest names every piece's and set's pack (kit_gate checks the same).
  const missing = [
    ...Object.entries(manifest.materials).filter(([, s]) => typeof s.pack !== "string").map(([id]) => `set ${id}`),
    ...manifest.pieces.filter((p) => typeof p.pack !== "string").map((p) => `piece ${p.id}`),
  ];
  if (missing.length) die(`no "pack" on ${missing.join(", ")} in ${manifestPath}`);
  const sets = Object.entries(manifest.materials).map(([id, s]) => ({ id, ...s }));
  const pieces = manifest.pieces.map((p) => ({ ...p, cls: budgetClass(p) }));
  const packIds = [...new Set([CORE, ...pieces.map((p) => p.pack), ...sets.map((s) => s.pack)])];
  const inPack = (k) => ({ pieces: pieces.filter((p) => p.pack === k), sets: sets.filter((s) => s.pack === k) });
  // Sets a pack's pieces sample that another pack owns: loaded with the pack, counted in their owner.
  const borrowed = (k) => [...new Set(inPack(k).pieces.flatMap((p) =>
    Object.values(p.slots).map((sl) => sl.tileable).filter((id) => id && manifest.materials[id]?.pack !== k)))];
  if (borrowed(CORE).length) die(`core pieces sample non-core set(s) ${borrowed(CORE).join(", ")}`);
  const core = inPack(CORE);
  const heroCore = core.pieces.filter((p) => p.cls === "hero").map((p) => p.id);
  if (heroCore.length) die(`hero-class piece(s) ${heroCore.join(", ")} in the core pack; hero pieces stream (PRD §10)`);
  const pieceMaps = (ps, fam) => ps.flatMap((p) => Object.values(p.maps ?? {}).map((s) => famFile(s, fam)));
  const setMaps = (ss, fam) => ss.flatMap((s) => Object.values(s.maps).map((m) => famFile(m, fam)));
  const packFiles = (k, fam) => {
    const { pieces: ps, sets: ss } = inPack(k);
    return [...ps.map((p) => kit(p.glb)), ...pieceMaps(ps, fam), ...setMaps(ss, fam)];
  };

  const overlayDir = join(dist, ...OVERLAY);
  const notFetched = new Set(NOT_FETCHED.map((f) => join(dist, ...f)));
  const webShell = filesUnder(join(dist, "web")).filter((f) => !f.startsWith(overlayDir + sep) && !notFetched.has(f));
  const rootShell = ROOT_SHELL.map((f) => join(dist, f)).filter(existsSync);
  const categories = (fam) => [
    ["wasm", [join(dist, wasm[0])]],
    ["js glue", [...glue.map((f) => join(dist, f)), ...filesUnder(join(dist, "snippets"))]],
    ["html + web shell", [join(dist, "index.html"), ...rootShell, ...webShell]],
    ["ui overlay (web/ui)", filesUnder(overlayDir)],
    ["shaders", filesUnder(join(dist, "assets", "shaders"))],
    ["ibl", IBL.map(kit)],
    ["core piece glb", core.pieces.map((p) => kit(p.glb))],
    ["core piece maps", pieceMaps(core.pieces, fam)],
    ["core material sets", setMaps(core.sets, fam)],
    ["tile thumbnails", [join(dist, ...THUMBS, "manifest.json"), ...filesUnder(join(dist, ...THUMBS))]],
  ];
  const sum = (paths, seen) => {
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
  };
  const rel = (p) => relative(dist, p).split(sep).join("/");

  const counted = new Set();
  const families = {};
  for (const fam of Object.keys(FAMILIES)) {
    const seen = new Set();
    const total = { files: 0, raw: 0, wire: 0 };
    const cats = [];
    for (const [label, paths] of categories(fam)) {
      const t = sum(paths, seen);
      for (const k of Object.keys(total)) total[k] += t[k];
      cats.push([label, t]);
    }
    const initial = new Set([...seen].map(rel));
    const packs = packIds.filter((k) => k !== CORE).map((k) =>
      [k, sum(packFiles(k, fam), seen), borrowed(k).filter((id) => manifest.materials[id]?.pack !== CORE)]);
    for (const p of seen) counted.add(p);
    families[fam] = { categories: cats, total, initial, packs };
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
  return { dist, families, packs: packIds.map((k) => [k, inPack(k)]), skipped };
}

function budget() {
  const v = process.env.INITIAL_DOWNLOAD_BUDGET_BYTES;
  if (v === undefined || v === "") return BUDGET_DEFAULT;
  if (!/^[1-9][0-9]*$/.test(v)) die(`INITIAL_DOWNLOAD_BUDGET_BYTES must be a positive integer, got '${v}'`);
  return Number(v);
}

function main() {
  const limit = budget();
  const a = account(process.argv[2] ?? join(REPO, "dist"));
  const n = (v) => v.toLocaleString("en-US");
  const mib = (v) => (v / MIB).toFixed(2);
  const row = (label, t) =>
    `  ${label.padEnd(30)} ${String(t.files).padStart(5)} ${n(t.raw).padStart(14)} ${n(t.wire).padStart(14)} ${mib(t.wire).padStart(8)}`;
  console.log(`initial download of ${a.dist} (wire = zlib gzip level 9 for ${[...GZIP_EXT].join(" ")}; raw otherwise)`);
  for (const [fam, f] of Object.entries(a.families)) {
    console.log(`\nfamily ${fam}${GATED.includes(fam) ? " (gated)" : " (fallback, report only)"}`);
    console.log(`  ${"category".padEnd(30)} ${"files".padStart(5)} ${"raw B".padStart(14)} ${"wire B".padStart(14)} ${"wire MiB".padStart(8)}`);
    for (const [label, t] of f.categories) console.log(row(label, t));
    console.log(row("TOTAL initial download", f.total));
    let later = 0;
    for (const [k, t, b] of f.packs) {
      later += t.wire;
      console.log(row(`pack ${k} (streamed)`, t) + (b.length ? `  + loads ${b.join(", ")} from another pack` : ""));
    }
    console.log(`  all-as-core total: ${n(f.total.wire + later)} B (${mib(f.total.wire + later)} MiB)`);
  }
  console.log("\npacks:");
  for (const [k, { pieces: ps, sets: ss }] of a.packs) {
    const when = k === CORE ? "before the first interaction" : "streamed after the first interaction";
    console.log(`  ${k.padEnd(8)} ${when}: pieces ${ps.map((p) => `${p.id} (${p.cls})`).join(", ") || "-"}; sets ${ss.map((s) => s.id).join(", ") || "-"}`);
  }
  console.log("\ndist files not counted in any family or pack:");
  for (const [d, e] of a.skipped) console.log(`  ${d.padEnd(40)} ${String(e.files).padStart(4)} files ${n(e.raw).padStart(14)} B raw`);

  const totals = Object.fromEntries(Object.entries(a.families).map(([k, f]) => [k, f.total.wire]));
  const gated = GATED.reduce((x, y) => (totals[y] > totals[x] ? y : x));
  const verdict = `initial download ${n(totals[gated])} B (${mib(totals[gated])} MiB, family ${gated}) vs budget ${n(limit)} B (${mib(limit)} MiB), headroom ${n(limit - totals[gated])} B (${mib(limit - totals[gated])} MiB)`;
  // Ungated families (the PNG fallback) are monitored: a warning near or over the budget, no failure
  // (owner, 2026-10-21: keep report-only, watch it in case the lossless re-encode lever is needed).
  for (const fam of Object.keys(totals).filter((k) => !GATED.includes(k))) {
    const left = limit - totals[fam];
    if (left < WARN_HEADROOM)
      console.warn(`\nWARN: fallback family ${fam} ${n(totals[fam])} B (${mib(totals[fam])} MiB) is ${left < 0 ? `${mib(-left)} MiB over` : `within ${mib(left)} MiB of`} the ${mib(limit)} MiB budget (report only)`);
  }
  if (totals[gated] > limit) {
    console.error(`\nFAIL: ${verdict}`);
    process.exit(1);
  }
  console.log(`\nPASS: ${verdict}`);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    main();
  } catch (e) {
    if (!(e instanceof AccountError)) throw e;
    console.error(`initial-download: ${e.message}`);
    process.exit(2);
  }
}
