// Local-only browser smoke test: GPU Chromium, WebGPU backend, gate redirect, bad scenario fallback.
import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join } from "node:path";
import { BAD_TEXT } from "./console-errors.mjs";
import { TRUNK_HASHED, contentHash } from "../scripts/web/version-dist.mjs";
import { clampSampledTextures, countSampledTextures, markedModuleProbe } from "./webgpu-probes.mjs";

const dist = process.argv[2] ?? "dist";
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

// --enable-unsafe-webgpu must stay out of the no-GPU run: with it, --disable-gpu still yields a SwiftShader adapter.
const gpuArgs = process.env.PW_GPU === "swiftshader"
  ? ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--enable-unsafe-webgpu"]
  : ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu"];
const fail = (m) => { console.error("FAIL:", m); process.exitCode = 1; };
// Cases whose expectations are written for High pin it: without a `tier` key the tier is
// auto-detected, and the web warm-up probe may step it down on a loaded host.
const pinHigh = (q) => (/[?&]tier=/.test(q) ? q : q + "&tier=high");
// Handle-let aliases the shim rewrites per KitMaterial fragment module (Bevy's PBR input composed in),
// asserted by its `masks_texture` global (markedModuleProbe). Usually one module; two when a pipeline
// compiled before the IBL loaded is specialized again after it.
const KIT_COMPAT_ALIASES = 8;
// WebGPU's default maxSampledTexturesPerShaderStage.
const WEBGPU_DEFAULT_SAMPLED = 16;
// Measured fragment-stage sampled textures of the kit pipelines with post=full. The forward count
// is asserted exactly, so the clamped case cannot pass without the kit pipeline. Forward: the view
// bind group (point and directional shadow maps, SSAO, depth/normal/motion-vector prepass, plus the
// two noon environment cubemaps with `ibl=on`) and the material (StandardMaterial's six slots and
// `masks`). Bevy's `bluenoise_texture` feature is off (crates/hex-render/Cargo.toml), so the view
// binds no blue-noise texture: 15 with the IBL, one below WebGPU's default 16.
// The tilt-shift pass samples the colour target and the prepass depth.
const TILT_SAMPLED = 2;
const KIT_SAMPLED = { ibl: { forward: 15, prepass: 7 }, noIbl: { forward: 13, prepass: 7 } };
// A camera with screen-space transmission (refracting water, High and Medium) adds the view
// transmission texture to every mesh pipeline in its view, the kit's included: exactly 16 at High.
const KIT_SAMPLED_WITH_WATER = { forward: KIT_SAMPLED.ibl.forward + 1, prepass: KIT_SAMPLED.ibl.prepass };
// Measured fragment-stage sampled textures of the water scenario's pipelines with post=full and the
// IBL. The view layout sets them: High adds SSAO (its texture and the normal prepass), refracting
// tiers (High, Medium) add the transmission texture; each material binds one texture.
// Measured fragment-stage sampled textures of the grass pipelines in play beside refracting water: the
// view's textures alone (the material binds none), below the kit forward's 16.
const GRASS_SAMPLED_WITH_WATER = { forward: 9, prepass: 0 };
const WATER_SAMPLED = { high: { water: 10, terrain: 10 }, medium: { water: 8, terrain: 8 }, low: { water: 7, terrain: 7 } };
// Handle-let aliases per rewritten module in a water scenario run: neither water shader composes
// StandardMaterial's PBR input (the marker probe checks the water module keeps none), but the boid
// material's fragment does, as the kit fragment does.
const WATER_COMPAT_ALIASES = KIT_COMPAT_ALIASES;

// The boid material (crates/hex-render/src/boids.rs: StandardMaterial + `masks`, the boid buffer a
// vertex-stage storage binding) draws in every scenario. Its forward pipeline samples exactly what the
// kit forward pipeline does in the same view: 15 with the IBL, 13 without, 16 beside refracting water.
async function waitBoids(page, ibl, deadline) {
  while (Date.now() < deadline && !(await page.evaluate((ibl) =>
    (window.__sampledTextures?.boid ?? []).some((b) => b.ibl === ibl), ibl))) await page.waitForTimeout(200);
}
function checkBoids(tag, sampled, ibl, want, transmission) {
  const got = (sampled?.boid ?? []).filter((b) => b.ibl === ibl);
  if (!got.length) fail(`${tag}: no boid pipeline (ibl ${ibl}) was created: ${JSON.stringify(sampled?.boid)}`);
  else if (!got.every((b) => b.sampled === want))
    fail(`${tag}: boid pipeline uses ${got.map((b) => b.sampled)} sampled textures, expected ${want}`);
  else if (!got.every((b) => b.transmission === transmission))
    fail(`${tag}: boid pipeline transmission binding ${JSON.stringify(got)}, expected ${transmission}`);
  else console.log(`${tag}: boid pipelines ${JSON.stringify(sampled.boid)}`);
}

// `allow(message)` exempts console messages a case provokes on purpose (its injected request failures).
async function withPage(args, fn, allow = () => false) {
  const browser = await chromium.launch({ channel: "chromium", headless: true, args });
  const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const no = ++caseNo;
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  // Bevy's wasm logger prints ERROR through console.log, so match the text, not only the console type.
  page.on("console", (m) => { if ((m.type() === "error" || BAD_TEXT.test(m.text())) && !allow(m)) errors.push(m.text()); });
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

// 1. The Single scenario becomes ready on the WebGPU backend.
await withPage(gpuArgs, async (page, errors) => {
  const kit = new Map();
  page.on("console", (m) => { if (/texture family/i.test(m.text())) console.log("app:", m.text()); });
  page.on("requestfinished", async (r) => {
    const path = new URL(r.url()).pathname;
    if (!path.includes("/assets/kit/")) return;
    const t = r.timing(), size = (await r.sizes()).responseBodySize;
    kit.set(path.split("/").pop(), { ms: Math.round(t.responseEnd), bytes: size });
  });
  await page.goto(base + "?scenario=single");
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
  // The HUD overlay (web/ui): its loading layer clears and no play HUD covers a non-play scenario,
  // so canvas-locator screenshots here and in the perf scripts stay the engine's pixels.
  const hud = await page.evaluate(() => ({ load: !!document.querySelector(".load"), off: !!document.querySelector("#hud.is-off") }));
  if (hud.load || !hud.off) fail("HUD overlay left a layer over the Single scenario: " + JSON.stringify(hud));
  await page.locator("canvas#hex-world").screenshot({ path: "e2e/test-results/smoke-single.png" });
});

// 2. The bare URL opens the game: the play scenario with the Legend HUD over it (web default).
await withPage(gpuArgs, async (page) => {
  await page.goto(base);
  await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 90_000 });
  const state = await page.evaluate(() => window.hexWorld.state());
  if (state.scenario !== "Play") fail("bare URL did not open the play scenario: " + JSON.stringify(state.scenario));
  // The web frame-latency limit is on by default (web/frame-latency.js).
  const fl = await page.evaluate(() => window.__frameLatency && { limit: window.__frameLatency.limit, source: window.__frameLatency.source, deferrals: window.__frameLatency.deferrals() });
  if (fl?.limit !== 3 || fl?.source !== "default") fail("bare URL: frame-latency limit not on by default: " + JSON.stringify(fl));
  else console.log("frame-latency (bare URL):", JSON.stringify(fl));
  await page.waitForSelector("#hud:not(.is-off) .bar", { state: "visible", timeout: 15_000 })
    .catch(() => fail("bare URL: the HUD tool bar never showed"));
  await page.waitForSelector(".sheet--tiles:not(.is-closed)", { state: "visible", timeout: 15_000 })
    .catch(() => fail("bare URL: the Tiles sheet is not open by default"));
  await page.screenshot({ path: "e2e/test-results/smoke-default-play.png" });
});

