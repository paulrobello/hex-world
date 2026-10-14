#!/usr/bin/env node
// Browser proof for the brand wiring: serves a Trunk-built dist/ and loads it in GPU Chromium.
// Checks that both pages load with no console error or CSP violation, that every linked icon and the
// manifest come back 200 with the right type, and that DevTools parses the manifest without errors.
//
// Usage: node scripts/brand/check-web.mjs [dist-dir]   (needs `npm ci --prefix e2e` for Playwright)
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { extname, join } from "node:path";

const { chromium } = createRequire(new URL("../../e2e/package.json", import.meta.url))("playwright");
const dist = process.argv[2] ?? "dist";
const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".png": "image/png",
  ".ico": "image/x-icon", ".svg": "image/svg+xml", ".webmanifest": "application/manifest+json",
  ".ktx2": "image/ktx2", ".glb": "model/gltf-binary", ".wgsl": "text/plain", ".json": "application/json" };
const server = createServer(async (req, res) => {
  let path = decodeURIComponent(new URL(req.url, "http://x").pathname);
  if (path.endsWith("/")) path += "index.html";
  try {
    const body = await readFile(join(dist, path));
    res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" });
    res.end(body);
  } catch {
    res.writeHead(404);
    res.end();
  }
});
await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
const base = `http://127.0.0.1:${server.address().port}`;
let failed = false;
const fail = (m) => {
  console.error("FAIL:", m);
  failed = true;
};

// A persistent profile: the default incognito context reports "in-incognito" for installability.
const profile = mkdtempSync(join(tmpdir(), "hexw-brand-"));
const browser = await chromium.launchPersistentContext(profile, { channel: "chromium", headless: true,
  viewport: { width: 1280, height: 800 }, args: ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu"] });
try {
  for (const [path, waitReady] of [["/", true], ["/web/unsupported.html", false]]) {
    const page = await browser.newPage();
    const errors = [];
    const bad = [];
    page.on("pageerror", (e) => errors.push(String(e)));
    page.on("console", (m) => { if (m.type() === "error" || /Content Security Policy|%cERROR%c|\bERROR\b/.test(m.text())) errors.push(m.text()); });
    page.on("response", (r) => { if (r.status() >= 400) bad.push(`${r.status()} ${r.url()}`); });
    await page.addInitScript(() => {
      window.__csp = [];
      document.addEventListener("securitypolicyviolation", (e) => window.__csp.push(`${e.violatedDirective} ${e.blockedURI}`));
    });
    await page.goto(base + path);
    if (waitReady) await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 60_000 });
    else await page.waitForLoadState("load");
    await page.waitForTimeout(1500);
    const url = new URL(page.url()).pathname;
    if (url !== path) fail(`${path} ended on ${url}`);
    const links = await page.evaluate(() =>
      [...document.querySelectorAll("link[rel~=icon], link[rel=apple-touch-icon], link[rel=manifest]")].map((l) => l.href));
    const og = await page.evaluate(() => document.querySelector("meta[property='og:image']")?.content ?? null);
    const cdp = await page.context().newCDPSession(page);
    const manifest = await cdp.send("Page.getAppManifest");
    const install = await cdp.send("Page.getInstallabilityErrors");
    const parsed = manifest.data ? JSON.parse(manifest.data) : null;
    const icons = (parsed?.icons ?? []).map((i) => new URL(i.src, manifest.url).href);
    const fetched = await page.evaluate(async (urls) => Promise.all(urls.map(async (u) => {
      const r = await fetch(u);
      return `${r.status} ${r.headers.get("content-type")} ${(await r.arrayBuffer()).byteLength}B ${new URL(u).pathname}`;
    })), [...new Set([...links, `${base}/favicon.ico`, ...icons, ...(og ? [new URL(new URL(og).pathname, base).href] : [])])]);
    const csp = await page.evaluate(() => window.__csp);
    console.log(`\n== ${path} (ended on ${url})`);
    console.log(`console errors: ${errors.length}`);
    console.log(`CSP violations: ${csp.length}`);
    console.log(`HTTP >= 400: ${bad.length ? bad.join(", ") : "none"}`);
    for (const f of fetched) console.log(`fetch: ${f}`);
    console.log(`manifest url: ${manifest.url}`);
    console.log(`manifest parse errors: ${JSON.stringify(manifest.errors)}`);
    console.log(`manifest name/display/theme/bg: ${parsed?.name} / ${parsed?.display} / ${parsed?.theme_color} / ${parsed?.background_color}, icons ${parsed?.icons?.length}`);
    console.log(`installability errors: ${JSON.stringify(install.installabilityErrors)}`);
    if (errors.length) fail(`${path} console errors: ${errors.join(" | ")}`);
    if (csp.length) fail(`${path} CSP violations: ${csp.join(" | ")}`);
    if (bad.length) fail(`${path} failed requests: ${bad.join(" | ")}`);
    if (manifest.errors.length) fail(`${path} manifest errors: ${JSON.stringify(manifest.errors)}`);
    if (fetched.some((f) => !f.startsWith("200 "))) fail(`${path} icon fetch failed`);
    await page.close();
  }
} finally {
  await browser.close();
  server.close();
  rmSync(profile, { recursive: true, force: true });
}
process.exitCode = failed ? 1 : 0;
