// Buffer stop evidence (owner D2, 2026-10-20) on the built site in GPU Chromium: a line of two
// straights ending in a bend, placed through the bridge, then the train standing at each buffer
// stop (the straight end in the west, the bend end in the east), and the placement ghost of a lone
// straight with a stop at each of its open ends. Writes docs/checkpoints/rail_buffer_stop/
// runtime-{straight,bend,ghost}.jpg and runtime-web.json (each capture's train_list). The web twin
// of a native script, for when native windows cannot draw (an idle or occluded display).
// Run: node e2e/rail-buffer-stop-shots.mjs dist
import { chromium } from "playwright";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { extname, join } from "node:path";
import { BAD_TEXT } from "./console-errors.mjs";

const dist = process.argv[2] ?? "dist";
const out = "docs/checkpoints/rail_buffer_stop";
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
// A line of two straights on row r = -1 ending in a bend at (2, -1), beside the start tile. Each
// end's stop pivot (hex_core::rail_end, tile frame) in world space: the straight's edge 3 at
// (0, -1), and the bend's open end at the rotation its ghost picked.
const BEND = { q: 2, r: -1 };
const build = [
  { cmd: "select_tile", id: "rail_straight" },
  { cmd: "place", q: 0, r: -1 },
  { cmd: "place", q: 1, r: -1 },
  { cmd: "select_tile", id: "rail_bend_60" },
  { cmd: "hover", ...BEND },
];
const centre = (q, r) => [16 * Math.sqrt(3) * (q + r / 2), 24 * r];
const turn = ([x, z], k) => {
  const a = (Math.PI / 3) * k;
  return [x * Math.cos(a) - z * Math.sin(a), x * Math.sin(a) + z * Math.cos(a)];
};
const add = (a, b) => [a[0] + b[0], a[1] + b[1]];
const frame = (at) => ({ cmd: "camera", pos: [at[0] + 10, 13, at[1] + 20], look_at: [at[0] - 2, 1.5, at[1]] });
let ENDS = {};

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
// Rotation k puts the bend's local edges 0 and 1 on world edges k and k + 1; the one not on world
// edge 3 (toward the straights) is open: local 0 at (10.467, 0.753) or local 1 at (5.886, 8.688).
let k;
for (const deadline = Date.now() + 10_000; k === undefined && Date.now() < deadline;) {
  await page.waitForTimeout(200);
  k = await page.evaluate((b) => {
    const g = window.hexWorld.state().play?.ghost;
    return g && g.q === b.q && g.r === b.r ? g.rotation : undefined;
  }, BEND);
}
if (k !== 2 && k !== 3) fail(`bend ghost rotation ${k}`);
await page.evaluate((b) => window.hexWorld.cmd({ cmd: "place", ...b }), BEND);
const bendAt = add(centre(BEND.q, BEND.r), turn(k === 2 ? [10.467, 0.753] : [5.886, 8.688], k));
const straightAt = add(centre(0, -1), [-10.356, 0]);
ENDS = { straight: { at: straightAt, camera: frame(straightAt) }, bend: { at: bendAt, camera: frame(bendAt) } };
console.log("ends", JSON.stringify({ k, straightAt, bendAt }));
await page.keyboard.press("Tab");

const state = () => page.evaluate(() => {
  const s = window.hexWorld.state();
  return { tiles: s.play?.tiles ?? s.tiles, trains: s.play?.train_list ?? [] };
});
const trains = async () => (await state()).trains;
const settled = () => page.evaluate(() => {
  const s = window.hexWorld.state();
  return Object.values(s.packs ?? {}).every((v) => v === "ready") && (s.play?.rising ?? []).length === 0;
});
const record = {};
async function shot(name) {
  await page.screenshot({ path: `${out}/${name}.jpg`, type: "jpeg", quality: 88 });
  record[name] = await trains();
  console.log(name, JSON.stringify(record[name]));
}
const near = (t) => Object.keys(ENDS).reduce((a, b) =>
  Math.hypot(t.x - ENDS[a].at[0], t.z - ENDS[a].at[1])
    <= Math.hypot(t.x - ENDS[b].at[0], t.z - ENDS[b].at[1]) ? a : b);

for (const deadline = Date.now() + 120_000; !(await settled()) && Date.now() < deadline;) await page.waitForTimeout(250);
await page.evaluate(() => window.hexWorld.cmd({ cmd: "hover", q: 8, r: -8 }));
for (const deadline = Date.now() + 60_000; (await trains()).length === 0 && Date.now() < deadline;) await page.waitForTimeout(250);
if ((await trains()).length !== 1) fail(`want one train, have ${JSON.stringify(await trains())}`);

// Follow the train: frame the end it heads for, and shoot when it stands at that end's stop.
const left = new Set(Object.keys(ENDS));
let framed = null;
for (const deadline = Date.now() + 240_000; left.size > 0 && Date.now() < deadline;) {
  const [t] = await trains();
  if (!t) break;
  const end = near(t);
  if (end !== framed && t.waiting !== "line_end") {
    await page.evaluate((c) => window.hexWorld.cmd(c), ENDS[end].camera);
    await page.evaluate(() => window.hexWorld.cmd({ cmd: "hover", q: 8, r: -8 }));
    framed = end;
  }
  if (t.waiting === "line_end" && end === framed && left.has(end)) {
    await page.waitForTimeout(300);
    await shot(`runtime-${end}`);
    left.delete(end);
  }
  await page.waitForTimeout(100);
}
for (const k of left) fail(`the train never stood at the ${k} end`);

// The ghost of a straight south-west of the start tile at (-1, 1): a stop at each open end.
await page.evaluate(() => window.hexWorld.cmd({ cmd: "select_tile", id: "rail_straight" }));
await page.evaluate(() => window.hexWorld.cmd({ cmd: "camera", pos: [-2, 30, 52], look_at: [-14, 0, 24] }));
await page.evaluate(() => window.hexWorld.cmd({ cmd: "hover", q: -1, r: 1 }));
await page.waitForTimeout(800);
const ghost = await page.evaluate(() => window.hexWorld.state().play?.ghost ?? null);
record["runtime-ghost"] = ghost;
await page.screenshot({ path: `${out}/runtime-ghost.jpg`, type: "jpeg", quality: 88 });
console.log("runtime-ghost", JSON.stringify(ghost));

if (errors.length) fail(`console errors: ${errors.slice(0, 3).join(" | ")}`);
writeFileSync(`${out}/runtime-web.json`, `${JSON.stringify(record, null, 2)}\n`);
await browser.close();
server.close();
if (failures) process.exit(1);
console.log(`wrote ${out}/runtime-{straight,bend,ghost}.jpg`);