// 2b. Unknown scenario falls back to the web default (play) without errors; the URL sets the
// frame-latency limit.
await withPage(gpuArgs, async (page, errors) => {
  await page.goto(base + "?shot=nope&frame_latency=2");
  await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 90_000 });
  const state = await page.evaluate(() => window.hexWorld.state());
  if (state.scenario !== "Play") fail("bad scenario did not fall back: " + JSON.stringify(state.scenario));
  const fl = await page.evaluate(() => window.__frameLatency && { limit: window.__frameLatency.limit, source: window.__frameLatency.source });
  if (fl?.limit !== 2 || fl?.source !== "url") fail("?frame_latency=2 did not set the limit: " + JSON.stringify(fl));
  if (errors.some((e) => /panic|unreachable/i.test(e))) fail("panic on bad scenario");
});

// 2b2. `?frame_latency=off` turns the default limit off: nothing is patched.
await withPage(gpuArgs, async (page) => {
  await page.goto(base + "?scenario=single&frame_latency=off");
  await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 60_000 });
  const fl = await page.evaluate(() => window.__frameLatency ?? null);
  if (fl !== null) fail("?frame_latency=off still patched the frame loop: " + JSON.stringify(fl));
});

// 2b3. Frame rate cap (owner A29): `?frame_cap=` and `set_frame_cap` reach `state().frame_cap` and
// the page's animation-frame gate (`window.__frameCap`), a 60 cap holds the engine at or under 60
// frames a second, a refused value is skipped, and Off hands the frame loop back. Only the upper
// bound is asserted: the lower one depends on host load (e2e/frame-pacing.mjs measures both).
await withPage(gpuArgs, async (page) => {
  await page.goto(base + "?scenario=single&frame_cap=30");
  await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 60_000 });
  const cap = () => page.evaluate(() => ({ state: window.hexWorld.state().frame_cap, gate: window.__frameCap?.fps() }));
  const until = (fps) => page.waitForFunction((fps) => window.hexWorld.state().frame_cap?.fps === fps
    && window.__frameCap?.fps() === (fps ?? 0), fps, { timeout: 10_000 }).then(() => true, () => false);
  if (!(await until(30))) fail("?frame_cap=30 did not reach state and the gate: " + JSON.stringify(await cap()));
  const send = (fps) => page.evaluate((fps) => window.hexWorld.cmd(JSON.stringify({ cmd: "set_frame_cap", fps })), fps);
  await send(60);
  if (!(await until(60))) fail("set_frame_cap 60: " + JSON.stringify(await cap()));
  const rate = await page.evaluate(() => new Promise((resolve) => {
    const f0 = window.hexWorld.state().frames, t0 = performance.now();
    setTimeout(() => resolve((window.hexWorld.state().frames - f0) / ((performance.now() - t0) / 1000)), 2000);
  }));
  if (!(rate <= 66)) fail(`60 fps cap: the engine drew ${rate.toFixed(1)} frames a second`);
  else console.log(`frame cap 60: ${rate.toFixed(1)} engine frames a second (upper bound asserted)`);
  await send(75);
  await page.waitForTimeout(500);
  if ((await cap()).state?.fps !== 60) fail("set_frame_cap 75 was not skipped: " + JSON.stringify(await cap()));
  await send(null);
  if (!(await until(null))) fail("set_frame_cap null (Off): " + JSON.stringify(await cap()));
});

// 2c. Quality tier (PRD §9.2): with no `tier` key it is detected from the adapter's limits and
// features plus the warm-up probe, and published with its reason; `set_tier` changes it at runtime.
// The detected value depends on the machine and its load, so only the shape is asserted.
// `gpu_timing=on` publishes per-pass GPU times when the adapter has timestamp-query.
await withPage(gpuArgs, async (page, errors) => {
  await page.goto(base + "?scenario=single&gpu_timing=on");
  await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 90_000 });
  const t = (await page.evaluate(() => window.hexWorld.state())).tier;
  const tiers = ["low", "medium", "high"];
  if (!t || !tiers.includes(t.tier) || !["detected", "probe"].includes(t.source) || !t.reason
    || !tiers.includes(t.detected?.tier) || t.adapter?.web !== true || typeof t.probe?.median_ms !== "number")
    fail("auto tier state: " + JSON.stringify(t));
  console.log(`tier: ${t?.tier} (${t?.source}): ${t?.reason}; adapter ${JSON.stringify(t?.adapter)}`);
  const gpuOk = await page.waitForFunction(() => {
    const g = window.hexWorld.diag().gpu;
    return g && (g.supported === false || g.frames >= 30);
  }, null, { timeout: 30_000, polling: 250 }).then(() => true, () => false);
  const d = await page.evaluate(() => window.hexWorld.diag());
  if (!gpuOk || d.gpu?.enabled !== true) fail("gpu_timing diag: " + JSON.stringify(d.gpu));
  else if (d.gpu.supported && !(d.gpu.passes?.opaque && d.gpu.total?.p50 >= 0)) fail("gpu passes missing: " + JSON.stringify(d.gpu));
  else console.log(`gpu_timing: supported=${d.gpu.supported} frames=${d.gpu.frames} rejected=${d.gpu.rejected_non_monotonic} ` +
    `total=${JSON.stringify(d.gpu.total)} (unguarded functional run, not a measurement)`);
  if (d.stall?.histogram?.length !== 8) fail("stall histogram: " + JSON.stringify(d.stall));
  await page.evaluate(() => window.hexWorld.cmd(JSON.stringify({ cmd: "set_tier", tier: "low" })));
  const lowOk = await page.waitForFunction(() => {
    const s = window.hexWorld.state();
    return s.tier?.tier === "low" && s.tier?.source === "runtime" && !(s.diag?.post ?? []).includes("ssao");
  }, null, { timeout: 30_000, polling: 250 }).then(() => true, () => false);
  if (!lowOk) fail("set_tier low: " + JSON.stringify((await page.evaluate(() => window.hexWorld.state())).tier));
  if (errors.some((e) => /panic|unreachable/i.test(e))) fail("panic around set_tier: " + errors.join(" | "));
});

// 2d. An explicit `tier` key wins over detection; detection is still reported.
await withPage(gpuArgs, async (page) => {
  await page.goto(base + "?scenario=single&tier=medium");
  await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 90_000 });
  const t = (await page.evaluate(() => window.hexWorld.state())).tier;
  if (t?.tier !== "medium" || t?.source !== "explicit" || !t?.detected?.tier || t?.probe !== null)
    fail("explicit tier state: " + JSON.stringify(t));
});

// 3. No GPU → unsupported page.
await withPage(["--disable-gpu"], async (page) => {
  await page.goto(base);
  await page.waitForURL(/unsupported\.html(\?v=[0-9a-f]+)?$/, { timeout: 15_000 }).catch(() => fail("no redirect without GPU"));
  if (!(await page.locator("#unsupported").count())) fail("unsupported page content missing");
  // The Legend S11 page: one visible headline, the app icon and Manrope loaded under the page CSP.
  await page.waitForLoadState("networkidle");
  const s11 = await page.evaluate(async () => {
    await document.fonts.ready;
    const h1 = [...document.querySelectorAll("h1")].filter((h) => h.offsetParent);
    const icon = document.querySelector(".card__icon");
    return { h1: h1.map((h) => h.textContent), icon: !!icon?.naturalWidth, font: document.fonts.check("700 16px Manrope") };
  });
  if (s11.h1.length !== 1 || !s11.icon || !s11.font) fail("unsupported page not rendered: " + JSON.stringify(s11));
});

// 4. Forced WGSL compat shim (web/wgsl-compat.js, the Safari path): Tint must accept every rewritten module.
// Exact per-module alias counts pin Bevy 0.20's PBR fragment shader (8 handle lets); a Bevy bump that changes
// them must be re-checked in Safari. A pipeline compiled before the IBL loads is specialized again after it,
// so the same fragment can be rewritten twice under host load: the shim records each rewritten module's
// alias count (perModule), and every one must match exactly, however many modules a run rewrites.
const compatOk = (c, perModule) => c?.active && c.rewritten >= 1 && c.perModule?.length === c.rewritten &&
  c.perModule.every((n) => n === perModule) && c.aliases === perModule * c.rewritten;
