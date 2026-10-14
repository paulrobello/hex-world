// Local-only web screenshot gallery: drives scripts/shots/gallery.json through window.hexWorld.
import { chromium } from "playwright";
import { createServer } from "node:http";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { extname, join } from "node:path";

const dist = process.argv[2] ?? "dist";
const script = JSON.parse(await readFile(process.argv[3] ?? "scripts/shots/gallery.json", "utf8"));
// Optional extra query keys, e.g. "kit_src=fixture".
const extra = process.argv[4] ? "&" + process.argv[4] : "";
const outDir = "e2e/test-results";
const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css",
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

const gpuArgs = process.env.PW_GPU === "swiftshader"
  ? ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--enable-unsafe-webgpu"]
  : ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu"];
const fail = (m) => { console.error("FAIL:", m); process.exitCode = 1; };

await mkdir(outDir, { recursive: true });
const browser = await chromium.launch({ channel: "chromium", headless: true, args: gpuArgs });
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));
page.on("console", (m) => { if (m.type() === "error") errors.push(m.text()); });
try {
  await page.goto(base + "?fps=off" + extra);
  await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 60_000 });
  // FIFO queue: screenshots go through screenshot() so they land behind the preceding wait/camera.
  for (const c of script) {
    if (c.cmd === "exit") continue;
    if (c.cmd === "screenshot") {
      const url = await page.evaluate((n) => window.hexWorld.screenshot(n, 60_000), c.name);
      const file = join(outDir, `web-${c.name}.png`);
      await writeFile(file, Buffer.from(url.replace(/^data:image\/png;base64,/, ""), "base64"));
      console.log("wrote", file);
    } else {
      await page.evaluate((j) => window.hexWorld.cmd(j), JSON.stringify(c));
    }
  }
  console.log("state", JSON.stringify(await page.evaluate(() => window.hexWorld.dumpState())));
} catch (e) {
  fail(String(e));
} finally {
  await browser.close();
  server.close();
}
if (errors.length) fail("console errors: " + errors.join(" | "));
console.log(process.exitCode ? "web-shots FAILED" : "web-shots OK");
