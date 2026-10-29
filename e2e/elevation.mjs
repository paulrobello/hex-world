// Elevation web smoke (P4 first round) on the built site in GPU Chromium: places the world of
// docs/checkpoints/elevation/smoke.json (a level-1 plateau ringed by hillsides, a two-portal tunnel
// under a raised cover, a house at each end of the tunnel road) through the bridge, then asserts from
// `window.hexWorld.state().play`, within 60 s of the build settling, that
//   1. the levels are present: ground tiles and tiles on level 1 (the plateau and the tunnel cover),
//   2. a complete tunnel exists: a road_tunnel_portal at each end and a level-1 cover between them,
//   3. a car with opacity 0 (`car_list[].opacity`) is inside it, near the cover.
// Exits 0 only when all three hold and the page logged no error. Not a timing run, so it takes no
// load guard. Run after `make build-web`: node e2e/elevation.mjs dist
import { chromium } from "playwright";
import { createServer } from "node:http";
import { readFileSync } from "node:fs";
import { extname, join } from "node:path";
import { BAD_TEXT } from "./console-errors.mjs";

const dist = process.argv[2] ?? "dist";
const HEX_RADIUS = 16;
const PORTALS = [{ q: 2, r: 2 }, { q: 4, r: 2 }];
const COVER = { q: 3, r: 2 };
const SETTLE_MS = 120_000;
const ASSERT_MS = 60_000;
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
const script = JSON.parse(readFileSync("docs/checkpoints/elevation/smoke.json", "utf8"));

// The cover's world centre, as hex-core Axial::to_world (pointy-top, radius 16 m).
const centre = (c) => [HEX_RADIUS * Math.sqrt(3) * (c.q + c.r / 2), HEX_RADIUS * 1.5 * c.r];

const browser = await chromium.launch({ channel: "chromium", headless: true, args: gpuArgs });
const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } });
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));
page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });
let exit = 1;
try {
  await page.goto(base + "?scenario=play&tier=high");
  await page.waitForFunction(() => window.hexWorld?.ready() === true, null, { timeout: 120_000 });
  await page.evaluate(() => window.hexWorld.cmd({ cmd: "set_reduced_motion", on: true }));
  await page.evaluate(() => window.hexWorld.cmd({ cmd: "tilt", on: false, strength: 0 }));
  for (const c of script) await page.evaluate((c) => window.hexWorld.cmd(c), c);

  const want = script.filter((c) => c.cmd === "place").length;
  const settled = () => page.evaluate(() => {
    const s = window.hexWorld.state();
    return s.queued === 0 && (s.play?.rising ?? []).length === 0 && (s.play?.tiles ?? []).length > 0;
  });
  for (const d = Date.now() + SETTLE_MS; !(await settled()) && Date.now() < d;) await page.waitForTimeout(250);
  const tiles = await page.evaluate(() => window.hexWorld.state().play?.tiles ?? []);
  // The script's placements plus the start tile every play world opens with.
  if (tiles.length !== want + 1) fail(`world has ${tiles.length} tiles, the script places ${want} plus the start tile`);

  const at = (c) => tiles.find((t) => t.q === c.q && t.r === c.r);
  const levels = [...new Set(tiles.map((t) => t.level))].sort();
  if (!(levels.includes(0) && levels.includes(1))) fail(`levels present ${JSON.stringify(levels)}, want 0 and 1`);
  console.log("levels", JSON.stringify(levels), "tiles", tiles.length);

  const portals = PORTALS.map(at);
  const cover = at(COVER);
  const complete = portals.every((p) => p?.tile === "road_tunnel_portal")
    && cover !== undefined && portals.every((p) => cover.level > p.level);
  if (!complete) fail(`tunnel incomplete: portals ${JSON.stringify(portals)} cover ${JSON.stringify(cover)}`);
  else console.log("tunnel complete: portals", PORTALS.map((p) => `${p.q},${p.r}`).join(" and "), "cover level", cover.level);

  // A car inside the tunnel is faded out (opacity 0) within reach of the cover.
  const [cx, cz] = centre(COVER);
  const found = async () => (await page.evaluate(() => window.hexWorld.state().play?.car_list ?? []))
    .find((c) => c.opacity === 0 && Math.hypot(c.x - cx, c.z - cz) <= 2 * HEX_RADIUS);
  let car;
  for (const d = Date.now() + ASSERT_MS; !(car = await found()) && Date.now() < d;) await page.waitForTimeout(250);
  if (!car) fail(`no car with opacity 0 inside the tunnel within ${ASSERT_MS / 1000} s`);
  else console.log("car in the tunnel", JSON.stringify(car));

  // The HUD "Level N" readout (web only: the native shots draw no HUD). A hillside ghost on a free
  // slot beside the plateau, turned until it stands on a level above the ground, shows the tag.
  // Evidence, not an assertion: a missing tag warns.
  const slots = await page.evaluate(() => window.hexWorld.state().play?.frontier ?? []);
  await page.evaluate(() => window.hexWorld.cmd({ cmd: "select_tile", id: "hillside_000011" }));
  let tagged = false;
  for (const c of slots.slice(0, 16)) {
    await page.evaluate((c) => window.hexWorld.cmd({ cmd: "hover", q: c.q, r: c.r }), c);
    for (let i = 0; i < 12 && !tagged; i++) {
      await page.waitForTimeout(150);
      const g = await page.evaluate(() => window.hexWorld.state().play?.ghost);
      if (process.env.DEBUG_GHOST) console.log(JSON.stringify({ c, g: g && { tile: g.tile, level: g.level, valid: g.valid, legal: g.legal_rotations } }));
      if (g?.valid && g.level > 0 && (await page.getByText(`Level ${g.level}`).count()) > 0) tagged = true;
      else await page.evaluate(() => window.hexWorld.cmd({ cmd: "rotate", dir: "cw" }));
    }
    if (tagged) break;
  }
  if (tagged) {
    await page.screenshot({ path: "docs/checkpoints/elevation/hud-level.jpg", type: "jpeg", quality: 88 });
    console.log("HUD Level readout captured: docs/checkpoints/elevation/hud-level.jpg");
  } else {
    console.warn("WARN: no ghost stood on a level above the ground, so no HUD Level frame was captured");
  }

  if (errors.length > 0) fail(`page errors: ${errors.slice(0, 5).join(" | ")}`);
  exit = failures === 0 ? 0 : 1;
} catch (e) {
  fail(String(e?.stack ?? e));
} finally {
  await browser.close();
  server.close();
}
console.log(exit === 0 ? "elevation smoke OK" : "elevation smoke FAILED");
process.exit(exit);