for (const [query, aliases] of [["?wgsl_compat=1&scenario=single", 8],
  ["?wgsl_compat=1&scenario=gate2000&view=closeup&layout=instanced", 8]]) {
  await withPage(gpuArgs, async (page) => {
    await page.goto(base + query);
    await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 90_000 });
    const compat = await page.evaluate(() => window.__wgslCompat);
    if (!compatOk(compat, aliases))
      fail(`wgsl-compat ${query}: expected ${aliases} aliases per rewritten module, got ` + JSON.stringify(compat));
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
  // Tilt-shift is off by default (owner, 2026-10-14), so every case but the `tilt=off` one asks for it
  // with `tilt=on` to keep its pipeline under the clamp; `tilt=off` must not create it.
  const tiltOn = !query.includes("tilt=off");
  const nav = tiltOn ? `${query}&tilt=on` : query;
  const want = ibl ? KIT_SAMPLED.ibl : KIT_SAMPLED.noIbl;
  await withPage(gpuArgs, async (page, errors) => {
    let spawned = null;
    const iblStatus = new Map();
    page.on("response", (r) => {
      const m = /\/ibl\/(noon\.(?:specular|diffuse)\.ktx2)$/.exec(new URL(r.url()).pathname);
      if (m) iblStatus.set(m[1], r.status());
    });
    page.on("console", (m) => {
      const s = /kit: spawned (\d+) meshes/.exec(m.text());
      if (s) spawned = Number(s[1]);
    });
    if (clamp !== null) await page.addInitScript(clampSampledTextures, clamp);
    await page.addInitScript(countSampledTextures);
    if (aliases !== null) await page.addInitScript(markedModuleProbe, "masks_texture");
    await page.goto(base + pinHigh(nav));
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
    await waitBoids(page, ibl, deadline + 60_000);
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
    checkBoids(tag, sampled, ibl, want.forward, false);
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
      if (!compatOk(compat, aliases))
        fail(`wgsl-compat ${query}: expected ${aliases} aliases per rewritten module, got ` + JSON.stringify(compat));
      if (compat?.unhandled) fail(`wgsl-compat ${query} left handle lets unrewritten: ` + compat.unhandled);
      if (!marked?.some((m) => m.before > 0 && m.after === 0))
        fail(`wgsl-compat ${query}: the kit fragment (masks_texture) was not rewritten: ` + JSON.stringify(marked));
      else console.log(`${tag}: kit fragment rewritten ${JSON.stringify(marked)}`);
    }
    if (query === "?scenario=kit&kit_src=fixture" && clamp === null)
      await page.locator("canvas#hex-world").screenshot({ path: "e2e/test-results/smoke-kit.png" });
  });
}

// 6. Water scenario (PRD §9.1 hero water): the water and terrain pipelines must specialize and draw on
// WebGPU with the device clamped to the default 16 sampled textures per stage, at High (refraction:
// the view gains the transmission texture), Medium and Low (no transmission), and through the forced
// WGSL compat shim. Counts are pinned exactly (fragment stage, post=full, IBL on), so a new binding
// that eats the headroom fails here before it fails on a 16-texture adapter.
for (const [query, aliases, clamp] of [["?scenario=water&post=full", null, WEBGPU_DEFAULT_SAMPLED],
  ["?scenario=water&post=full&tier=medium", null, WEBGPU_DEFAULT_SAMPLED],
  ["?scenario=water&post=full&tier=low", null, WEBGPU_DEFAULT_SAMPLED],
  ["?wgsl_compat=1&scenario=water&post=full", WATER_COMPAT_ALIASES, null]]) {
  const tier = /tier=(\w+)/.exec(query)?.[1] ?? "high";
  const refraction = tier !== "low";
  const want = WATER_SAMPLED[tier];
  await withPage(gpuArgs, async (page) => {
    if (clamp !== null) await page.addInitScript(clampSampledTextures, clamp);
    await page.addInitScript(countSampledTextures);
    if (aliases !== null) await page.addInitScript(markedModuleProbe, "water_noise_texture");
    await page.goto(base + pinHigh(query));
    await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 90_000 });
    // Wait (bounded) for both pipelines composed with the IBL, then let compilation settle.
    const deadline = Date.now() + 60_000;
    while (Date.now() < deadline && !(await page.evaluate(() => {
      const w = window.__sampledTextures?.water ?? [];
      return w.some((k) => k.pass === "water" && k.ibl) && w.some((k) => k.pass === "terrain" && k.ibl);
    }))) await page.waitForTimeout(200);
    await page.waitForTimeout(1_500);
    const { compat, sampled, limit, marked, state } = await page.evaluate(() => ({
      compat: window.__wgslCompat, sampled: window.__sampledTextures, limit: window.__deviceSampledLimit,
      marked: window.__markedModules, state: window.hexWorld.state() }));
    const tag = `water ${query}${clamp !== null ? ` (clamped ${clamp})` : ""}`;
    console.log(`${tag}: sampled textures per stage max ${sampled?.max}, water pipelines ${JSON.stringify(sampled?.water)}; ` +
      `tiles ${state.tiles}; compat ${JSON.stringify(compat)}`);
    if (state.scenario !== "Water" || state.tiles !== 12) fail(`${tag}: state ${JSON.stringify(state)}`);
    for (const pass of ["water", "terrain"]) {
      const got = (sampled?.water ?? []).filter((k) => k.pass === pass && k.ibl);
      if (!got.length) fail(`${tag}: no IBL ${pass} pipeline was created`);
      else if (!got.every((k) => k.sampled === want[pass])) fail(`${tag}: ${pass} pipeline uses ${got.map((k) => k.sampled)} sampled textures, expected ${want[pass]}`);
      // The terrain shader never reads the transmission texture (its layout still counts it).
      if (pass === "water" && got.some((k) => k.transmission !== refraction))
        fail(`${tag}: water shader transmission binding ${JSON.stringify(got)}, expected ${refraction}`);
    }
    if (clamp !== null) {
      if (limit !== clamp) fail(`${tag}: device limit ${limit}, expected ${clamp}`);
      if (!(sampled?.max <= clamp)) fail(`${tag}: a pipeline layout uses ${sampled?.max} sampled textures`);
    }
    if (aliases !== null) {
      if (!compatOk(compat, aliases))
        fail(`wgsl-compat ${query}: expected ${aliases} aliases per rewritten module, got ` + JSON.stringify(compat));
      if (compat?.unhandled) fail(`wgsl-compat ${query} left handle lets unrewritten: ` + compat.unhandled);
      if (!marked?.length || !marked.every((m) => m.after === 0))
        fail(`wgsl-compat ${query}: the water module kept handle aliases: ` + JSON.stringify(marked));
      else console.log(`${tag}: water module through the shim ${JSON.stringify(marked)}`);
    }
    if (query === "?scenario=water&post=full")
      await page.locator("canvas#hex-world").screenshot({ path: "e2e/test-results/smoke-water.png" });
  });
}

