// Local-only browser smoke test: GPU Chromium, WebGPU backend, gate redirect, bad scenario fallback.
import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join } from "node:path";
import { BAD_TEXT } from "./console-errors.mjs";
import { clampSampledTextures, countSampledTextures, markedModuleProbe } from "./webgpu-probes.mjs";

const dist = process.argv[2] ?? "dist";
const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm",
  ".ktx2": "image/ktx2", ".glb": "model/gltf-binary", ".png": "image/png", ".wgsl": "text/plain" };
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

// --enable-unsafe-webgpu must stay out of the no-GPU run: with it, --disable-gpu still yields a SwiftShader adapter.
const gpuArgs = process.env.PW_GPU === "swiftshader"
  ? ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--enable-unsafe-webgpu"]
  : ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu"];
const fail = (m) => { console.error("FAIL:", m); process.exitCode = 1; };
// Handle-let aliases the shim rewrites once the kit scenario has drawn: one module, the KitMaterial
// fragment (Bevy's PBR input composed in), asserted by its `masks_texture` global (markedModuleProbe).
const KIT_COMPAT_ALIASES = 8;
// WebGPU's default maxSampledTexturesPerShaderStage.
const WEBGPU_DEFAULT_SAMPLED = 16;
// Measured fragment-stage sampled textures of the kit pipelines with post=full (2026-10-13). The
// forward count is asserted exactly, so the clamped case cannot pass without the kit pipeline.
// `ibl=on` (the kit default) adds the two noon environment cubemaps to the view bind group, which
// puts the forward pipeline exactly at WebGPU's default 16 (measured 2026-10-13): no headroom left.
// The tilt-shift pass samples the colour target and the prepass depth.
const TILT_SAMPLED = 2;
const KIT_SAMPLED = { ibl: { forward: 16, prepass: 7 }, noIbl: { forward: 14, prepass: 7 } };

async function withPage(args, fn) {
  const browser = await chromium.launch({ channel: "chromium", headless: true, args });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const no = ++caseNo;
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  // Bevy's wasm logger prints ERROR through console.log, so match the text, not only the console type.
  page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });
  // Surface WebGPU validation errors that nothing catches.
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
  try {
    await fn(page, errors);
    if (errors.length) fail(`console errors (case ${no}): ` + errors.join(" | "));
  } finally { await browser.close(); }
}
let caseNo = 0;

// 1. Default scenario becomes ready on the WebGPU backend.
await withPage(gpuArgs, async (page, errors) => {
  const kit = new Map();
  page.on("console", (m) => { if (/texture family/i.test(m.text())) console.log("app:", m.text()); });
  page.on("requestfinished", async (r) => {
    if (!r.url().includes("/assets/kit/")) return;
    const t = r.timing(), size = (await r.sizes()).responseBodySize;
    kit.set(r.url().split("/").pop(), { ms: Math.round(t.responseEnd), bytes: size });
  });
  await page.goto(base);
  await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 60_000 });
  // The kit disc loads after the renderer is up; screenshot once both files have arrived.
  const deadline = Date.now() + 20_000;
  while (kit.size < 2 && Date.now() < deadline) await page.waitForTimeout(100);
  await page.waitForTimeout(1_000);
  if (![...kit.keys()].some((k) => k.endsWith(".glb"))) fail("kit GLB never loaded");
  if (![...kit.keys()].some((k) => /\.(ktx2|png)$/.test(k))) fail("kit texture never loaded");
  for (const [name, v] of kit) console.log(`kit load: ${name} ${v.bytes} B in ${v.ms} ms`);
  const info = await page.evaluate(async () => {
    const i = (await navigator.gpu.requestAdapter())?.info;
    return i ? [i.vendor, i.architecture, i.description].join(" | ") : null;
  });
  if (!info) fail("no WebGPU adapter in GPU run");
  else if (/swiftshader/i.test(info) && process.env.PW_GPU !== "swiftshader") fail("SwiftShader adapter in GPU run: " + info);
  else console.log("adapter:", info);
  await page.locator("canvas#hex-world").screenshot({ path: "e2e/test-results/smoke-single.png" });
});

// 2. Unknown scenario falls back to default without errors.
await withPage(gpuArgs, async (page, errors) => {
  await page.goto(base + "?shot=nope");
  await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 60_000 });
  const state = await page.evaluate(() => window.hexWorld.state());
  if (state.scenario !== "Single") fail("bad scenario did not fall back: " + JSON.stringify(state));
  if (errors.some((e) => /panic|unreachable/i.test(e))) fail("panic on bad scenario");
});

// 3. No GPU → unsupported page.
await withPage(["--disable-gpu"], async (page) => {
  await page.goto(base);
  await page.waitForURL(/unsupported\.html$/, { timeout: 15_000 }).catch(() => fail("no redirect without GPU"));
  if (!(await page.locator("#unsupported").count())) fail("unsupported page content missing");
});

