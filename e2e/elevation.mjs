// Elevation web smoke (P4 first round) on the built site in GPU Chromium: places the world of
// docs/checkpoints/elevation/smoke.json (a level-1 plateau ringed by hillsides, a two-portal tunnel
// under a raised cover, a house at each end of the tunnel road) through the bridge, then asserts from
// `window.hexWorld.state().play`, within 60 s of the build settling, that
//   1. the levels are present: ground tiles and tiles on level 1 (the plateau and the tunnel cover),
//   2. a complete tunnel exists: a road_tunnel_portal at each end and a level-1 cover between them,
//   3. a car with its headlights fully up (`car_list[].lamps` 1, the level a car reports only
//      inside a tunnel) is near the cover.
//   4. a river_straight cannot go on the cell between a level-0 spring and a level-1 straight that face it
//      (play.fits refusal river_uphill, worded in the sheet).
// Exits 0 only when all of them hold and the page logged no error. Not a timing run, so it takes no
// load guard. Run after `make build-web`: node e2e/elevation.mjs dist (also run by `make ui-e2e`).
// ELEVATION_OUT (default e2e/test-results/elevation) is where the HUD frame goes:
// ELEVATION_OUT=docs/checkpoints/elevation refreshes the committed evidence.
// DEBUG_GHOST (any value) logs the ghost state of each probed cell while it looks for a level tag.
import { readFileSync } from "node:fs";
import { BAD_TEXT } from "./console-errors.mjs";
import { serveDist } from "./lib/harness.mjs";
import { launch } from "./lib/browser.mjs";

const dist = process.argv[2] ?? "dist";
const out = process.env.ELEVATION_OUT ?? "e2e/test-results/elevation";
const HEX_RADIUS = 16;
const PORTALS = [{ q: 2, r: 2 }, { q: 4, r: 2 }];
const COVER = { q: 3, r: 2 };
const SETTLE_MS = 120_000;
const ASSERT_MS = 60_000;
const { server, base } = await serveDist(dist);
let failures = 0;
const fail = (m) => { console.error("FAIL:", m); failures++; };
const script = JSON.parse(readFileSync("docs/checkpoints/elevation/smoke.json", "utf8"));

// The cover's world centre, as hex-core Axial::to_world (pointy-top, radius 16 m).
const centre = (c) => [HEX_RADIUS * Math.sqrt(3) * (c.q + c.r / 2), HEX_RADIUS * 1.5 * c.r];

const browser = await launch();
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

  // A car inside the tunnel has its headlights fully up (lamps 1) within reach of the cover.
  const [cx, cz] = centre(COVER);
  const found = async () => (await page.evaluate(() => window.hexWorld.state().play?.car_list ?? []))
    .find((c) => c.lamps === 1 && Math.hypot(c.x - cx, c.z - cz) <= 2 * HEX_RADIUS);
  let car;
  for (const d = Date.now() + ASSERT_MS; !(car = await found()) && Date.now() < d;) await page.waitForTimeout(250);
  if (!car) fail(`no car with lamps 1 inside the tunnel within ${ASSERT_MS / 1000} s`);
  else console.log("car in the tunnel", JSON.stringify(car));

  // The HUD "Level N" readout (web only: the native shots draw no HUD). A hillside ghost on a free
  // slot beside the plateau, lifted (the `level` command, E) until it stands above the ground, shows the tag.
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
      else await page.evaluate(() => window.hexWorld.cmd({ cmd: "level", dir: "up" }));
    }
    if (tagged) break;
  }
  if (tagged) {
    await page.screenshot({ path: `${out}/hud-level.jpg`, type: "jpeg", quality: 88 });
    console.log(`HUD Level readout captured: ${out}/hud-level.jpg`);
  } else {
    console.warn("WARN: no ghost stood on a level above the ground, so no HUD Level frame was captured");
  }

  // Rivers cannot run uphill: on a fresh world, a level-0 river_spring and a level-1 river_straight
  // whose lanes face the same empty cell (the pinch, hex-core world.rs `pinch_world`) leave that
  // cell with no legal river_straight. Its `play.fits` entry carries refusal river_uphill and the
  // Tiles sheet words it. Both tiles lie along world edges 0 and 3 at rotation 0.
  await page.evaluate(() => window.hexWorld.cmd({ cmd: "new_world" }));
  await page.waitForFunction(() => (window.hexWorld.state().play?.tiles ?? []).length === 1, null, { timeout: 30_000 });
  const SPRING = { q: 1, r: 0 };
  const PINCH = { q: 2, r: 0 };
  const STRAIGHT = { q: 3, r: 0 };
  const put = async (id, c, level) => {
    await page.evaluate(([id, c, level]) => {
      window.hexWorld.cmd({ cmd: "select_tile", id });
      window.hexWorld.cmd({ cmd: "place", q: c.q, r: c.r, ...(level === undefined ? {} : { level }) });
    }, [id, c, level]);
    for (const d = Date.now() + 15_000; Date.now() < d;) {
      const t = (await page.evaluate(() => window.hexWorld.state().play?.tiles ?? [])).find((t) => t.q === c.q && t.r === c.r);
      if (t) return t;
      await page.waitForTimeout(150);
    }
    return undefined;
  };
  // Support meadows along r = 1 reach both ends of the pinch.
  for (const c of [{ q: 0, r: 1 }, { q: 1, r: 1 }, { q: 2, r: 1 }]) await put("meadow", c);
  const spring = await put("river_spring", SPRING);
  const straight = await put("river_straight", STRAIGHT, 1);
  if (spring?.level !== 0 || spring?.rotation !== 0) fail(`pinch spring not placed at level 0, rotation 0: ${JSON.stringify(spring)}`);
  else if (straight?.level !== 1 || straight?.rotation !== 0) fail(`pinch straight not placed at level 1, rotation 0: ${JSON.stringify(straight)}`);
  else {
    await page.evaluate((c) => {
      window.hexWorld.cmd({ cmd: "select_tile", id: "river_straight" });
      window.hexWorld.cmd({ cmd: "hover", q: c.q, r: c.r });
    }, PINCH);
    let entry;
    for (const d = Date.now() + 10_000; Date.now() < d;) {
      const fits = await page.evaluate(() => window.hexWorld.state().play?.fits);
      entry = fits?.entries?.find((e) => e.id === "river_straight");
      if (entry && entry.refusal) break;
      await page.waitForTimeout(150);
    }
    if (entry?.refusal !== "river_uphill") fail(`pinch fits entry for river_straight: ${JSON.stringify(entry)}, want refusal river_uphill`);
    else if ((await page.getByText("Rivers can't run uphill.").count()) === 0) fail("the Tiles sheet does not word river_uphill");
    else console.log("pinch refused: river_straight at", PINCH.q + "," + PINCH.r, "refusal", entry.refusal);
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
