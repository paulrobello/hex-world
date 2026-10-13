// Local-only browser smoke test: GPU Chromium, WebGPU backend, gate redirect, bad scenario fallback.
import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join } from "node:path";
import { BAD_TEXT } from "./console-errors.mjs";

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

server.close();
console.log(process.exitCode ? "smoke FAILED" : "smoke OK");
