// Boats evidence (owner D4) on the built site in GPU Chromium: loads
// docs/checkpoints/boats-play/boats.hexworld (a river through a road bridge into a river mouth, and
// the sea beyond: `boats::tests::checkpoint_world`, BOATS_SAVE writes it), then captures play
// distance over the river and the sea and a close-up of each boat: the rowing boat moored at its
// landing stage, a rowing boat under way, a sailing boat under way and the one at anchor. Checks
// that three boats of each kind are drawn, that the moving ones move between captures, and that
// oar strokes reach the event stream. Writes docs/checkpoints/boats-play/*.jpg and boats-web.json.
// The web twin of a native script, for when native windows cannot draw (an occluded display).
// Run: node e2e/boats-shots.mjs dist
import { chromium } from "playwright";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { extname, join } from "node:path";
import { BAD_TEXT } from "./console-errors.mjs";

const dist = process.argv[2] ?? "dist";
const out = "docs/checkpoints/boats-play";
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

const browser = await chromium.launch({ channel: "chromium", headless: true, args: gpuArgs });
const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } });
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));
page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });
await page.goto(base + "?scenario=play&tier=high");
await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 120_000 });
const bytes = Array.from(readFileSync(`${out}/boats.hexworld`));
const loaded = await page.evaluate((b) => window.hexWorld.load(new Uint8Array(b)), bytes);
if (!loaded.ok) throw new Error(`load refused: ${JSON.stringify(loaded)}`);
await page.evaluate(() => window.hexWorld.cmd({ cmd: "tilt", on: false, strength: 0 }));
await page.keyboard.press("Tab");

const boats = () => page.evaluate(() => window.hexWorld.state().play?.boats ?? null);
const ready = () => page.evaluate(() => {
  const s = window.hexWorld.state();
  return Object.values(s.packs ?? {}).every((v) => v === "ready") && (s.play?.boats?.list?.length ?? 0) > 0;
});
for (const deadline = Date.now() + 180_000; !(await ready()) && Date.now() < deadline;) await page.waitForTimeout(250);
await page.evaluate(() => window.hexWorld.events());
const strokes = [];
const flaps = [];
const drain = async () => {
  for (const e of await page.evaluate(() => window.hexWorld.events())) {
    if (e.type === "boatStroke") strokes.push(e);
    if (e.type === "sailFlap") flaps.push(e);
  }
};
const record = {};
async function shot(name, pos, look) {
  await page.evaluate(([pos, look]) => window.hexWorld.cmd({ cmd: "camera", pos, look_at: look }), [pos, look]);
  await page.evaluate(() => window.hexWorld.cmd({ cmd: "hover", q: 30, r: -30 }));
  await page.waitForTimeout(1200);
  await page.screenshot({ path: `${out}/${name}.jpg`, type: "jpeg", quality: 88 });
  record[name] = await boats();
  await drain();
  console.log(name, JSON.stringify(record[name]?.list?.map((b) => [b.kind, b.state, b.pos])));
}

const first = await boats();
const count = (b, k) => b.list.filter((x) => x.kind === k).length;
if (count(first, "rowing") !== 3 || count(first, "sailing") !== 3) fail(`want 3 + 3 boats: ${JSON.stringify(first)}`);
const W = -1.4;
await shot("boats-river-play", [60, 70, 95], [60, 0, 24]);
await shot("boats-sea-play", [185, 85, 120], [185, 0, 30]);
await shot("boats-overview", [110, 160, 175], [110, 0, 30]);
const now = await boats();
const pick = (kind, state) => now.list.find((b) => b.kind === kind && b.state === state);
const close = async (name, b, back, up) => {
  if (!b) return fail(`no ${name} boat`);
  const [hx, hz] = b.heading;
  // A moving boat goes on while the camera settles (1.2 s): frame where it will be.
  const lead = b.state === "moving" ? (b.kind === "sailing" ? 2.4 : 1.1) * 1.5 : 0;
  const [x, z] = [b.pos[0] + hx * lead, b.pos[1] + hz * lead];
  // From abeam and a little astern, on the side away from the bank.
  const side = [-hz, hx];
  await shot(name, [x + side[0] * back - hx * back * 0.5, W + up, z + side[1] * back - hz * back * 0.5], [x, W + 0.3, z]);
};
await close("boats-rowing-moored-close", pick("rowing", "moored"), 7, 3.5);
await close("boats-rowing-moving-close", pick("rowing", "moving"), 7, 3.5);
await close("boats-sailing-moving-close", pick("sailing", "moving"), 14, 5);
await close("boats-sailing-anchored-close", pick("sailing", "moored"), 12, 5);
const later = await boats();
for (const b of first.list) {
  const c = later.list.find((x) => x.id === b.id);
  const moved = c && Math.hypot(c.pos[0] - b.pos[0], c.pos[1] - b.pos[1]);
  if (b.state === "moving" && !(moved > 1)) fail(`moving boat ${b.id} did not move: ${moved}`);
  if (b.state === "moored" && moved > 1e-3) fail(`moored boat ${b.id} moved ${moved}`);
}
await drain();
if (strokes.length === 0) fail("no boatStroke event");
if (errors.length) fail(`console errors: ${errors.slice(0, 3).join(" | ")}`);
writeFileSync(`${out}/boats-web.json`, `${JSON.stringify({ strokes: strokes.length, flaps: flaps.length, boats: record }, null, 2)}\n`);
await browser.close();
server.close();
if (failures) process.exit(1);
console.log(`wrote ${out}: ${Object.keys(record).join(", ")}; ${strokes.length} strokes, ${flaps.length} flaps`);
