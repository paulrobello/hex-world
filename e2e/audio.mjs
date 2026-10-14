// Local-only audio e2e (make audio-e2e, also run by make ui-e2e): GPU Chromium on the built site, the
// play scenario with the web/ui audio layer (PRD §8). Nothing here listens: every check reads
// hexWorld.audio.state(), which reports the context, loaded cues, the cues fired, emitter gains and the
// output level measured after the master gain. Writes e2e/test-results/audio/*.json as evidence.
import { chromium } from "playwright";
import { createServer } from "node:http";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { extname, join } from "node:path";
import { BAD_TEXT } from "./console-errors.mjs";

const dist = process.argv[2] ?? "dist";
const out = "e2e/test-results/audio";
mkdirSync(out, { recursive: true });
const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css",
  ".json": "application/json", ".woff2": "font/woff2", ".ktx2": "image/ktx2", ".glb": "model/gltf-binary",
  ".png": "image/png", ".webp": "image/webp", ".webm": "audio/webm", ".m4a": "audio/mp4" };
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
const check = (ok, m) => { if (!ok) fail(m); else console.log("ok:", m); };

// --mute-audio keeps unattended runs quiet; the graph still renders, so the analyser still measures.
const browser = await chromium.launch({ channel: "chromium", headless: true, args: [...gpuArgs, "--mute-audio"] });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
const errors = [];
page.on("pageerror", (e) => errors.push(String(e)));
page.on("console", (m) => { if (m.type() === "error" || BAD_TEXT.test(m.text())) errors.push(m.text()); });
let audioRequests = [];
page.on("request", (r) => { if (r.url().includes("/assets/audio/")) audioRequests.push(r.url()); });

const audio = () => page.evaluate(() => window.hexWorld.audio.state());
const play = () => page.evaluate(() => window.hexWorld.state().play);
const save = (name, v) => writeFileSync(`${out}/${name}.json`, JSON.stringify(v, null, 1));
async function until(fn, what, ms = 10_000) {
  const deadline = Date.now() + ms;
  for (;;) {
    const v = await fn();
    if (v) { console.log("ok:", what); return v; }
    if (Date.now() > deadline) { fail(`timed out: ${what}`); return v; }
    await page.waitForTimeout(50);
  }
}
const same = (c, q, r) => c && c.q === q && c.r === r;
async function pointAt(q, r) {
  const b = await page.evaluate(([q, r]) => window.hexWorld.cellRect(q, r), [q, r]);
  await page.mouse.move((b[0] + b[2]) / 2, (b[1] + b[3]) / 2, { steps: 4 });
  await until(async () => same((await play()).hovered, q, r), `pointer hovers (${q}, ${r})`);
}
/** Waits for `id` among the recently fired cues, fired by `event`. */
const fired = (id, event, ms) =>
  until(async () => (await audio()).recent.some((f) => f.id === id && f.event === event), `${event} fires ${id}`, ms);
const cmd = (c) => page.evaluate((c) => window.hexWorld.cmd(c), c);
const tiles = async () => (await play()).tiles.length;
async function open() {
  audioRequests = [];
  await page.goto(base + "?scenario=play&tilt=off&fps=off");
  await page.waitForFunction(() => window.hexWorld?.ready() && window.hexWorld.audio, null, { timeout: 90_000 });
  await until(async () => (await play())?.inventory_open === true, "play HUD up");
  await page.waitForTimeout(1500);
}
/** A press on the bar's world label: a gesture that changes nothing in the game. */
const gesture = () => page.locator(".bar__world").click();