// 7. Water and kit pieces in one view (PRD §15 P2, ADR 0002 "Water and the texture limit"):
// `scenario=water_kit` spawns the water patch and the kit under the water camera. At High the camera
// has screen-space transmission, so the kit forward pipeline binds the view transmission texture on
// top of the IBL. With the device clamped to WebGPU's default 16 it must specialize and draw with no
// validation error; its count is pinned exactly and its layout must bind the transmission texture.
for (const query of ["?scenario=water_kit&kit_src=fixture_tiled&post=full", "?scenario=water_kit&post=full"]) {
  const clamp = WEBGPU_DEFAULT_SAMPLED;
  await withPage(gpuArgs, async (page) => {
    let spawned = null;
    page.on("console", (m) => {
      const s = /kit: spawned (\d+) meshes/.exec(m.text());
      if (s) spawned = Number(s[1]);
    });
    await page.addInitScript(clampSampledTextures, clamp);
    await page.addInitScript(countSampledTextures);
    await page.goto(base + pinHigh(query));
    const tag = `water_kit ${query} (clamped ${clamp})`;
    // A pipeline layout over the limit is a device validation error that stops the app before READY:
    // report what the probe saw instead of throwing.
    if (!(await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 90_000 }).then(() => true, () => false))) {
      const s = await page.evaluate(() => window.__sampledTextures);
      fail(`${tag}: never ready; sampled textures per stage max ${s?.max}, kit pipelines ${JSON.stringify(s?.kit)}`);
      return;
    }
    const deadline = Date.now() + 60_000;
    while (spawned === null && Date.now() < deadline) await page.waitForTimeout(100);
    if (!spawned) { fail(`${tag}: no kit meshes spawned (${spawned})`); return; }
    // Wait (bounded) for the IBL-composed kit forward, water and terrain pipelines, then let compilation settle.
    while (Date.now() < deadline + 30_000 && !(await page.evaluate(() => {
      const s = window.__sampledTextures ?? {};
      return (s.kit ?? []).some((k) => k.pass === "forward" && k.ibl) && (s.kit ?? []).some((k) => k.pass === "prepass") &&
        (s.water ?? []).some((k) => k.pass === "water" && k.ibl) && (s.water ?? []).some((k) => k.pass === "terrain" && k.ibl);
    }))) await page.waitForTimeout(200);
    await waitBoids(page, true, deadline + 60_000);
    await page.waitForTimeout(1_500);
    const { sampled, limit, state } = await page.evaluate(() => ({
      sampled: window.__sampledTextures, limit: window.__deviceSampledLimit, state: window.hexWorld.state() }));
    checkBoids(tag, sampled, true, KIT_SAMPLED_WITH_WATER.forward, true);
    console.log(`${tag}: ${spawned} meshes; sampled textures per stage max ${sampled?.max}, kit pipelines ` +
      `${JSON.stringify(sampled?.kit)}, water pipelines ${JSON.stringify(sampled?.water)}; state.gpu ${JSON.stringify(state.gpu)}`);
    // The fixture lays out the water patch's 12 cells; the runtime kit adds its showcase blocks, whose
    // exact count `tile_counts` in crates/hex-render/src/lib.rs pins (it grows with every block).
    const tilesOk = query.includes("kit_src=fixture") ? state.tiles === 12 : state.tiles > 12;
    if (state.scenario !== "WaterKit" || !tilesOk) fail(`${tag}: state scenario ${state.scenario}, tiles ${state.tiles}`);
    for (const pass of ["forward", "prepass"]) {
      const got = (sampled?.kit ?? []).filter((k) => k.pass === pass && (pass === "prepass" || k.ibl));
      if (!got.length) fail(`${tag}: no kit ${pass} pipeline was created`);
      else if (!got.every((k) => k.sampled === KIT_SAMPLED_WITH_WATER[pass]))
        fail(`${tag}: kit ${pass} pipeline uses ${got.map((k) => k.sampled)} sampled textures, expected ${KIT_SAMPLED_WITH_WATER[pass]}`);
      if (pass === "forward" && !got.every((k) => k.transmission))
        fail(`${tag}: kit forward pipeline layout lacks the view transmission texture: ${JSON.stringify(got)}`);
    }
    for (const pass of ["water", "terrain"]) {
      const got = (sampled?.water ?? []).filter((k) => k.pass === pass && k.ibl);
      if (!got.length) fail(`${tag}: no IBL ${pass} pipeline was created`);
      else if (!got.every((k) => k.sampled === WATER_SAMPLED.high[pass]))
        fail(`${tag}: ${pass} pipeline uses ${got.map((k) => k.sampled)} sampled textures, expected ${WATER_SAMPLED.high[pass]}`);
    }
    if (limit !== clamp) fail(`${tag}: device limit ${limit}, expected ${clamp}`);
    if (state.gpu?.maxSampledTexturesPerShaderStage !== clamp) fail(`${tag}: hexWorld.state().gpu ${JSON.stringify(state.gpu)}`);
    if (!(sampled?.max <= clamp)) fail(`${tag}: a pipeline layout uses ${sampled?.max} sampled textures`);
    if (query === "?scenario=water_kit&post=full")
      await page.locator("canvas#hex-world").screenshot({ path: "e2e/test-results/smoke-water-kit.png" });
  });
}

