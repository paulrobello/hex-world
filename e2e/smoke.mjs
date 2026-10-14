// Local-only browser smoke test: GPU Chromium, WebGPU backend, gate redirect, bad scenario fallback.
import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join } from "node:path";
import { BAD_TEXT } from "./console-errors.mjs";
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
// Handle-let aliases the shim rewrites once the kit scenario has drawn: one module, the KitMaterial
// fragment (Bevy's PBR input composed in), asserted by its `masks_texture` global (markedModuleProbe).
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
const WATER_SAMPLED = { high: { water: 10, terrain: 10 }, medium: { water: 8, terrain: 8 }, low: { water: 7, terrain: 7 } };
// Handle-let aliases the compat shim rewrites in a water scenario run: none, since neither water
// shader composes StandardMaterial's PBR input (the marker probe still checks the water module).
const WATER_COMPAT_ALIASES = 0;

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

// 1. The Single scenario becomes ready on the WebGPU backend.
await withPage(gpuArgs, async (page, errors) => {
  const kit = new Map();
  page.on("console", (m) => { if (/texture family/i.test(m.text())) console.log("app:", m.text()); });
  page.on("requestfinished", async (r) => {
    if (!r.url().includes("/assets/kit/")) return;
    const t = r.timing(), size = (await r.sizes()).responseBodySize;
    kit.set(r.url().split("/").pop(), { ms: Math.round(t.responseEnd), bytes: size });
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
  await page.waitForSelector("#hud:not(.is-off) .bar", { state: "visible", timeout: 15_000 })
    .catch(() => fail("bare URL: the HUD tool bar never showed"));
  await page.waitForSelector(".sheet--tiles:not(.is-closed)", { state: "visible", timeout: 15_000 })
    .catch(() => fail("bare URL: the Tiles sheet is not open by default"));
  await page.screenshot({ path: "e2e/test-results/smoke-default-play.png" });
});

// 2b. Unknown scenario falls back to the web default (play) without errors.
await withPage(gpuArgs, async (page, errors) => {
  await page.goto(base + "?shot=nope");
  await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 90_000 });
  const state = await page.evaluate(() => window.hexWorld.state());
  if (state.scenario !== "Play") fail("bad scenario did not fall back: " + JSON.stringify(state.scenario));
  if (errors.some((e) => /panic|unreachable/i.test(e))) fail("panic on bad scenario");
});

// 3. No GPU → unsupported page.
await withPage(["--disable-gpu"], async (page) => {
  await page.goto(base);
  await page.waitForURL(/unsupported\.html$/, { timeout: 15_000 }).catch(() => fail("no redirect without GPU"));
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
    await page.goto(base + query);
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
      if (!compat?.active || compat.aliases !== aliases)
        fail(`wgsl-compat ${query}: expected ${aliases} aliases, got ` + JSON.stringify(compat));
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
    await page.goto(base + query);
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
    await page.waitForTimeout(1_500);
    const { sampled, limit, state } = await page.evaluate(() => ({
      sampled: window.__sampledTextures, limit: window.__deviceSampledLimit, state: window.hexWorld.state() }));
    console.log(`${tag}: ${spawned} meshes; sampled textures per stage max ${sampled?.max}, kit pipelines ` +
      `${JSON.stringify(sampled?.kit)}, water pipelines ${JSON.stringify(sampled?.water)}; state.gpu ${JSON.stringify(state.gpu)}`);
    if (state.scenario !== "WaterKit" || state.tiles !== 12) fail(`${tag}: state scenario ${state.scenario}, tiles ${state.tiles}`);
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
    await page.goto(base + query);
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
        (s.water ?? []).some((k) => k.pass === "water" && k.ibl) && (s.water ?? []).some((k) => k.pass === "terrain" && k.ibl);
    }))) await page.waitForTimeout(200);
    await page.waitForTimeout(1_500);
    const { sampled, limit, state } = await page.evaluate(() => ({
      sampled: window.__sampledTextures, limit: window.__deviceSampledLimit, state: window.hexWorld.state() }));
    console.log(`${tag}: tiles ${state.tiles}; sampled textures per stage max ${sampled?.max}, kit pipelines ` +
      `${JSON.stringify(sampled?.kit)}, water pipelines ${JSON.stringify(sampled?.water)}; state.gpu ${JSON.stringify(state.gpu)}`);
    if (state.scenario !== "Play" || state.tiles !== 5) fail(`${tag}: state scenario ${state.scenario}, tiles ${state.tiles}`);
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
      if (pass === "water" && !got.every((k) => k.transmission))
        fail(`${tag}: water shader not composed with the transmission texture: ${JSON.stringify(got)}`);
    }
    if (limit !== clamp) fail(`${tag}: device limit ${limit}, expected ${clamp}`);
    if (state.gpu?.maxSampledTexturesPerShaderStage !== clamp) fail(`${tag}: hexWorld.state().gpu ${JSON.stringify(state.gpu)}`);
    if (!(sampled?.max <= clamp)) fail(`${tag}: a pipeline layout uses ${sampled?.max} sampled textures`);
    await page.locator("canvas#hex-world").screenshot({ path: "e2e/test-results/smoke-play-water.png" });
  });
}