try {
  // Nothing loads before the first interaction.
  await open();
  let a = await audio();
  check(a.context === "none" && a.loading === "idle", `no AudioContext before a gesture (${a.context}, ${a.loading})`);
  check(audioRequests.length === 0, `no /assets/audio/ requests before the first interaction (${audioRequests.length})`);

  // After a gesture the context runs and the manifest's cues load within the budget.
  await gesture();
  await until(async () => (await audio()).context === "running", "a gesture starts the AudioContext");
  await until(async () => (await audio()).loading === "done", "cues loaded", 20_000);
  a = await audio();
  save("loaded", a);
  check(a.format === "webm", `Opus/WebM picked in Chromium (${a.format})`);
  check(a.sampleRate === 48000, `context at the mastering rate (${a.sampleRate})`);
  check(a.cues.loaded.length === 12, `12 decoded cues loaded (${a.cues.loaded.join(", ")})`);
  check(a.cues.failed.length === 0 && a.cues.skipped.length === 0, `no failed or skipped cues ${JSON.stringify([a.cues.failed, a.cues.skipped])}`);
  // Gapless: each loop decodes in Chromium to exactly its manifest loop length, so the loop points
  // the runtime sets are the seam make audio-check verified on the ffmpeg decode.
  const manifest = JSON.parse(readFileSync(join(dist, "assets/audio/manifest.json"), "utf8"));
  for (const c of manifest.cues.filter((c) => c.loop && c.status === "mastered"))
    check(a.cues.frames[c.id] === c.loop.endSample,
      `${c.id} decodes to its loop length (${a.cues.frames[c.id]} frames, manifest ${c.loop.endSample})`);
  check(a.decodedBytes > 0 && a.decodedBytes <= a.decodedBudgetBytes, `decoded ${a.decodedBytes} B within ${a.decodedBudgetBytes} B`);
  check(a.music.tracks === 0 && a.music.current === null, "no music mastered: the playlist idles");
  check(JSON.stringify(a.missingEmitters) === '["town-murmur"]', `only town murmur missing (${a.missingEmitters})`);
  check(audioRequests.every((u) => !/\.m4a$/.test(u)), "no m4a fetched when WebM plays");
  await until(async () => ((await audio()).bed.find((b) => b.cue === "meadow-bed")?.gain ?? 0) > 0.3, "meadow bed up for the start meadow");
  await until(async () => (await audio()).outputRms > 1e-4, "the bed reaches the output");
  // The occasional bee swarm is armed 45-120 s out, with the start meadow as a flower tile.
  a = await audio();
  check(a.bees?.cue === "meadow-bees" && a.bees.nextAt > a.currentTime && a.bees.nextAt <= a.currentTime + 120
    && a.bees.flowerTiles >= 1 && a.bees.plays === 0, `meadow-bees armed ${JSON.stringify(a.bees)}`);

  // Placement and UI cues from the manifest's event lists.
  await page.evaluate(() => window.hexWorld.selectTile("road_straight"));
  await fired("ui-select", "tileSelected");
  await pointAt(1, 0);
  await page.keyboard.press("r");
  await fired("ui-rotate", "ghostRotated");
  await page.mouse.down();
  await page.mouse.up();
  await until(async () => (await tiles()) === 2, "road placed at (1, 0)");
  await fired("place-rumble", "tilePlaced");
  await fired("place-thunk", "tilePlaced", 6_000);
  a = await audio();
  const rumble = a.recent.find((f) => f.id === "place-rumble");
  const thunk = a.recent.find((f) => f.id === "place-thunk");
  check(thunk.t - rumble.t > 0.5, `the thunk waits for the settle (${(thunk.t - rumble.t).toFixed(2)} s after the rumble)`);
  await page.keyboard.press("ControlOrMeta+z");
  await fired("ui-undo", "undone");
  await page.waitForTimeout(200);
  await page.keyboard.press("ControlOrMeta+Shift+z");
  await fired("ui-undo", "redone");
  await page.evaluate(() => { window.hexWorld.selectTile("ocean"); window.hexWorld.placeTile(-1, 0); });
  await fired("ui-invalid", "placementRefused");
  await page.evaluate(() => { window.hexWorld.selectTile("meadow"); window.hexWorld.placeTile(2, 0); });
  await until(async () => (await tiles()) === 3, "meadow placed at (2, 0)");
  await cmd([{ cmd: "set_tool", mode: "bulldoze" }, { cmd: "bulldoze", q: 1, r: 0 }]);
  await fired("ui-invalid", "bulldozeRefused");
  await cmd([{ cmd: "bulldoze", q: 2, r: 0 }, { cmd: "set_tool", mode: "place" }]);
  await fired("ui-bulldoze", "tileBulldozed");
  a = await audio();
  check(a.lastCue?.id === "ui-bulldoze", `lastCue reports the latest cue (${a.lastCue?.id})`);
  save("events", a);

  // A river of six straights heading west, away from the start meadow.
  const river = await page.evaluate(async () => {
    const hw = window.hexWorld;
    const DIRS = [[1, 0], [0, 1], [-1, 1], [-1, 0], [0, -1], [1, -1]];
    const dist = (c) => Math.max(Math.abs(c.q), Math.abs(c.r), Math.abs(c.q + c.r));
    const at = (q, r) => hw.state().play.tiles.find((t) => t.q === q && t.r === r);
    const wait = async (q, r) => { for (let i = 0; i < 100 && !at(q, r); i++) await new Promise((f) => setTimeout(f, 30)); return at(q, r); };
    hw.selectTile("river_straight");
    hw.placeTile(-1, 0);
    const first = await wait(-1, 0);
    if (!first) return null;
    const k = first.rotation;
    const step = (c, e) => ({ q: c.q + DIRS[e][0], r: c.r + DIRS[e][1] });
    const e = dist(step(first, k)) > dist(step(first, (k + 3) % 6)) ? k : (k + 3) % 6;
    const cells = [{ q: -1, r: 0 }];
    for (let i = 0; i < 5; i++) {
      const c = step(cells.at(-1), e);
      hw.placeTile(c.q, c.r);
      if (!(await wait(c.q, c.r))) return { cells, stuck: c };
      cells.push(c);
    }
    return { cells };
  });
  check(river?.cells?.length === 6, `six river tiles placed ${JSON.stringify(river)}`);
  const near = river.cells[0];
  const end = river.cells.at(-1);
  const gainAt = (s, c) => s.emitters.find((e) => e.cue === "river-babble" && e.q === c.q && e.r === c.r)?.gain ?? 0;
  const world = (c) => [16 * Math.sqrt(3) * (c.q + c.r / 2), 16 * 1.5 * c.r];
  const lookAt = async (c, dist) => {
    const [x, z] = world(c);
    await cmd({ cmd: "camera", pos: [x, dist * 0.8, z + dist * 0.6], look_at: [x, 0, z] });
    await until(async () => Math.abs(((await play()).zoom ?? 0) - dist) < 1, `camera at ${dist} m over (${c.q}, ${c.r})`);
    await page.waitForTimeout(500);
    return audio();
  };

  // Zooming onto a river tile raises its emitter over a far one.
  const wide = await lookAt(end, 600);
  const zoomed = await lookAt(end, 30);
  save("river-zoom", { river, wide, zoomed });
  const gWide = gainAt(wide, end), gEnd = gainAt(zoomed, end), gNear = gainAt(zoomed, near);
  check(gEnd > gWide, `zooming in on (${end.q}, ${end.r}) raises its babble (${gWide} -> ${gEnd})`);
  check(gEnd > 0.5 && gEnd > 10 * gNear, `the zoomed river tile ${gEnd} outweighs the far one at (${near.q}, ${near.r}) ${gNear}`);
  check(zoomed.emitters.some((e) => e.voiced && e.q === end.q && e.r === end.r), "the zoomed tile has a playing voice");
  check(zoomed.mix.river > 0.5, `the biome mix in view turns to river (${zoomed.mix.river})`);
  await until(async () => (await audio()).outputRms > 1e-4, "ambience audible before mute");

  // Mute silences the output, and the setting persists for this viewer.
  await page.evaluate(() => window.hexWorld.audio.setMuted(true));
  await page.waitForTimeout(500);
  a = await audio();
  save("muted", a);
  check(a.muted && a.masterGain < 1e-4, `mute drops the master gain (${a.masterGain})`);
  check(a.outputRms < 1e-5, `muted output is silent (rms ${a.outputRms})`);
  await page.evaluate(() => window.hexWorld.audio.setVolume(0.6));
  await open();
  a = await audio();
  check(a.muted === true && a.volumes.master === 0.6, `mute and volume persist across a reload (${a.muted}, ${a.volumes.master})`);
  check(audioRequests.length === 0, "the reload loads no audio before a gesture either");
  await gesture();
  await until(async () => (await audio()).loading === "done", "cues loaded after the reload", 20_000);
  await page.waitForTimeout(800);
  a = await audio();
  check(a.outputRms < 1e-5 && a.masterGain < 1e-4, `still silent after the reload (rms ${a.outputRms})`);
  await page.evaluate(() => window.hexWorld.audio.setMuted(false));
  await until(async () => (await audio()).outputRms > 1e-4, "unmute brings the output back");
} catch (e) {
  fail(String(e?.stack ?? e));
} finally {
  if (errors.length) fail("console errors: " + errors.join(" | "));
  await browser.close();
  server.close();
}
console.log(failures ? `audio e2e FAILED (${failures})` : `audio e2e OK: state dumps in ${out}/`);
process.exitCode = failures ? 1 : 0;