// 8. Water tiles in play (P2 W3c, crates/hex-render/src/water/tiles.rs): the play camera always
// carries the water camera components (depth prepass, screen-space transmission at High), and placed
// river, beach and ocean tiles draw with the water and terrain materials next to the kit pieces. With
// the device clamped to WebGPU's default 16 the kit forward pipeline must bind the transmission
// texture at exactly 16, and the water pipelines must match the water scenario's High counts.
{
  const clamp = WEBGPU_DEFAULT_SAMPLED;
  const query = "?scenario=play&post=full";
  await withPage(gpuArgs, async (page) => {
    await page.addInitScript(clampSampledTextures, clamp);
    await page.addInitScript(countSampledTextures);
    await page.goto(base + pinHigh(query));
    const tag = `play water ${query} (clamped ${clamp})`;
    if (!(await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 90_000 }).then(() => true, () => false))) {
      const s = await page.evaluate(() => window.__sampledTextures);
      fail(`${tag}: never ready; sampled textures per stage max ${s?.max}, kit pipelines ${JSON.stringify(s?.kit)}`);
      return;
    }
    // The first tiles of docs/checkpoints/water-play/water-play.json: a river straight and bend west
    // of the start meadow, a beach and an ocean east of it.
    await page.evaluate(() => {
      const h = window.hexWorld;
      h.selectTile("river_straight"); h.placeTile(-1, 0);
      h.selectTile("river_bend"); h.placeTile(-1, 1);
      h.selectTile("beach"); h.placeTile(1, 0);
      h.selectTile("ocean"); h.placeTile(2, 0);
    });
    const deadline = Date.now() + 90_000;
    while (Date.now() < deadline && !(await page.evaluate(() => {
      const s = window.__sampledTextures ?? {};
      return window.hexWorld.state().tiles === 5 &&
        (s.kit ?? []).some((k) => k.pass === "forward" && k.ibl && k.transmission) && (s.kit ?? []).some((k) => k.pass === "prepass") &&
        (s.water ?? []).some((k) => k.pass === "water" && k.ibl);
    }))) await page.waitForTimeout(200);
    await waitBoids(page, true, deadline + 30_000);
    await page.waitForTimeout(1_500);
    // The ghost's see-through meshes (GhostMaterial: one uniform, no texture) beside refracting water,
    // so their pipeline is created under the same clamp.
    await page.evaluate(() => { const h = window.hexWorld; h.selectTile("river_straight"); h.hover(-2, 0); });
    const ghostBy = Date.now() + 30_000;
    while (Date.now() < ghostBy && !(await page.evaluate(() => window.hexWorld.state().ghost_meshes === true)))
      await page.waitForTimeout(200);
    await page.waitForTimeout(1_000);
    const ghost = await page.evaluate(() => ({ meshes: window.hexWorld.state().ghost_meshes, at: window.hexWorld.state().play?.ghost }));
    console.log(`${tag}: ghost ${JSON.stringify(ghost)}`);
    if (ghost.meshes !== true || ghost.at?.q !== -2 || ghost.at?.r !== 0)
      fail(`${tag}: the ghost at (-2, 0) is not drawn with its meshes: ${JSON.stringify(ghost)}`);
    // Grass blades (crates/hex-render/src/play/grass.rs, one uniform and no texture) near a low camera
    // over the start meadow beside refracting water: their forward and prepass pipelines are created
    // under the same clamp and sample only the view's textures.
    await page.evaluate(() => window.hexWorld.cmd({ cmd: "camera", pos: [6, 6, 16], look_at: [0, 0, 0] }));
    const grassBy = Date.now() + 30_000;
    while (Date.now() < grassBy && !(await page.evaluate(() => {
      const g = window.hexWorld.state().grass;
      const s = window.__sampledTextures?.grass ?? [];
      return g?.groups > 0 && s.some((k) => k.pass === "forward" && k.ibl) && s.some((k) => k.pass === "prepass");
    }))) await page.waitForTimeout(200);
    await page.waitForTimeout(1_000);
    const { sampled, limit, state } = await page.evaluate(() => ({
      sampled: window.__sampledTextures, limit: window.__deviceSampledLimit, state: window.hexWorld.state() }));
    console.log(`${tag}: tiles ${state.tiles}; sampled textures per stage max ${sampled?.max}, kit pipelines ` +
      `${JSON.stringify(sampled?.kit)}, water pipelines ${JSON.stringify(sampled?.water)}; state.gpu ${JSON.stringify(state.gpu)}`);
    console.log(`${tag}: grass ${JSON.stringify({ status: state.grass?.status, groups: state.grass?.groups, blades: state.grass?.blades })}, ` +
      `grass pipelines ${JSON.stringify(sampled?.grass)}`);
    if (!(state.grass?.groups > 0)) fail(`${tag}: no grass mesh near the camera: ${JSON.stringify(state.grass)}`);
    for (const pass of ["forward", "prepass"]) {
      const got = (sampled?.grass ?? []).filter((k) => k.pass === pass && (pass === "prepass" || k.ibl));
      if (!got.length) fail(`${tag}: no grass ${pass} pipeline was created`);
      else if (!got.every((k) => k.sampled === GRASS_SAMPLED_WITH_WATER[pass]))
        fail(`${tag}: grass ${pass} pipeline uses ${got.map((k) => k.sampled)} sampled textures, expected ${GRASS_SAMPLED_WITH_WATER[pass]}`);
    }
    if (state.scenario !== "Play" || state.tiles !== 5) fail(`${tag}: state scenario ${state.scenario}, tiles ${state.tiles}`);
    // Birds over the start world and fish in the river and ocean tiles: the boid forward pipeline
    // beside refracting water binds the transmission texture, at exactly 16.
    checkBoids(tag, sampled, true, KIT_SAMPLED_WITH_WATER.forward, true);
    if (state.boids?.pieces !== "ready") fail(`${tag}: boid pieces ${JSON.stringify(state.boids?.pieces)}, expected ready`);
    for (const pass of ["forward", "prepass"]) {
      const got = (sampled?.kit ?? []).filter((k) => k.pass === pass && (pass === "prepass" || k.ibl));
      if (!got.length) fail(`${tag}: no kit ${pass} pipeline was created`);
      else if (!got.every((k) => k.sampled === KIT_SAMPLED_WITH_WATER[pass]))
        fail(`${tag}: kit ${pass} pipeline uses ${got.map((k) => k.sampled)} sampled textures, expected ${KIT_SAMPLED_WITH_WATER[pass]}`);
      if (pass === "forward" && !got.every((k) => k.transmission))
        fail(`${tag}: kit forward pipeline layout lacks the view transmission texture: ${JSON.stringify(got)}`);
    }
    // Play draws the terrain material only as the placeholder bed of a water tile whose centre
    // piece is not in the manifest. A tile waiting on the water pack is hidden (owner decision A10),
    // so with every centre piece shipped no terrain pipeline is created here; the water scenario
    // cases above cover the terrain pipeline's texture count.
    for (const pass of ["water", "terrain"]) {
      const got = (sampled?.water ?? []).filter((k) => k.pass === pass && k.ibl);
      if (!got.length) {
        if (pass === "water") fail(`${tag}: no IBL ${pass} pipeline was created`);
      } else if (!got.every((k) => k.sampled === WATER_SAMPLED.high[pass]))
        fail(`${tag}: ${pass} pipeline uses ${got.map((k) => k.sampled)} sampled textures, expected ${WATER_SAMPLED.high[pass]}`);
      if (pass === "water" && !got.every((k) => k.transmission))
        fail(`${tag}: water shader not composed with the transmission texture: ${JSON.stringify(got)}`);
    }
    if (limit !== clamp) fail(`${tag}: device limit ${limit}, expected ${clamp}`);
    if (state.gpu?.maxSampledTexturesPerShaderStage !== clamp) fail(`${tag}: hexWorld.state().gpu ${JSON.stringify(state.gpu)}`);
    if (!(sampled?.max <= clamp)) fail(`${tag}: a pipeline layout uses ${sampled?.max} sampled textures`);
    await page.locator("canvas#hex-world").screenshot({ path: "e2e/test-results/smoke-play-water.png" });
  });
}

