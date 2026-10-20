// Train routing evidence (P3) on the built site in GPU Chromium: the world of
// docs/checkpoints/rail-routing/play.json (a turnout with a station off each leg, one train), placed
// through the bridge, then the train standing at each station in turn with that station as its
// destination. Writes docs/checkpoints/rail-routing/routing-{overview,east,north}.jpg and
// routing-web.json (each capture's train_list, read from hexWorld.state()). The web twin of
// play.sh, for when native windows cannot draw (an idle or occluded display).
// Run: node e2e/rail-routing-shots.mjs dist
import { chromium } from "playwright";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { extname, join } from "node:path";
import { BAD_TEXT } from "./console-errors.mjs";

const dist = process.argv[2] ?? "dist";
const out = "docs/checkpoints/rail-routing";
mkdirSync(out, { recursive: true });
const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css",
  ".woff2": "font/woff2", ".ktx2": "image/ktx2", ".glb": "model/gltf-binary", ".png": "image/png", ".webp": "image/webp" };
const server = createServer((req, res) => {
  let path = decodeURIComponent(new URL(req.url, "http://x").pathname);
  if (path.endsWith("/")) path += "index.html";
  try {
    const body = readFileSync(join(dist, path));
    res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" });
    res.end(body);
  } catch { res.writeHead(404); res.end(); }
}).listen(0);
const base = `http://127.0.0.1:${server.address().port}/`;
const gpuArgs = process.env.PW_GPU === "swiftshader"
  ? ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--enable-unsafe-webgpu"]
  : ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu"];

let failures = 0;
const fail = (m) => { console.error("FAIL:", m); failures++; };
const STATIONS = { east: { q: 1, r: -1 }, north: { q: 0, r: -2 } };
const script = JSON.parse(readFileSync(`${out}/play.json`, "utf8"));
// The placements: every command before the first camera move.
const build = script.slice(0, script.findIndex((c) => c.cmd === "camera"));
const view = script.filter((c) => c.cmd === "camera");

const browser = await chromium.launch({ channel: "chromium", headless: true, args: gpuArgs });
const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } });
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));
page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });
await page.goto(base + "?scenario=play&tier=high");
await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 120_000 });
await page.evaluate(() => window.hexWorld.cmd({ cmd: "set_reduced_motion", on: true }));
await page.evaluate(() => window.hexWorld.cmd({ cmd: "tilt", on: false, strength: 0 }));
for (const c of build) await page.evaluate((c) => window.hexWorld.cmd(c), c);
// Fold the tile sheet away (Tab), so the east station is not under it.
await page.keyboard.press("Tab");

const trains = () => page.evaluate(() => window.hexWorld.state().play?.train_list ?? []);
const settled = () => page.evaluate(() => {
  const s = window.hexWorld.state();
  return Object.values(s.packs ?? {}).every((v) => v === "ready") && (s.play?.rising ?? []).length === 0;
});
const record = {};
async function shot(name) {
  await page.waitForTimeout(400);
  await page.screenshot({ path: `${out}/${name}.jpg`, type: "jpeg", quality: 88 });
  record[name] = await trains();
  console.log(name, JSON.stringify(record[name]));
}

// The overview, once every pack has landed and the rises are done.
for (const deadline = Date.now() + 120_000; !(await settled()) && Date.now() < deadline;) await page.waitForTimeout(250);
await page.evaluate((c) => window.hexWorld.cmd(c), view[0]);
await page.evaluate(() => window.hexWorld.cmd({ cmd: "hover", q: 8, r: -8 }));
for (const deadline = Date.now() + 60_000; (await trains()).length === 0 && Date.now() < deadline;) await page.waitForTimeout(250);
await shot("routing-overview");
if ((await trains()).length !== 1) fail(`want one train, have ${JSON.stringify(await trains())}`);

// Both stations and the turnout; poll until the train stands at each station as its destination.
await page.evaluate((c) => window.hexWorld.cmd(c), view[1]);
await page.evaluate(() => window.hexWorld.cmd({ cmd: "hover", q: 8, r: -8 }));
const order = [];
const left = new Set(Object.keys(STATIONS));
for (const deadline = Date.now() + 300_000; left.size > 0 && Date.now() < deadline;) {
  const [t] = await trains();
  const at = t && t.waiting === "station" && t.speed === 0
    ? Object.keys(STATIONS).find((k) => t.destination?.q === STATIONS[k].q && t.destination?.r === STATIONS[k].r)
    : undefined;
  if (at && left.has(at)) {
    await shot(`routing-${at}`);
    left.delete(at);
    order.push(at);
  }
  await page.waitForTimeout(200);
}
for (const k of left) fail(`the train never stood at the ${k} station`);
if (errors.length) fail(`console errors: ${errors.slice(0, 3).join(" | ")}`);
writeFileSync(`${out}/routing-web.json`, `${JSON.stringify({ order, trains: record }, null, 2)}\n`);
await browser.close();
server.close();
if (failures) process.exit(1);
console.log(`wrote ${out}/routing-{overview,${order.join(",")}}.jpg`);
