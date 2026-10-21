// Farm animal motion evidence (P3) on the built site in GPU Chromium: a farm among meadows, placed
// through the bridge; then a frame sequence of a walking cow (`animal_cow`, legs swinging, the
// gait phase advancing with the ground it covers), a grazing sheep (`animal_sheep_c`, head down,
// nibbling) and, when one walks in time, a grazing cow walking with its head lifted
// (`animal_cow_b`). The camera is set beside the animal from its `play.farm` position at the
// start of each sequence and held, and each frame records the animal's `play.farm` row (state,
// phase, walk, head). Writes docs/checkpoints/farm-motion/<seq>-<n>.jpg and farm-motion.json.
// Run: node e2e/farm-motion-shots.mjs dist
import { chromium } from "playwright";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { extname, join } from "node:path";
import { BAD_TEXT } from "./console-errors.mjs";

const dist = process.argv[2] ?? "dist";
const out = process.env.FARM_MOTION_OUT ?? "docs/checkpoints/farm-motion";
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
const build = [
  { cmd: "select_tile", id: "farm" },
  { cmd: "place", q: 1, r: 0 },
  { cmd: "select_tile", id: "meadow" },
  { cmd: "place", q: 1, r: -1 },
  { cmd: "place", q: 2, r: -1 },
  { cmd: "place", q: 0, r: 1 },
  { cmd: "place", q: 1, r: 1 },
  { cmd: "place", q: 2, r: 0 },
  { cmd: "hover", q: 8, r: -8 },
];

const browser = await chromium.launch({ channel: "chromium", headless: true, args: gpuArgs });
const page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));
page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });
await page.goto(base + "?scenario=play&tier=high");
await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 120_000 });
await page.evaluate(() => window.hexWorld.cmd({ cmd: "set_reduced_motion", on: true }));
await page.evaluate(() => window.hexWorld.cmd({ cmd: "tilt", on: false, strength: 0 }));
for (const c of build) await page.evaluate((c) => window.hexWorld.cmd(c), c);
await page.keyboard.press("Tab");

const herd = () => page.evaluate(() => window.hexWorld.state().play?.farm?.list ?? []);
const settled = () => page.evaluate(() => {
  const s = window.hexWorld.state();
  return Object.values(s.packs ?? {}).every((v) => v === "ready") && (s.play?.rising ?? []).length === 0;
});
for (const deadline = Date.now() + 120_000; !(await settled()) && Date.now() < deadline;) await page.waitForTimeout(250);
await page.evaluate(() => window.hexWorld.cmd({ cmd: "camera", pos: [-4, 26, 50], look_at: [27.7, 3, 0] }));
await page.waitForTimeout(1500);
await page.screenshot({ path: `${out}/farm-motion-play.jpg`, type: "jpeg", quality: 88 });

await page.keyboard.press("h");

const record = {};
// Waits for `piece` to hold `state` (and `ok(row)`) over two reads `settle` ms apart, then frames
// it from `dist` m off its flank (square to its heading `dir`) with the eye at `eye` m, looking
// `lead` m ahead along its heading (where a walker will be once the camera has eased there), and shoots `n` frames `every` ms apart.
async function sequence(name, piece, state, ok, { dist, eye, look, lead, n, every, wait, settle }) {
  let row;
  const find = async () => (await herd()).find((a) => a.piece === piece && a.state === state && ok(a));
  for (const deadline = Date.now() + wait; Date.now() < deadline;) {
    const a = await find();
    if (a) {
      await page.waitForTimeout(settle);
      const b = (await herd()).find((x) => x.slot === a.slot);
      if (b && b.state === state && ok(b)) {
        row = b;
        break;
      }
    }
    await page.waitForTimeout(150);
  }
  if (!row) return false;
  const [x, z] = row.pos;
  const way = row.dir;
  const at = [x + way[0] * lead, z + way[1] * lead];
  await page.evaluate((c) => window.hexWorld.cmd(c), {
    cmd: "camera", pos: [at[0] - way[1] * dist, eye, at[1] + way[0] * dist], look_at: [at[0], look, at[1]],
  });
  // The camera eases to a new pose over about 2 s.
  await page.waitForTimeout(2500);
  record[name] = [];
  for (let i = 0; i < n; i++) {
    await page.waitForTimeout(every);
    await page.screenshot({ path: `${out}/${name}-${i}.jpg`, type: "jpeg", quality: 88 });
    record[name].push((await herd()).find((a) => a.slot === row.slot));
  }
  console.log(name, JSON.stringify(record[name].map((a) => [a.state, a.phase, a.walk, a.head])));
  return true;
}

const walking = (a) => a.walk > 0.9;
if (!await sequence("cow-walk", "animal_cow", "walk", walking,
  { dist: 5, eye: 1.6, look: 0.8, lead: 1.4, n: 10, every: 150, wait: 300_000, settle: 800 })) {
  fail("the cow never walked at full gait");
}
if (!await sequence("sheep-graze", "animal_sheep_c", "graze", (a) => a.walk === 0,
  { dist: 2.4, eye: 0.9, look: 0.3, lead: 0.4, n: 10, every: 350, wait: 240_000, settle: 300 })) {
  fail("the grazing sheep never grazed");
}
await sequence("cow-b-walk", "animal_cow_b", "walk", (a) => walking(a) && a.head > 0.9,
  { dist: 5, eye: 1.6, look: 0.7, lead: 1.4, n: 6, every: 200, wait: 120_000, settle: 800 });

const moving = (rows) => rows && new Set(rows.map((a) => a.phase)).size > 1;
if (!moving(record["cow-walk"])) fail("the cow's gait phase did not advance across its frames");
if (errors.length) fail(`console errors: ${errors.slice(0, 3).join(" | ")}`);
writeFileSync(`${out}/farm-motion.json`, `${JSON.stringify(record, null, 2)}\n`);
await browser.close();
server.close();
if (failures) process.exit(1);
console.log(`wrote ${out}: ${Object.keys(record).join(", ")}`);
