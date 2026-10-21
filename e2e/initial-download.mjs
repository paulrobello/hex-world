// Local-only (GPU Chromium): the initial-download gate's accounting (scripts/size/initial-download.mjs,
// PRD §10) against what the play scenario requests before the first interaction, per texture family.
// Each family is forced by hiding the adapter's compression features (BC7: none hidden; ASTC: BC
// hidden; PNG: BC and ASTC hidden), so the fallback family is measured, not only computed.
// After READY and 2 s with no input, it fails when
//   - a request is not in the gate's initial set for that family (the gate undercounts), or
//   - a kit file the gate counts was never requested (core accounting drifted from the runtime), or
//   - the app picked another family, or a non-core pack left idle.
// Counted files the page did not request outside the kit (thumbnails of rows below the fold, the
// favicon variants) are printed with their bytes: an overcount, which keeps the gate conservative.
// Usage: node e2e/initial-download.mjs [dist]   (after make build-web)
import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { extname, join } from "node:path";
import { account } from "../scripts/size/initial-download.mjs";

const dist = process.argv[2] ?? "dist";
const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css",
  ".ktx2": "image/ktx2", ".glb": "model/gltf-binary", ".png": "image/png", ".wgsl": "text/plain",
  ".jpg": "image/jpeg", ".woff2": "font/woff2", ".webp": "image/webp", ".svg": "image/svg+xml", ".json": "application/json" };
const server = createServer(async (req, res) => {
  let path = decodeURIComponent(new URL(req.url, "http://x").pathname);
  if (path.endsWith("/")) path += "index.html";
  try {
    const body = await readFile(join(dist, path));
    res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" });
    res.end(body);
  } catch { res.writeHead(404); res.end(); }
}).listen(0);
const base = `http://127.0.0.1:${server.address().port}/`;
const gpuArgs = process.env.PW_GPU === "swiftshader"
  ? ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--enable-unsafe-webgpu"]
  : ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu"];
const fail = (m) => { console.error("FAIL:", m); process.exitCode = 1; };
const MIB = 1_048_576;

const gate = account(dist);
// Feature-name prefixes hidden per family (wgpu reads `texture-compression-bc`, `-astc`).
const HIDE = { bc7: [], astc: ["texture-compression-bc"], png: ["texture-compression-bc", "texture-compression-astc"] };

for (const [fam, hide] of Object.entries(HIDE)) {
  const tag = `initial-download ${fam}`;
  const browser = await chromium.launch({ channel: "chromium", headless: true, args: gpuArgs });
  try {
    const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
    await page.addInitScript((hide) => {
      if (!window.GPUAdapter) return;
      const real = Object.getOwnPropertyDescriptor(GPUAdapter.prototype, "features");
      Object.defineProperty(GPUAdapter.prototype, "features", {
        get() {
          return new Set([...real.get.call(this)].filter((f) => !hide.some((h) => f.startsWith(h))));
        },
      });
    }, hide);
    const requested = new Map();
    page.on("request", (r) => {
      const u = new URL(r.url());
      if (u.origin + "/" !== base) return;
      let p = decodeURIComponent(u.pathname).replace(/^\//, "");
      if (p === "" || p.endsWith("/")) p += "index.html";
      requested.set(p, (requested.get(p) ?? 0) + 1);
    });
    const families = [];
    page.on("console", (m) => {
      const f = /kit: texture family (\w+)/.exec(m.text());
      if (f) families.push(f[1].toLowerCase());
    });
    await page.goto(base + "?scenario=play&tier=high");
    if (!(await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 120_000 }).then(() => true, () => false))) {
      fail(`${tag}: never ready`);
      continue;
    }
    await page.waitForTimeout(2_000);
    const s = await page.evaluate(() => window.hexWorld.state());
    const notIdle = Object.entries(s.packs ?? {}).filter(([k, v]) => k !== "core" && v !== "idle");
    if (s.packs?.core !== "ready") fail(`${tag}: core not ready: ${JSON.stringify(s.packs)}`);
    if (notIdle.length) fail(`${tag}: packs requested before any interaction: ${JSON.stringify(s.packs)}`);
    const fams = [...new Set(families)];
    if (fams.length && !(fams.length === 1 && fams[0] === fam)) fail(`${tag}: the app picked family ${fams.join(", ")}`);
    const maps = [...requested.keys()].filter((p) => /\.(bc7\.ktx2|astc\.ktx2|png)$/.test(p) && p.startsWith("assets/kit/") && !p.includes("/ibl/"));
    const seen = [...new Set(maps.map((p) => p.match(/\.(bc7\.ktx2|astc\.ktx2|png)$/)[1]))];
    const want = { bc7: "bc7.ktx2", astc: "astc.ktx2", png: "png" }[fam];
    if (!(seen.length === 1 && seen[0] === want)) fail(`${tag}: kit maps requested in ${JSON.stringify(seen)}, expected ${want}`);

    const initial = gate.families[fam].initial;
    const uncounted = [...requested.keys()].filter((p) => !initial.has(p));
    if (uncounted.length) fail(`${tag}: requested before the first interaction but not counted by the gate: ${uncounted.join(", ")}`);
    const unrequested = [...initial].filter((p) => !requested.has(p));
    const kitUnrequested = unrequested.filter((p) => p.startsWith("assets/kit/"));
    if (kitUnrequested.length) fail(`${tag}: counted by the gate but never requested: ${kitUnrequested.join(", ")}`);
    let fetched = 0, over = 0;
    for (const p of requested.keys()) if (initial.has(p)) fetched += (await stat(join(dist, p))).size;
    const overByDir = new Map();
    for (const p of unrequested) {
      const b = (await stat(join(dist, p))).size;
      over += b;
      const d = p.split("/").slice(0, -1).join("/") || ".";
      overByDir.set(d, (overByDir.get(d) ?? 0) + 1);
    }
    const total = gate.families[fam].total;
    console.log(`${tag}: ${requested.size} files requested before the first interaction (${(fetched / MIB).toFixed(2)} MiB raw), ` +
      `${uncounted.length} not counted; gate counts ${total.files} files, ${(total.wire / MIB).toFixed(2)} MiB wire; ` +
      `${unrequested.length} counted but not requested (${(over / MIB).toFixed(2)} MiB raw: ${[...overByDir].map(([d, n]) => `${d} ${n}`).join(", ") || "-"})`);
  } finally {
    await browser.close();
  }
}
server.close();
