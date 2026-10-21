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
// Paths only: fetches carry a content-hash query (?v=, scripts/web/version-dist.mjs).
page.on("request", (r) => { const p = new URL(r.url()).pathname; if (p.includes("/assets/audio/")) audioRequests.push(p); });

const audio = () => page.evaluate(() => window.hexWorld.audio.state());
const play = () => page.evaluate(() => window.hexWorld.state().play);
const save = (name, v) => writeFileSync(`${out}/${name}.json`, JSON.stringify(v, null, 1));
const playCue = (name) => page.evaluate((n) => window.hexWorld.audio.play(n), name);
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
  const manifest = JSON.parse(readFileSync(join(dist, "assets/audio/manifest.json"), "utf8"));
  const decodeCues = manifest.cues.filter((c) => c.status === "mastered" && c.playback === "decode" && c.files?.webm);
  check(a.cues.loaded.length === decodeCues.length,
    `${decodeCues.length} decoded cues loaded (${a.cues.loaded.join(", ")})`);
  check(a.cues.failed.length === 0 && a.cues.skipped.length === 0, `no failed or skipped cues ${JSON.stringify([a.cues.failed, a.cues.skipped])}`);
  // Gapless: each loop decodes in Chromium to exactly its manifest loop length, so the loop points
  // the runtime sets are the seam make audio-check verified on the ffmpeg decode.
  for (const c of manifest.cues.filter((c) => c.loop && c.status === "mastered" && c.playback === "decode"))
    check(a.cues.frames[c.id] === c.loop.endSample,
      `${c.id} decodes to its loop length (${a.cues.frames[c.id]} frames, manifest ${c.loop.endSample})`);
  check(a.decodedBytes > 0 && a.decodedBytes <= a.decodedBudgetBytes, `decoded ${a.decodedBytes} B within ${a.decodedBudgetBytes} B`);
  // Music streams through a media element. The start meadow picks meadow-day, which fades in after the gesture.
  const streamed = manifest.cues.filter((c) => c.status === "mastered" && c.playback === "stream").map((c) => c.id);
  check(a.music.tracks === streamed.length && ["meadow-day", "coast-breeze", "hearth-town", "photo-mode"].every((id) => streamed.includes(id)),
    `${streamed.length} music cues stream (${a.cues.streams.join(", ")})`);
  const track = async (id) => (await audio()).music.gains.find((g) => g.id === id) ?? null;
  const streaming = (id) => until(async () => {
    const g = await track(id);
    return g && !g.paused && g.time > 0 ? g : null;
  }, `${id} streams: its media element plays`, 10_000);
  await until(async () => (await audio()).music.current === "meadow-day", "the start meadow picks meadow-day");
  const m0 = await streaming("meadow-day");
  await page.waitForTimeout(1500);
  const m1 = await track("meadow-day");
  check(m0 && m1 && m1.gain > m0.gain && m1.time > m0.time,
    `meadow-day's gain rises (${m0?.gain} -> ${m1?.gain}) while it plays (${m0?.time} -> ${m1?.time} s)`);
  check(audioRequests.some((u) => /\/assets\/audio\/music\/meadow-day\.webm$/.test(u)), "meadow-day.webm is fetched after the gesture");
  check(!audioRequests.some((u) => /\/music\/(title-theme|photo-mode|coast-breeze|hearth-town)\./.test(u)),
    "no music the mix does not ask for is fetched");
  // Photo mode (D7) switches to its own cue at once and back to the mood's pick when it closes.
  await cmd({ cmd: "toggle", what: "photo" });
  await until(async () => (await play()).photo_mode === true, "photo mode opens");
  await until(async () => (await audio()).music.current === "photo-mode", "photo mode plays photo-mode");
  const p0 = await streaming("photo-mode");
  await page.waitForTimeout(1500);
  const p1 = await track("photo-mode");
  check(p0 && p1 && p1.gain > p0.gain && p1.time > p0.time,
    `photo-mode's gain rises (${p0?.gain} -> ${p1?.gain}) while it plays (${p0?.time} -> ${p1?.time} s)`);
  save("music", { meadow: [m0, m1], photo: [p0, p1], state: await audio() });
  await cmd({ cmd: "toggle", what: "photo" });
  await until(async () => (await play()).photo_mode === false, "photo mode closes");
  await until(async () => (await audio()).music.current === "meadow-day", "closing photo mode returns to meadow-day");
  check(JSON.stringify(a.missingEmitters) === '["town-murmur"]', `only town murmur missing (${a.missingEmitters})`);
  check(audioRequests.every((u) => !/\.m4a$/.test(u)), "no m4a fetched when WebM plays");
  // The start meadow has no bed: an on-demand forest-birds play is what reaches the output. The start world
  // has no forest tile, so it may play non-positional (at null).
  const birds = manifest.cues.filter((c) => c.pool === "forest-birds").map((c) => c.id);
  let t0 = Date.now();
  const played = await playCue("forest-birds");
  check(birds.includes(played.played) && played.pool === "forest-birds"
    && (played.at === null || (Number.isInteger(played.at.q) && Number.isInteger(played.at.r))),
    `play("forest-birds") plays one of its variants ${JSON.stringify(played)}`);
  await until(async () => (await audio()).outputRms > 1e-4, "the on-demand birds reach the output", 2000 - (Date.now() - t0));
  a = await audio();
  save("play", { result: played, state: a });
  check(a.oneShots?.["forest-birds"]?.lastCue === played.played && a.oneShots["forest-birds"].plays === 1,
    `the on-demand play counts in its row ${JSON.stringify(a.oneShots?.["forest-birds"])}`);
  const unknown = await playCue("no-such-cue");
  check(unknown.played === null && unknown.reason === "unknown-cue", `play("no-such-cue") is refused ${JSON.stringify(unknown)}`);
  // The occasional bee swarm is armed 45-120 s out, with the start meadow as a flower tile; the forest
  // birds and leaves are armed too.
  check(a.bees?.cue === "meadow-bees" && a.bees.nextAt > a.currentTime && a.bees.nextAt <= a.currentTime + 120
    && a.bees.flowerTiles >= 1 && a.bees.plays === 0, `meadow-bees armed ${JSON.stringify(a.bees)}`);
  for (const name of ["forest-birds", "forest-leaves"])
    check(a.oneShots?.[name]?.nextAt > a.currentTime, `${name} armed ${JSON.stringify(a.oneShots?.[name])}`);

  // Placement and UI cues from the manifest's event lists.
  await page.evaluate(() => window.hexWorld.selectTile("road_straight"));
  await fired("ui-select", "tileSelected");
  await pointAt(1, 0);
  await page.keyboard.press("r");
  await fired("ui-rotate", "ghostRotated");
  await page.mouse.down();
  await page.mouse.up();
  await until(async () => same((await play()).pinned, 1, 0), "the first click pins (1, 0)");
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
  const sloshes = manifest.cues.filter((c) => c.pool === "place-slosh").map((c) => c.id);
  check(sloshes.length >= 2 && JSON.stringify(a.placeSlosh?.variants) === JSON.stringify(sloshes)
    && a.placeSlosh.plays === 0, `land placements play no slosh ${JSON.stringify(a.placeSlosh)}`);

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
  await until(async () => (await audio()).placeSlosh?.plays >= 1, "a river tile sloshes as it settles", 6_000);
  a = await audio();
  check(sloshes.includes(a.placeSlosh.lastCue) && a.placeSlosh.plays <= river.cells.length,
    `the water placements play slosh variants ${JSON.stringify(a.placeSlosh)}`);
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

  // Cars (owner request 2026-10-14): a new world with the cars checkpoint's two connected house clusters. A
  // seeded departure pulls a car out about 10 s after both connect; its pull-out plays car-start at the spot, its
  // pass-by car-drive at the car, and its park car-door, all within three hex steps of the focus.
  await cmd({ cmd: "new_world" });
  await until(async () => (await tiles()) === 1, "a new world");
  const script = JSON.parse(readFileSync("docs/checkpoints/cars/cars.json", "utf8"));
  const build = script.slice(0, script.findIndex((c) => c.cmd === "dump_state"))
    .filter((c) => ["select_tile", "place", "hover", "rotate"].includes(c.cmd));
  await cmd(build);
  await until(async () => (await play()).connected_destinations === 2, "two house clusters connected", 20_000);
  await cmd({ cmd: "camera", pos: [14, 80, 100], look_at: [14, 0, 4] });
  const pools = Object.fromEntries(["car-start", "car-door", "car-drive"]
    .map((p) => [p, manifest.cues.filter((c) => c.pool === p && c.status === "mastered").map((c) => c.id)]));
  a = await audio();
  check(JSON.stringify(a.cars.pools) === JSON.stringify(pools), `car pools from the manifest ${JSON.stringify(a.cars.pools)}`);
  let maxVoices = 0;
  const carPlay = (event, pool, ms) => until(async () => {
    const s = await audio();
    maxVoices = Math.max(maxVoices, s.cars.voices.length);
    return s.cars.recent.find((f) => f.event === event && pools[pool].includes(f.id)) ?? null;
  }, `${event} plays ${pool}`, ms);
  const pulled = await carPlay("carPulledOut", "car-start", 60_000);
  const drove = await carPlay("carDriving", "car-drive", 30_000);
  const parked = await carPlay("carParked", "car-door", 90_000);
  a = await audio();
  save("cars", { pulled, drove, parked, maxVoices, cars: a.cars, play: await play() });
  check(pulled && drove && parked && pulled.car === drove.car && pulled.t < drove.t,
    `one car's pull-out, then its pass-by ${JSON.stringify([pulled, drove])}`);
  for (const f of [pulled, drove, parked].filter(Boolean))
    check(f.gain > 0 && f.gain <= 0.4 && Math.abs(f.rate - 1) <= 0.05, `${f.id} at focus-mix gain ${f.gain}, rate ${f.rate}`);
  check(maxVoices <= 3, `never more than three car voices (${maxVoices})`);
  check(a.recent.every((f) => !f.id.startsWith("car-")), "car cues stay out of the event-cue history");

  // Trains (P3): a new world with the rail checkpoint's loop round the start meadow. Each train's first move is a
  // trainDeparted event, which blows train-whistle at its front; a moving train chuffs train-chuff where it runs.
  await cmd({ cmd: "new_world" });
  await until(async () => (await tiles()) === 1, "a new world for the trains");
  await cmd({ cmd: "camera", pos: [18, 62, 78], look_at: [0, 0, 0] });
  const rail = JSON.parse(readFileSync("docs/checkpoints/rail/rail-play.json", "utf8"));
  // The loop's placements: from the first tile selection that places, to the next hover.
  const start = rail.findIndex((c, i) => c.cmd === "select_tile" && rail[i + 1]?.cmd === "place");
  await cmd(rail.slice(start, rail.findIndex((c, i) => i > start && c.cmd === "hover")));
  const trainPools = Object.fromEntries(["train-whistle", "train-chuff", "buffer-clunk"]
    .map((p) => [p, manifest.cues.filter((c) => c.pool === p && c.status === "mastered").map((c) => c.id)]));
  a = await audio();
  check(JSON.stringify(a.trains.pools) === JSON.stringify(trainPools), `train pools from the manifest ${JSON.stringify(a.trains.pools)}`);
  await until(async () => (await play()).trains >= 2, "the loop has its trains", 60_000);
  const trainPlay = (event, pool, ms) => until(async () => {
    const s = await audio();
    return s.trains.recent.find((f) => f.event === event && trainPools[pool].includes(f.id)) ?? null;
  }, `${event} plays ${pool}`, ms);
  const whistled = await trainPlay("trainDeparted", "train-whistle", 30_000);
  const chuffed = await trainPlay("trainMoving", "train-chuff", 30_000);
  a = await audio();
  save("trains", { whistled, chuffed, trains: a.trains, train_list: (await play()).train_list });
  for (const f of [whistled, chuffed].filter(Boolean))
    check(f.gain > 0 && f.gain <= 0.45, `${f.id} at focus-mix gain ${f.gain}`);
  check(a.trains.voices.length <= 3, `never more than three train voices (${a.trains.voices.length})`);
  check(a.recent.every((f) => !f.id.startsWith("train-")), "train cues stay out of the event-cue history");

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
  // A play, so the check doesn't hang on what the restored world and camera happen to make audible.
  t0 = Date.now();
  const back = await playCue("forest-birds");
  check(back.played !== null, `play after unmute ${JSON.stringify(back)}`);
  await until(async () => (await audio()).outputRms > 1e-4, "unmute brings the output back", 2000 - (Date.now() - t0));
} catch (e) {
  fail(String(e?.stack ?? e));
} finally {
  if (errors.length) fail("console errors: " + errors.join(" | "));
  await browser.close();
  server.close();
}
console.log(failures ? `audio e2e FAILED (${failures})` : `audio e2e OK: state dumps in ${out}/`);
process.exitCode = failures ? 1 : 0;