// 9. Kit streaming by pack (PRD §10, crates/hex-play/src/play/packs.rs). Before the first
// interaction the play scenario may fetch only core files (the network log, mapped to packs through
// dist's kit manifest), and every core file must be in by READY. Selecting the oak tile requests
// the forest and hero packs at once; their files are held (page.route) so the tile, placed next, is caught
// waiting (placed and undoable, hidden, not rising), then released: the tile must leave `pack_waiting`
// and rise in the kit look to an unskipped `settled` cue, which a rebuild (it resets every rise to
// rest with no cue) would not produce. Every other pack then streams in the background.
{
  const manifest = JSON.parse(await readFile(join(dist, "assets/kit/manifest.json"), "utf8"));
  const packOf = new Map([["p1/ibl/noon.diffuse.ktx2", "core"], ["p1/ibl/noon.specular.ktx2", "core"]]);
  const suffixes = ["bc7.ktx2", "astc.ktx2", "png"];
  const addMaps = (maps, pack) => Object.values(maps ?? {}).forEach((m) => suffixes.forEach((s) => packOf.set(`${m}.${s}`, pack)));
  for (const s of Object.values(manifest.materials)) addMaps(s.maps, s.pack);
  for (const p of manifest.pieces) { packOf.set(p.glb, p.pack); addMaps(p.maps, p.pack); }
  const packIds = [...new Set(manifest.pieces.map((p) => p.pack))];
  await withPage(gpuArgs, async (page) => {
    const tag = "streaming";
    const log = [];
    let phase = "before";
    page.on("request", (r) => {
      const m = new URL(r.url()).pathname.match(/\/assets\/kit\/(.+)$/);
      if (m) log.push({ path: m[1], phase });
    });
    let release;
    const held = new Promise((r) => { release = r; });
    await page.route(/\/assets\/kit\/p1\/(forest_base|tree_forest(_[a-z])?|tree_hero_oak|materials\/litter|materials\/bark)\//,
      async (route) => { await held; await route.continue(); });
    await page.goto(base + "?scenario=play&tier=high");
    if (!(await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 90_000 }).then(() => true, () => false))) {
      fail(`${tag}: never ready`);
      release();
      return;
    }
    const state = () => page.evaluate(() => window.hexWorld.state());
    let s = await state();
    if (s.packs?.core !== "ready") fail(`${tag}: core not ready at READY: ${JSON.stringify(s.packs)}`);
    // No interaction for 2 s: nothing but core may load.
    await page.waitForTimeout(2_000);
    s = await state();
    const idle = packIds.filter((k) => k !== "core" && s.packs?.[k] !== "idle");
    if (idle.length) fail(`${tag}: packs requested before any interaction: ${JSON.stringify(s.packs)}`);
    const before = log.filter((e) => e.phase === "before");
    const family = before.map((e) => e.path).find((p) => /\.(bc7\.ktx2|astc\.ktx2|png)$/.test(p) && !p.includes("/ibl/"))?.replace(/^.*?\.(bc7\.ktx2|astc\.ktx2|png)$/, "$1");
    const notCore = before.filter((e) => packOf.get(e.path) !== "core");
    if (notCore.length) fail(`${tag}: non-core kit requests before interaction: ${notCore.map((e) => e.path).join(", ")}`);
    const coreFiles = [...packOf].filter(([p, k]) => k === "core" && (p.endsWith(".glb") || p.endsWith(`.${family}`) || p.includes("/ibl/"))).map(([p]) => p);
    const missingCore = coreFiles.filter((p) => !before.some((e) => e.path === p));
    if (!family || missingCore.length) fail(`${tag}: family ${family}; core files never requested: ${missingCore.join(", ")}`);
    console.log(`${tag}: before interaction ${before.length} kit requests, all core (family ${family}); packs ${JSON.stringify(s.packs)}`);

    phase = "after";
    // Selecting the oak tile requests its packs (forest_base, the hero oak) before any slot is hovered.
    await page.evaluate(() => { window.hexWorld.events(); window.hexWorld.selectTile("oak"); });
    const selDeadline = Date.now() + 10_000;
    while (Date.now() < selDeadline && (s = await state()).packs?.forest !== "loading") await page.waitForTimeout(50);
    if (s.packs?.forest !== "loading" || s.packs?.hero !== "loading" || s.tiles !== 1)
      fail(`${tag}: selecting the oak tile did not request its packs: ${JSON.stringify(s.packs)}`);
    const onSelect = log.filter((e) => e.phase === "after" && packOf.get(e.path) === "forest").length;
    console.log(`${tag}: selected oak (no hover, no placement): ${onSelect} forest files requested, packs ${JSON.stringify(s.packs)}`);
    await page.evaluate(() => window.hexWorld.placeTile(1, 0));
    const deadline = Date.now() + 20_000;
    while (Date.now() < deadline && !(s = await state()).pack_waiting?.some((w) => w.q === 1 && w.r === 0)) await page.waitForTimeout(50);
    const waiting = s.pack_waiting?.find((w) => w.q === 1 && w.r === 0);
    const rise0 = s.play?.rising?.find((t) => t.q === 1 && t.r === 0);
    if (s.tiles !== 2 || !waiting || waiting.tile !== "oak") fail(`${tag}: oak tile not placed and waiting: ${JSON.stringify(s.pack_waiting)}`);
    if (JSON.stringify(waiting?.packs) !== JSON.stringify(["forest", "hero"])) fail(`${tag}: oak tile waits on ${JSON.stringify(waiting?.packs)}`);
    if (s.packs?.forest !== "loading" || s.packs?.hero !== "loading") fail(`${tag}: forest/hero not loading: ${JSON.stringify(s.packs)}`);
    // Owner decision A10: the placement registers at once (tiles, undo), but the tile stays hidden
    // and does not rise until its packs arrive.
    if (rise0) fail(`${tag}: oak tile rises before its packs arrive: ${JSON.stringify(rise0)}`);
    if (!s.play?.tiles?.some((t) => t.q === 1 && t.r === 0 && t.tile === "oak") || !s.play?.can_undo)
      fail(`${tag}: the placement did not register while its packs stream: ${JSON.stringify(s.play?.tiles)} can_undo ${s.play?.can_undo}`);
    await page.waitForTimeout(150);
    await page.locator("canvas#hex-world").screenshot({ path: "e2e/test-results/smoke-streaming-waiting.png" });
    const forestFiles = [...packOf].filter(([p, k]) => k === "forest" && (p.endsWith(".glb") || p.endsWith(`.${family}`))).map(([p]) => p);
    const requested = new Set(log.filter((e) => e.phase === "after").map((e) => e.path));
    const unrequested = forestFiles.filter((p) => !requested.has(p));
    if (unrequested.length) fail(`${tag}: forest pack files not requested after the placement: ${unrequested.join(", ")}`);
    console.log(`${tag}: placed oak at (1, 0): waiting ${JSON.stringify(waiting?.packs)}, hidden (not rising); ` +
      `${forestFiles.length} forest files requested (held), packs ${JSON.stringify(s.packs)}`);

    release();
    const swapDeadline = Date.now() + 30_000;
    let swapped = null;
    while (Date.now() < swapDeadline) {
      s = await state();
      if (s.packs?.forest === "ready" && s.packs?.hero === "ready" && !s.pack_waiting?.some((w) => w.q === 1 && w.r === 0)) { swapped = s; break; }
      await page.waitForTimeout(25);
    }
    if (!swapped) { fail(`${tag}: oak tile never swapped: ${JSON.stringify(s.packs)} waiting ${JSON.stringify(s.pack_waiting)}`); return; }
    const riseAtSwap = swapped.play?.rising?.find((t) => t.q === 1 && t.r === 0);
    const settleDeadline = Date.now() + 10_000;
    let settled = null;
    const events = [];
    while (Date.now() < settleDeadline && !settled) {
      events.push(...(await page.evaluate(() => window.hexWorld.events())));
      settled = events.find((e) => e.type === "riseStage" && e.q === 1 && e.r === 0 && e.stage === "settled");
      if (!settled) await page.waitForTimeout(50);
    }
    if (!settled || settled.skipped) fail(`${tag}: oak rise did not settle on its own after the swap: ${JSON.stringify(settled)}`);
    console.log(`${tag}: swapped to kit ${riseAtSwap ? `mid-rise at t ${riseAtSwap.t} s (${riseAtSwap.stage})` : "after the rise"}; ` +
      `settled cue skipped=${settled?.skipped}`);
    await page.locator("canvas#hex-world").screenshot({ path: "e2e/test-results/smoke-streaming-kit.png" });
    const allDeadline = Date.now() + 60_000;
    while (Date.now() < allDeadline && !packIds.every((k) => s.packs?.[k] === "ready")) {
      await page.waitForTimeout(250);
      s = await state();
    }
    if (!packIds.every((k) => s.packs?.[k] === "ready")) fail(`${tag}: background streaming did not finish: ${JSON.stringify(s.packs)}`);
    const order = [];
    for (const e of log.filter((e) => e.phase === "after")) {
      const k = packOf.get(e.path);
      if (k && k !== "core" && !order.includes(k)) order.push(k);
    }
    console.log(`${tag}: after the placement packs were first requested in order ${order.join(", ")}; final ${JSON.stringify(s.packs)}`);
  });
}