// 4. Forced WGSL compat shim (web/wgsl-compat.js, the Safari path): Tint must accept every rewritten module.
// Exact alias counts pin Bevy 0.20's PBR fragment shader (8 handle lets); a Bevy bump that changes them must
// be re-checked in Safari.
for (const [query, aliases] of [["?wgsl_compat=1", 8],
  ["?wgsl_compat=1&scenario=gate2000&view=closeup&layout=instanced", 8]]) {
  await withPage(gpuArgs, async (page) => {
    await page.goto(base + query);
    await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 90_000 });
    const compat = await page.evaluate(() => window.__wgslCompat);
    if (!compat?.active || compat.aliases !== aliases)
      fail(`wgsl-compat ${query}: expected ${aliases} aliases, got ` + JSON.stringify(compat));
    else console.log(`wgsl-compat ${query}: ${compat.rewritten} module(s), ${compat.aliases} aliases, ${compat.ms.toFixed(1)} ms`);
    if (compat?.unhandled) fail(`wgsl-compat ${query} left handle lets unrewritten: ` + compat.unhandled);
  });
}

// 5. Kit scenario with the gate fixture: KitMaterial (StandardMaterial + the masks texture) must
// specialize and draw on WebGPU without a validation error: on the adapter's own limits, with the
// device clamped to WebGPU's default 16 sampled textures per stage (the limit many adapters report;
// Bevy otherwise requests the adapter's, e.g. 48 here), and through the forced WGSL compat shim.
// `kit_src=fixture_tiled` binds the cap to a probe tileable set, so the tileable branches of the
// forward shader and the kit prepass (normal prepass, read by SSAO under post=full) run too.
// Every run must create both kit pipelines (forward and prepass, identified by their WGSL).
// `ready()` fires before the kit loads, so wait for the spawn log, then for the shim's module count
// to settle (pipelines compile asynchronously). The `ibl=off` case pins the layout without the
// environment cubemaps; IBL-on cases must fetch both RGBA16F KTX2 cubemaps with status 200.
for (const [query, aliases, clamp] of [["?scenario=kit&kit_src=fixture", null, null],
  ["?scenario=kit&kit_src=fixture", null, WEBGPU_DEFAULT_SAMPLED],
  ["?wgsl_compat=1&scenario=kit&kit_src=fixture", KIT_COMPAT_ALIASES, null],
  ["?scenario=kit&kit_src=fixture_tiled&post=full", null, WEBGPU_DEFAULT_SAMPLED],
  ["?wgsl_compat=1&scenario=kit&kit_src=fixture_tiled&post=full", KIT_COMPAT_ALIASES, null],
  ["?scenario=kit&kit_src=fixture_tiled&post=full&ibl=off", null, WEBGPU_DEFAULT_SAMPLED],
  ["?scenario=kit&kit_src=fixture_tiled&post=full&tilt=off", null, WEBGPU_DEFAULT_SAMPLED],
  // The runtime kit: meadow, roads and the hero oak (UV-mapped tileable bark slot with tileUvScale,
  // the unique canopy-shell slot, the alpha-masked double-sided leaves slot, and the TEXCOORD_1 wind
  // attribute) under the default 16 clamp (D7).
  ["?scenario=kit&post=full", null, WEBGPU_DEFAULT_SAMPLED]]) {
  const ibl = !query.includes("ibl=off");
  // The kit scenario turns the tilt-shift pass on by default (High tier, zoomed out); `tilt=off` must not create it.
  const tiltOn = !query.includes("tilt=off");
  const want = ibl ? KIT_SAMPLED.ibl : KIT_SAMPLED.noIbl;
  await withPage(gpuArgs, async (page, errors) => {
    let spawned = null;
    const iblStatus = new Map();
    page.on("response", (r) => {
      const m = /\/ibl\/(noon\.(?:specular|diffuse)\.ktx2)$/.exec(r.url());
      if (m) iblStatus.set(m[1], r.status());
    });
    page.on("console", (m) => {
      const s = /kit: spawned (\d+) meshes/.exec(m.text());
      if (s) spawned = Number(s[1]);
    });
    if (clamp !== null) await page.addInitScript(clampSampledTextures, clamp);
    await page.addInitScript(countSampledTextures);
    if (aliases !== null) await page.addInitScript(markedModuleProbe, "masks_texture");
    await page.goto(base + query);
    await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 90_000 });
    const deadline = Date.now() + 30_000;
    while (spawned === null && Date.now() < deadline) await page.waitForTimeout(100);
    if (!spawned) { fail(`kit ${query}: no kit meshes spawned (${spawned})`); return; }
    let last = -1, stable = 0;
    while (stable < 20 && Date.now() < deadline + 30_000) {
      const n = await page.evaluate(() => window.__wgslCompat?.rewritten ?? 0);
      stable = n === last ? stable + 1 : 0;
      last = n;
      await page.waitForTimeout(100);
    }
    await page.waitForTimeout(1_000);
    // IBL cases: wait (bounded) for the forward pipeline composed with the environment map. A forward
    // pipeline specialized before the cubemaps loaded (no ENVIRONMENT_MAP, 14 textures) is transient
    // and is not counted; any pipeline composed with the IBL must still match exactly.
    while (ibl && Date.now() < deadline + 60_000 && !(await page.evaluate(() =>
      (window.__sampledTextures?.kit ?? []).some((k) => k.pass === "forward" && k.ibl)))) await page.waitForTimeout(200);
    // Tilt-shift builds its pipeline once the shader loads and a frame runs with the camera component.
    while (tiltOn && Date.now() < deadline + 60_000 && !(await page.evaluate(() => window.__sampledTextures?.tilt?.length))) await page.waitForTimeout(200);
    const { compat, sampled, limit, marked, gpu } = await page.evaluate(() => ({
      compat: window.__wgslCompat, sampled: window.__sampledTextures, limit: window.__deviceSampledLimit,
      marked: window.__markedModules, gpu: window.hexWorld.state().gpu }));
    const tag = `kit ${query}${clamp !== null ? ` (clamped ${clamp})` : ""}`;
    console.log(`${tag}: ${spawned} meshes; sampled textures per stage max ${sampled?.max}, kit pipelines ` +
      `${JSON.stringify(sampled?.kit)}; state.gpu ${JSON.stringify(gpu)}; compat ` + JSON.stringify(compat));
    for (const pass of ["forward", "prepass"]) {
      const all = (sampled?.kit ?? []).filter((k) => k.pass === pass);
      const preIbl = ibl && pass === "forward" ? all.filter((k) => !k.ibl) : [];
      if (preIbl.length) console.log(`${tag}: ignoring ${preIbl.length} forward pipeline(s) compiled before the IBL loaded: ${JSON.stringify(preIbl)}`);
      if (!ibl && all.some((k) => k.ibl)) fail(`${tag}: ibl=off composed a kit ${pass} shader with the environment map`);
      const got = all.filter((k) => !preIbl.includes(k)).map((k) => k.sampled);
      if (!got.length) fail(`${tag}: no kit ${pass} pipeline was created`);
      else if (!got.every((n) => n === want[pass])) fail(`${tag}: kit ${pass} pipeline uses ${got} sampled textures, expected ${want[pass]}`);
    }
    const tilts = sampled?.tilt ?? [];
    if (tiltOn && !tilts.length) fail(`${tag}: no tilt-shift pipeline was created`);
    else if (tiltOn && !tilts.every((t) => t.sampled === TILT_SAMPLED))
      fail(`${tag}: tilt-shift pipeline uses ${tilts.map((t) => t.sampled)} sampled textures, expected ${TILT_SAMPLED}`);
    else if (!tiltOn && tilts.length) fail(`${tag}: tilt=off still created a tilt-shift pipeline: ${JSON.stringify(tilts)}`);
    else console.log(`${tag}: tilt-shift ${tiltOn ? `pipeline sampled textures ${JSON.stringify(tilts)}` : "off, no pipeline"}`);
    const iblSeen = JSON.stringify(Object.fromEntries(iblStatus));
    if (ibl && !(iblStatus.get("noon.specular.ktx2") === 200 && iblStatus.get("noon.diffuse.ktx2") === 200))
      fail(`${tag}: IBL cubemaps not both loaded with 200: ${iblSeen}`);
    else if (!ibl && iblStatus.size) fail(`${tag}: ibl=off still fetched ${iblSeen}`);
    else console.log(`${tag}: ibl ${ibl ? "on" : "off"}, cubemap responses ${iblSeen}`);
    if (clamp !== null) {
      if (limit !== clamp) fail(`${tag}: device limit ${limit}, expected ${clamp}`);
      if (gpu?.maxSampledTexturesPerShaderStage !== clamp)
        fail(`${tag}: hexWorld.state().gpu does not report the device limit: ` + JSON.stringify(gpu));
      if (!(sampled?.max <= clamp)) fail(`${tag}: a pipeline layout uses ${sampled?.max} sampled textures`);
    }
    if (aliases !== null) {
      if (!compat?.active || compat.aliases !== aliases)
        fail(`wgsl-compat ${query}: expected ${aliases} aliases, got ` + JSON.stringify(compat));
      if (compat?.unhandled) fail(`wgsl-compat ${query} left handle lets unrewritten: ` + compat.unhandled);
      if (!marked?.some((m) => m.before > 0 && m.after === 0))
        fail(`wgsl-compat ${query}: the kit fragment (masks_texture) was not rewritten: ` + JSON.stringify(marked));
      else console.log(`${tag}: kit fragment rewritten ${JSON.stringify(marked)}`);
    }
    if (query === "?scenario=kit&kit_src=fixture" && clamp === null)
      await page.locator("canvas#hex-world").screenshot({ path: "e2e/test-results/smoke-kit.png" });
  });
}

server.close();
console.log(process.exitCode ? "smoke FAILED" : "smoke OK");
