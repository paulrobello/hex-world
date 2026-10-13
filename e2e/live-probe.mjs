// Probe the deployed site: time to READY, wire bytes per resource, console errors, screenshot.
// Usage: node e2e/live-probe.mjs [url] [out.json] [shot.png]
import { chromium } from "playwright";
import { writeFileSync } from "node:fs";
import { BAD_TEXT } from "./console-errors.mjs";

const url = process.argv[2] ?? "https://hex-world.pardev.net/";
const out = process.argv[3] ?? "docs/perf/p0-deploy.json";
const shot = process.argv[4] ?? "target/shots/live.png";
const args = ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu"];

const browser = await chromium.launch({ channel: "chromium", headless: true, args });
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const errors = [];
const res = [];
page.on("pageerror", (e) => errors.push(String(e)));
page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });
page.on("requestfinished", async (r) => {
  const s = await r.sizes();
  res.push({ url: r.url().replace(url, "/"), wire: s.responseBodySize + s.responseHeadersSize, encoding: (await r.response())?.headers()["content-encoding"] ?? "" });
});
const t0 = Date.now();
await page.goto(url);
await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 120_000 });
const readyMs = Date.now() - t0;
await page.waitForTimeout(2_000);
await page.screenshot({ path: shot });
await browser.close();

const wasm = res.find((r) => r.url.endsWith(".wasm"));
const report = {
  date: new Date().toISOString(), url, browser: "headless Chromium (Metal)", viewport: "1280x800",
  ready_ms: readyMs, wasm_wire_bytes: wasm?.wire ?? null, wasm_encoding: wasm?.encoding ?? null,
  total_wire_bytes: res.reduce((a, r) => a + r.wire, 0), requests: res.length, console_errors: errors,
  note: "Single cold load from this machine's network; not a controlled timing run.",
};
writeFileSync(out, JSON.stringify(report, null, 2) + "\n");
console.log(JSON.stringify({ ...report, console_errors: errors.length }, null, 2));
process.exitCode = errors.length ? 1 : 0;