// 10. Kit fetch retries (crates/hex-assets/src/bevy.rs): any host can drop a request, and one dropped
// fetch must not remove a feature for the session. The first request for a boid piece (core, loaded in
// every scenario) and for a forest pack file is reset: both are re-read and the game ends with the
// boids drawn, the forest pack ready and nothing in `assets.failed`. Then the boid piece is reset on
// every request: it gives up after the first request and three retries, names the file in an error
// and in `assets.failed`, and the flock degrades to `boids.pieces = "failed"` as before.
// Shaders load through the default source, which the web reads through the same retrying reader: the
// boid compute shader's first request is reset alongside them and the compute pass must still come up
// (`boids.dispatched` > 0); reset on every request it gives up and is named as `shaders/boids_compute.wgsl`.
{
  const BIRD = "p1/bird_starling/bird_starling.glb";
  const FOREST = "p1/forest_base/forest_base.glb";
  const SHADER = "shaders/boids_compute.wgsl";
  const ATTEMPTS = 4;
  // Bevy's own error for the failed load, which does not name the file (the web reader's message).
  const BEVY_LOAD_ERROR = /I\/O error while loading asset: Failed to fetch path/;
  // Request URLs carry ?v=<hash> (scripts/web/version-dist.mjs): match on the path.
  const urlPath = (u) => { try { return new URL(u).pathname; } catch { return ""; } };
  const site = (p) => (p.startsWith("shaders/") ? `/assets/${p}` : `/assets/kit/${p}`);
  const at = (p) => (url) => url.pathname === site(p);
  const dropped = (paths) => (m) => m.text().startsWith("Failed to load resource") && paths.some((p) => urlPath(m.location()?.url ?? "") === site(p));
  const play = async (page, tag) => {
    await page.goto(base + "?scenario=play&tier=high");
    const ok = await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 90_000 }).then(() => true, () => false);
    if (!ok) fail(`${tag}: never ready`);
    return ok;
  };
  const until = async (page, done, ms) => {
    const deadline = Date.now() + ms;
    let s;
    while (Date.now() < deadline && !done((s = await page.evaluate(() => window.hexWorld.state())))) await page.waitForTimeout(100);
    return s;
  };

  await withPage(gpuArgs, async (page) => {
    const tag = "kit-retry";
    const hits = new Map();
    for (const path of [BIRD, FOREST, SHADER]) {
      await page.route(at(path), async (route) => {
        const n = (hits.get(path) ?? 0) + 1;
        hits.set(path, n);
        if (n === 1) await route.abort("connectionreset");
        else await route.continue();
      });
    }
    if (!(await play(page, tag))) return;
    await page.evaluate(() => { window.hexWorld.events(); window.hexWorld.selectTile("forest"); });
    const s = await until(page, (s) => s.packs?.forest === "ready" && ["ready", "failed"].includes(s.boids?.pieces) && s.boids?.dispatched > 0, 60_000);
    const counts = Object.fromEntries([BIRD, FOREST, SHADER].map((p) => [p, hits.get(p) ?? 0]));
    if (Object.values(counts).some((n) => n !== 2)) fail(`${tag}: each reset file should be requested twice: ${JSON.stringify(counts)}`);
    if (s.boids?.pieces !== "ready") fail(`${tag}: boids.pieces ${s.boids?.pieces} after a reset boid piece request`);
    if (!(s.boids?.dispatched > 0)) fail(`${tag}: the boid compute pass never ran after a reset shader request (dispatched ${s.boids?.dispatched})`);
    if (s.packs?.forest !== "ready" || s.packs?.core !== "ready") fail(`${tag}: packs ${JSON.stringify(s.packs)}`);
    if (!Array.isArray(s.assets?.failed) || s.assets.failed.length) fail(`${tag}: assets.failed ${JSON.stringify(s.assets)}`);
    console.log(`${tag}: requests ${JSON.stringify(counts)}; boids.pieces ${s.boids?.pieces}, boids.dispatched ${s.boids?.dispatched}, ` +
      `packs core ${s.packs?.core} forest ${s.packs?.forest}, assets.failed ${JSON.stringify(s.assets?.failed)}`);
  }, dropped([BIRD, FOREST, SHADER]));

  await withPage(gpuArgs, async (page) => {
    const tag = "kit-give-up";
    let hits = 0;
    const gaveUp = [];
    page.on("console", (m) => { if (m.text().includes("ERROR") && m.text().includes(`kit://${BIRD}`)) gaveUp.push(m.text()); });
    await page.route(at(BIRD), async (route) => { hits++; await route.abort("connectionreset"); });
    const t0 = Date.now();
    if (!(await play(page, tag))) return;
    const s = await until(page, (s) => s.boids?.pieces === "failed" && s.assets?.failed?.length, 30_000);
    const secs = ((Date.now() - t0) / 1000).toFixed(1);
    if (hits !== ATTEMPTS) fail(`${tag}: ${hits} requests for ${BIRD}, expected ${ATTEMPTS} (the first and three retries)`);
    if (s.boids?.pieces !== "failed") fail(`${tag}: boids.pieces ${s.boids?.pieces}, expected failed`);
    if (JSON.stringify(s.assets?.failed) !== JSON.stringify([`kit://${BIRD}`])) fail(`${tag}: assets.failed ${JSON.stringify(s.assets)}`);
    if (s.packs?.core !== "ready") fail(`${tag}: core pack ${s.packs?.core}, expected ready with the bird as a placeholder`);
    if (!gaveUp.some((t) => /giving up/.test(t))) fail(`${tag}: no give-up error naming kit://${BIRD}: ${JSON.stringify(gaveUp)}`);
    console.log(`${tag}: ${hits} requests, gave up within ${secs} s of load; boids.pieces ${s.boids?.pieces}, assets.failed ${JSON.stringify(s.assets?.failed)}; ` +
      `errors naming the file: ${gaveUp.length} (${gaveUp.map((t) => t.replace(/%c/g, "").slice(0, 120)).join(" | ")})`);
  }, (m) => dropped([BIRD])(m) || m.text().includes(BIRD) || BEVY_LOAD_ERROR.test(m.text()));

  await withPage(gpuArgs, async (page) => {
    const tag = "shader-give-up";
    let hits = 0;
    const gaveUp = [];
    page.on("console", (m) => { if (m.text().includes("ERROR") && m.text().includes(SHADER)) gaveUp.push(m.text()); });
    await page.route(at(SHADER), async (route) => { hits++; await route.abort("connectionreset"); });
    if (!(await play(page, tag))) return;
    const s = await until(page, (s) => s.assets?.failed?.length, 30_000);
    await page.waitForTimeout(1_000);
    const after = await page.evaluate(() => window.hexWorld.state());
    if (hits !== ATTEMPTS) fail(`${tag}: ${hits} requests for ${SHADER}, expected ${ATTEMPTS} (the first and three retries)`);
    if (JSON.stringify(after.assets?.failed) !== JSON.stringify([SHADER])) fail(`${tag}: assets.failed ${JSON.stringify(after.assets)}`);
    if (after.boids?.dispatched !== 0) fail(`${tag}: boids dispatched ${after.boids?.dispatched} without their compute shader`);
    if (!gaveUp.some((t) => /giving up/.test(t))) fail(`${tag}: no give-up error naming ${SHADER}: ${JSON.stringify(gaveUp)}`);
    console.log(`${tag}: ${hits} requests; assets.failed ${JSON.stringify(after.assets?.failed)}, boids.dispatched ${after.boids?.dispatched}, ` +
      `the game still runs (frames ${after.frames}, first seen failed at ${s.frames}); errors naming the file: ${gaveUp.length}`);
  }, (m) => dropped([SHADER])(m) || m.text().includes(SHADER) || BEVY_LOAD_ERROR.test(m.text()));
}