// 9. Kit streaming by pack (PRD §10, crates/hex-render/src/play/packs.rs). Before the first
// interaction the play scenario may fetch only core files (the network log, mapped to packs through
// dist's kit manifest), and every core file must be in by READY. Selecting the forest tile requests
// the forest and hero packs at once; their files are held (page.route) so the tile, placed next, is caught on its
// placeholder mid-rise, then released: the tile must leave `pack_waiting` (swapped to the kit look)
// and its rise must run on to an unskipped `settled` cue, which a rebuild (it resets every rise to
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
    await page.route(/\/assets\/kit\/p1\/(forest_base|tree_forest|tree_hero_oak|materials\/litter|materials\/bark)\//,
      async (route) => { await held; await route.continue(); });
    await page.goto(base + "?scenario=play");
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
    // Selecting the forest tile requests its packs before any slot is hovered.
    await page.evaluate(() => { window.hexWorld.events(); window.hexWorld.selectTile("forest"); });
    const selDeadline = Date.now() + 10_000;
    while (Date.now() < selDeadline && (s = await state()).packs?.forest !== "loading") await page.waitForTimeout(50);
    if (s.packs?.forest !== "loading" || s.packs?.hero !== "loading" || s.tiles !== 1)
      fail(`${tag}: selecting the forest tile did not request its packs: ${JSON.stringify(s.packs)}`);
    const onSelect = log.filter((e) => e.phase === "after" && packOf.get(e.path) === "forest").length;
    console.log(`${tag}: selected forest (no hover, no placement): ${onSelect} forest files requested, packs ${JSON.stringify(s.packs)}`);
    await page.evaluate(() => window.hexWorld.placeTile(1, 0));
    const deadline = Date.now() + 20_000;
    while (Date.now() < deadline && !(s = await state()).pack_waiting?.some((w) => w.q === 1 && w.r === 0)) await page.waitForTimeout(50);
    const waiting = s.pack_waiting?.find((w) => w.q === 1 && w.r === 0);
    const rise0 = s.play?.rising?.find((t) => t.q === 1 && t.r === 0);
    if (s.tiles !== 2 || !waiting || waiting.tile !== "forest") fail(`${tag}: forest tile not placed and waiting: ${JSON.stringify(s.pack_waiting)}`);
    if (JSON.stringify(waiting?.packs) !== JSON.stringify(["forest", "hero"])) fail(`${tag}: forest tile waits on ${JSON.stringify(waiting?.packs)}`);
    if (s.packs?.forest !== "loading" || s.packs?.hero !== "loading") fail(`${tag}: forest/hero not loading: ${JSON.stringify(s.packs)}`);
    if (!rise0) fail(`${tag}: forest tile is not rising: ${JSON.stringify(s.play?.rising)}`);
    await page.waitForTimeout(150);
    await page.locator("canvas#hex-world").screenshot({ path: "e2e/test-results/smoke-streaming-placeholder.png" });
    const forestFiles = [...packOf].filter(([p, k]) => k === "forest" && (p.endsWith(".glb") || p.endsWith(`.${family}`))).map(([p]) => p);
    const requested = new Set(log.filter((e) => e.phase === "after").map((e) => e.path));
    const unrequested = forestFiles.filter((p) => !requested.has(p));
    if (unrequested.length) fail(`${tag}: forest pack files not requested after the placement: ${unrequested.join(", ")}`);
    console.log(`${tag}: placed forest at (1, 0): waiting ${JSON.stringify(waiting?.packs)}, rise t ${rise0?.t} s (${rise0?.stage}); ` +
      `${forestFiles.length} forest files requested (held), packs ${JSON.stringify(s.packs)}`);

    release();
    const swapDeadline = Date.now() + 30_000;
    let swapped = null;
    while (Date.now() < swapDeadline) {
      s = await state();
      if (s.packs?.forest === "ready" && s.packs?.hero === "ready" && !s.pack_waiting?.some((w) => w.q === 1 && w.r === 0)) { swapped = s; break; }
      await page.waitForTimeout(25);
    }
    if (!swapped) { fail(`${tag}: forest tile never swapped: ${JSON.stringify(s.packs)} waiting ${JSON.stringify(s.pack_waiting)}`); return; }
    const riseAtSwap = swapped.play?.rising?.find((t) => t.q === 1 && t.r === 0);
    if (riseAtSwap && rise0 && !(riseAtSwap.t > rise0.t)) fail(`${tag}: rise clock went back across the swap: ${rise0.t} -> ${riseAtSwap.t}`);
    const settleDeadline = Date.now() + 10_000;
    let settled = null;
    const events = [];
    while (Date.now() < settleDeadline && !settled) {
      events.push(...(await page.evaluate(() => window.hexWorld.events())));
      settled = events.find((e) => e.type === "riseStage" && e.q === 1 && e.r === 0 && e.stage === "settled");
      if (!settled) await page.waitForTimeout(50);
    }
    if (!settled || settled.skipped) fail(`${tag}: forest rise did not settle on its own after the swap: ${JSON.stringify(settled)}`);
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

server.close();
console.log(process.exitCode ? "smoke FAILED" : "smoke OK");
