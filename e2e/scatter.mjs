// Local-only (GPU Chromium, never CI): the scatter checkpoint's 20-tile world on WebGPU with the device
// clamped to WebGPU's default 16 sampled textures per stage and post=full, so scattered spruces, rocks
// and wildflowers draw through the kit pipelines next to refracting water. Asserts every kit forward
// pipeline binds exactly 16 (with the view transmission texture), the per-tile scatter equals the
// native dump at the same seed (docs/checkpoints/scatter/scatter-10-world.json), and the pass census
// stays within PRD §10's 500 draws. Writes e2e/test-results/web-scatter-*.png and web-scatter-state.json.
// Usage: node e2e/scatter.mjs <dist> <docs/checkpoints/scatter>
import { chromium } from "playwright";
import { createServer } from "node:http";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { extname, join } from "node:path";
import { BAD_TEXT } from "./console-errors.mjs";
import { clampSampledTextures, countSampledTextures } from "./webgpu-probes.mjs";

const dist = process.argv[2] ?? "dist";
const docs = process.argv[3] ?? "docs/checkpoints/scatter";
const outDir = "e2e/test-results";
const CLAMP = 16;
const native = JSON.parse(await readFile(join(docs, "scatter-10-world.json"), "utf8"));
// The checkpoint's placements (its first and its rising forest), then a still of the world and the cluster.
const placing = JSON.parse(await readFile(join(docs, "scatter.json"), "utf8"))
  .filter((c) => c.cmd === "select_tile" || c.cmd === "place");
const script = [
  ...placing,
  { cmd: "hover", q: 9, r: -9 },
  { cmd: "toggle", what: "photo" },
  { cmd: "photo", dof: "off", pause_agents: false },
  { cmd: "camera", pos: [20, 160, 150], look_at: [20, 0, -5] },
  { cmd: "wait", frames: 900 },
  { cmd: "screenshot", name: "web-scatter-overview" },
  { cmd: "camera", pos: [46, 58, 48], look_at: [46, 0, -12] },
  { cmd: "wait", frames: 60 },
  { cmd: "screenshot", name: "web-scatter-cluster" },
];

const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css",
  ".ktx2": "image/ktx2", ".glb": "model/gltf-binary", ".png": "image/png", ".wgsl": "text/plain",
  ".jpg": "image/jpeg", ".woff2": "font/woff2" };
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
let failed = 0;
const check = (ok, what, got) => { console.log(`${ok ? "ok  " : "FAIL"} ${what}: ${JSON.stringify(got)}`); if (!ok) failed++; };

await mkdir(outDir, { recursive: true });
const browser = await chromium.launch({ channel: "chromium", headless: true, args: gpuArgs });
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));
page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });
await page.addInitScript(() => {
  if (!window.GPUAdapter) return;
  const rd = GPUAdapter.prototype.requestDevice;
  GPUAdapter.prototype.requestDevice = function (...a) {
    return rd.apply(this, a).then((dev) => {
      dev.addEventListener("uncapturederror", (e) => console.error("uncapturederror: " + e.error?.message));
      return dev;
    });
  };
});
await page.addInitScript(clampSampledTextures, CLAMP);
await page.addInitScript(countSampledTextures);
try {
  await page.goto(base + `?scenario=play&post=full&fps=off&seed=${native.seed}`);
  await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 120_000 });
  for (const c of script) {
    if (c.cmd === "screenshot") {
      const url = await page.evaluate((n) => window.hexWorld.screenshot(n, 180_000), c.name);
      const file = join(outDir, `${c.name}.png`);
      await writeFile(file, Buffer.from(url.replace(/^data:image\/png;base64,/, ""), "base64"));
      console.log("wrote", file);
    } else {
      await page.evaluate((j) => window.hexWorld.cmd(j), JSON.stringify(c));
    }
  }
  const deadline = Date.now() + 180_000;
  let state;
  while (Date.now() < deadline) {
    state = await page.evaluate(() => window.hexWorld.state());
    const s = state.scatter;
    if (s && Object.values(state.packs ?? {}).every((v) => v === "ready") && s.waiting.length === 0 &&
      s.drawn >= native.scatter.drawn && state.diag?.passes?.length) break;
    await page.waitForTimeout(500);
  }
  await page.waitForTimeout(2_000);
  const { sampled, limit } = await page.evaluate(() => ({ sampled: window.__sampledTextures, limit: window.__deviceSampledLimit }));
  state = await page.evaluate(() => window.hexWorld.state());
  await writeFile(join(outDir, "web-scatter-state.json"), JSON.stringify(state));
  check(state.tiles === native.tiles, `${native.tiles} tiles, as natively`, state.tiles);
  check(Object.values(state.packs).every((v) => v === "ready"), "every pack ready", state.packs);
  check(JSON.stringify(state.scatter.tiles) === JSON.stringify(native.scatter.tiles),
    `every tile's planned, placed and drawn scatter equals the native dump at seed ${native.seed}`,
    { drawn: state.scatter.drawn, native: native.scatter.drawn, by: state.scatter.drawn_by_piece });
  check(limit === CLAMP && state.gpu?.maxSampledTexturesPerShaderStage === CLAMP, `device clamped to ${CLAMP}`, { limit, gpu: state.gpu });
  check(sampled?.max <= CLAMP, `no pipeline layout over ${CLAMP} sampled textures`, sampled?.max);
  const fwd = (sampled?.kit ?? []).filter((k) => k.pass === "forward" && k.ibl);
  check(fwd.length > 0 && fwd.every((k) => k.sampled === CLAMP && k.transmission),
    "kit forward pipelines bind exactly 16 with the transmission texture", fwd.map((k) => k.sampled));
  const passes = state.diag.passes;
  const draws = passes.reduce((n, p) => n + p.multidraw_sets + p.batchable_bins + p.unbatchable + p.non_mesh, 0);
  check(draws <= 500, "draws over every pass and cascade within 500 (PRD §10)", { draws, mode: state.diag.gpu_preprocessing_supported,
    passes: passes.map((p) => `${p.view}/${p.phase}:${p.multidraw_sets}+${p.batchable_bins}+${p.unbatchable}+${p.non_mesh}`) });
  check(errors.length === 0, "no console or uncaptured errors", errors.slice(0, 5));
} catch (e) {
  console.error("FAIL:", String(e));
  failed++;
} finally {
  await browser.close();
  server.close();
}
console.log(failed ? `web-scatter: ${failed} check(s) failed` : "web-scatter OK");
process.exit(failed ? 1 : 0);