// 11. Cache busting (scripts/web/version-dist.mjs): every same-origin request a full play load makes
// (HUD, scripts, fonts, thumbnails, shaders, every kit pack, the IBL, audio after a gesture and the
// streamed music) carries ?v=<hash> equal to the served file's content hash, except the page itself, the
// build id the HUD polls (version.json, fetched with its own cache-busting query) and Trunk's own hashed
// glue and wasm. References headless Chromium never fetches (favicons, the web
// manifest and its icons) are checked in the built files instead.
{
  const hashOf = new Map();
  const fileHash = async (p) => {
    if (!hashOf.has(p)) hashOf.set(p, contentHash(await readFile(join(dist, p))));
    return hashOf.get(p);
  };
  const origin = new URL(base).origin;
  const kind = (p) => {
    if (p.startsWith("assets/kit/p1/ibl/")) return "ibl";
    if (p.startsWith("assets/kit/")) return `kit ${p.endsWith(".glb") ? "glb" : p.replace(/^.*?\.((bc7|astc)\.ktx2|png)$/, "$1")}`;
    if (p.startsWith("assets/shaders/")) return "shader";
    if (p.startsWith("assets/thumbs/")) return "thumbnail";
    if (p === "assets/audio/manifest.json") return "audio manifest";
    if (p.startsWith("assets/audio/music/")) return "music (streamed)";
    if (p.startsWith("assets/audio/")) return "audio cue";
    if (p.startsWith("web/ui/fonts/")) return "font";
    if (p.startsWith("web/ui/")) return `hud ${p.split(".").pop()}`;
    if (p.startsWith("web/brand/") || p === "favicon.ico") return "icon";
    return p;
  };
  await withPage(gpuArgs, async (page) => {
    const tag = "versions";
    const seen = [];
    page.on("request", (r) => { const u = new URL(r.url()); if (u.origin === origin) seen.push(u); });
    await page.goto(base + "?scenario=play&tier=high&tilt=on");
    if (!(await page.waitForFunction(() => window.hexWorld?.ready() === true && window.hexWorld.audio, null, { timeout: 90_000 }).then(() => true, () => false))) {
      fail(`${tag}: never ready`);
      return;
    }
    // A gesture starts audio (cues decode, the start meadow's music streams); a selection and placements
    // start background streaming of every pack and draw water.
    await page.locator(".bar__world").click();
    await page.evaluate(() => {
      const h = window.hexWorld;
      h.selectTile("river_straight"); h.placeTile(-1, 0);
      h.selectTile("beach"); h.placeTile(1, 0);
      h.selectTile("oak");
    });
    const deadline = Date.now() + 90_000;
    let s, a;
    const packIds = [...new Set(JSON.parse(await readFile(join(dist, "assets/kit/manifest.json"), "utf8")).pieces.map((p) => p.pack))];
    while (Date.now() < deadline) {
      [s, a] = await page.evaluate(() => [window.hexWorld.state(), window.hexWorld.audio.state()]);
      const music = seen.some((u) => u.pathname.startsWith("/assets/audio/music/"));
      if (packIds.every((k) => s.packs?.[k] === "ready") && a.loading === "done" && music) break;
      await page.waitForTimeout(250);
    }
    if (!packIds.every((k) => s.packs?.[k] === "ready")) fail(`${tag}: packs never all ready: ${JSON.stringify(s.packs)}`);
    if (a?.loading !== "done") fail(`${tag}: audio never loaded: ${a?.loading} ${a?.error ?? ""}`);
    await page.waitForTimeout(1_000);
    const counts = {};
    const bad = [];
    const trunk = [];
    for (const u of seen) {
      const p = decodeURIComponent(u.pathname).replace(/^\//, "");
      if (p === "" || p === "index.html" || p === "version.json") continue;
      if (TRUNK_HASHED.test(p)) { trunk.push(p); continue; }
      const v = u.searchParams.get("v");
      const want = await fileHash(p).catch(() => null);
      if (!v || v !== want) bad.push(`${u.pathname}${u.search} (content ${want ?? "missing"})`);
      counts[kind(p)] = (counts[kind(p)] ?? 0) + 1;
    }
    if (bad.length) fail(`${tag}: ${bad.length} requests without their content hash: ${bad.join(", ")}`);
    for (const k of ["kit glb", "ibl", "shader", "thumbnail", "audio manifest", "audio cue", "music (streamed)", "hud js", "hud css", "font"])
      if (!counts[k]) fail(`${tag}: no ${k} request recorded`);
    console.log(`${tag}: ${seen.length} same-origin requests, ${seen.length - bad.length - trunk.length} versioned and matching, ` +
      `${trunk.length} Trunk-hashed (${[...new Set(trunk)].join(", ")}), ${bad.length} bad; by kind ${JSON.stringify(counts)}`);
  });

  // Built files: every same-origin reference in the shipped HTML, CSS and web manifest is versioned and
  // current, and index.html inlines the map before its first script.
  const html = await readFile(join(dist, "index.html"), "utf8");
  const map = JSON.parse(/window\.__HEX_HASHES=(\{.*?\});/.exec(html)?.[1] ?? "null");
  if (!map || html.indexOf("hex-hashes") > html.indexOf("<script src=")) fail("versions: index.html lacks the hash map before its first script");
  let refs = 0;
  for (const [file, base] of [["index.html", ""], ["web/unsupported.html", "web/"], ["web/ui/hud.css", "web/ui/"], ["web/brand/site.webmanifest", "web/brand/"]]) {
    const text = await readFile(join(dist, file), "utf8");
    for (const m of text.matchAll(/(?:href|src)="([^"#]+)"|"src":\s*"([^"]+)"|url\(([^)]+)\)|replace\("([^"]+)"\)/g)) {
      const ref = m[1] ?? m[2] ?? m[3] ?? m[4];
      if (/^[a-z]+:|^\/\//i.test(ref) || ref === "../") continue;
      const [path, query] = ref.split("?");
      const p = path.startsWith("/") ? path.slice(1) : join(base, path);
      if (TRUNK_HASHED.test(p)) continue;
      refs++;
      const want = await fileHash(p).catch(() => null);
      if (query !== `v=${want}` || map?.[p] !== want) fail(`versions: ${file} references ${ref}, content ${want}, map ${map?.[p]}`);
    }
  }
  console.log(`versions: index.html map has ${Object.keys(map ?? {}).length} files; ${refs} references in the built HTML, CSS and web manifest checked`);
}

// 12. Cars fading out over a lowered cap (PRD §7.2) draw with the blended copy of their car material,
// a pipeline no other case compiles. With the device clamped to WebGPU's default 16 sampled textures,
// the cars world of docs/checkpoints/cars/cars.json is built, `set_car_cap` drops the cap to 1 and
// photo mode pauses the agents about two ticks into the 250 ms fade, freezing the fading cars half
// drawn. The frozen frame must differ from the frame after the cars are gone (blended car pixels on
// screen), with no validation error and no pipeline over the limit.
{
  const clamp = WEBGPU_DEFAULT_SAMPLED;
  const script = JSON.parse(await readFile(join(dist, "../docs/checkpoints/cars/cars.json"), "utf8").catch(() => "[]"));
  const build = script.slice(0, script.findIndex((c) => c.cmd === "dump_state"));
  await withPage(gpuArgs, async (page) => {
    let carKit = false;
    page.on("console", (m) => { if (/kit car pieces ready/.test(m.text())) carKit = true; });
    await page.addInitScript(clampSampledTextures, clamp);
    await page.addInitScript(countSampledTextures);
    await page.goto(base + "?tier=high");
    const tag = `car fade (clamped ${clamp})`;
    if (!(await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 90_000 }).then(() => true, () => false))) {
      fail(`${tag}: never ready`);
      return;
    }
    const cmd = (c) => page.evaluate((c) => window.hexWorld.cmd(c), c);
    for (const c of build) await cmd(c);
    const cars = () => page.evaluate(() => { const p = window.hexWorld.state().play ?? {}; return (p.cars ?? 0) + (p.parked_cars ?? 0); });
    const deadline = Date.now() + 60_000;
    while (Date.now() < deadline && !(carKit && (await cars()) >= 3)) await page.waitForTimeout(200);
    if (!carKit || (await cars()) < 3) { fail(`${tag}: car kit ${carKit}, ${await cars()} cars`); return; }
    await cmd({ cmd: "camera", pos: [-10, 12, 40], look_at: [-2, 0.5, 22.5] });
    await page.waitForTimeout(1_000);
    const canvas = page.locator("canvas#hex-world");
    const before = (await canvas.screenshot()).toString("base64");
    await cmd({ cmd: "set_car_cap", cap: 1 });
    await cmd({ cmd: "wait", frames: 6 });
    await cmd({ cmd: "photo", pause_agents: true });
    await page.waitForTimeout(1_000);
    const fading = (await canvas.screenshot()).toString("base64");
    const during = await cars();
    await cmd({ cmd: "photo", pause_agents: false });
    await page.waitForTimeout(1_500);
    const gone = (await canvas.screenshot()).toString("base64");
    const after = await cars();
    await canvas.screenshot({ path: "e2e/test-results/smoke-car-fade.png" });
    // Pixels whose colour moves by more than 24/255 between two frames.
    const changed = await page.evaluate(async ([a, b]) => {
      const load = (s) => new Promise((ok) => { const i = new Image(); i.onload = () => ok(i); i.src = "data:image/png;base64," + s; });
      const [ia, ib] = await Promise.all([load(a), load(b)]);
      const px = (i) => { const c = document.createElement("canvas"); c.width = i.width; c.height = i.height; const x = c.getContext("2d"); x.drawImage(i, 0, 0); return x.getImageData(0, 0, i.width, i.height).data; };
      const [pa, pb] = [px(ia), px(ib)];
      let n = 0;
      for (let k = 0; k < pa.length; k += 4) if (Math.abs(pa[k] - pb[k]) + Math.abs(pa[k + 1] - pb[k + 1]) + Math.abs(pa[k + 2] - pb[k + 2]) > 24) n++;
      return n;
    }, [fading, gone]);
    const { sampled, limit } = await page.evaluate(() => ({ sampled: window.__sampledTextures, limit: window.__deviceSampledLimit }));
    console.log(`${tag}: cars ${during} frozen mid-fade, ${after} after; ${changed} px of fading cars on screen; sampled textures per stage max ${sampled?.max}`);
    if (during < 3 || after !== 1) fail(`${tag}: ${during} cars while fading, ${after} after (want 1)`);
    if (changed < 200) fail(`${tag}: only ${changed} px differ between the frozen fade and the empty spots`);
    if (before === fading) fail(`${tag}: the frozen frame equals the frame before the fade`);
    if (limit !== clamp) fail(`${tag}: device limit ${limit}, expected ${clamp}`);
    if (!(sampled?.max <= clamp)) fail(`${tag}: a pipeline layout uses ${sampled?.max} sampled textures`);
  });
}

server.close();
console.log(process.exitCode ? "smoke FAILED" : "smoke OK");
