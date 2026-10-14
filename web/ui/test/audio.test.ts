// The audio layer's pure parts: cue selection from the real manifest, format fallback, the decoded
// budget, the focus-mix and attenuation math, and the music crossfade with synthetic cues.
import assert from "node:assert/strict";
import { test } from "node:test";
import manifestJson from "../../../assets/audio/manifest.json" with { type: "json" };
import { type Cue, type Manifest, busOf, eventCues, pickFormat, planDecode, playable } from "../src/audio-cues.ts";
import {
  BEE_CUE,
  BEE_DELAY_S,
  type FlowerTile,
  type Mix,
  type PlacedTile,
  type View,
  attenuation,
  beeDelay,
  beeGate,
  beeIdle,
  beeStep,
  bedGains,
  biomeMix,
  biomeOf,
  cellWorld,
  emitterGain,
  emptyMix,
  focusWeight,
  isFlowerTile,
  MUSIC_OPTS,
  musicGains,
  musicIdle,
  musicStep,
  pickBeeTile,
  pickEmitters,
} from "../src/audio-mix.ts";
import schema from "../src/bridge-schema.json" with { type: "json" };

const manifest = manifestJson as unknown as Manifest;
const R = schema.catalog.hex_radius;
const close = (a: number, b: number, eps = 1e-6) => assert.ok(Math.abs(a - b) < eps, `${a} != ${b}`);

// ---------- Cues ----------
test("events map to the manifest's mastered cues", () => {
  const map = eventCues(playable(manifest.cues, "webm"));
  assert.deepEqual(map.get("tilePlaced"), ["place-rumble", "place-thunk"]);
  assert.deepEqual(map.get("tileSelected"), ["ui-select"]);
  assert.deepEqual(map.get("ghostRotated"), ["ui-rotate"]);
  assert.deepEqual(map.get("placementRefused"), ["ui-invalid"]);
  assert.deepEqual(map.get("bulldozeRefused"), ["ui-invalid"]);
  assert.deepEqual(map.get("undone"), ["ui-undo"]);
  assert.deepEqual(map.get("redone"), ["ui-undo"]);
  assert.deepEqual(map.get("tileBulldozed"), ["ui-bulldoze"]);
  for (const e of ["hoverChanged", "toolChanged", "inventoryToggled", "photoModeToggled", "actionRefused", "riseStage"])
    assert.equal(map.get(e), undefined, `${e} has no cue`);
});

test("pending music sources are not playable", () => {
  const ids = playable(manifest.cues, "webm").map((c) => c.id);
  assert.ok(!ids.includes("meadow-day"));
  assert.ok(ids.includes("river-babble"));
  const missingM4a: Cue = { ...manifest.cues[0]!, id: "x", files: { webm: manifest.cues[0]!.files!.webm! } };
  assert.deepEqual(
    playable([missingM4a], "m4a").map((c) => c.id),
    [],
  );
});

test("cue classes route to the Legend buses", () => {
  const by = (id: string) => busOf(manifest.cues.find((c) => c.id === id)!);
  assert.equal(by("place-rumble"), "effects");
  assert.equal(by("ui-select"), "interface");
  assert.equal(by("surf"), "ambience");
  assert.equal(by("meadow-day"), "music");
});

test("format: Opus/WebM first, m4a when WebM can't play, null when neither can", () => {
  const can = (ok: Record<string, string>) => (mime: string) => ok[mime.split(";")[0]!] ?? "";
  assert.equal(pickFormat(["webm", "m4a"], can({ "audio/webm": "probably", "audio/mp4": "probably" })), "webm");
  assert.equal(pickFormat(["webm", "m4a"], can({ "audio/mp4": "maybe" })), "m4a");
  assert.equal(pickFormat(["webm", "m4a"], can({ "audio/webm": "maybe", "audio/mp4": "probably" })), "m4a");
  assert.equal(pickFormat(["webm", "m4a"], can({})), null);
});

test("decoded budget: the real manifest fits, one-shots load before loops, streams never count", () => {
  const cues = playable(manifest.cues, "webm");
  const plan = planDecode(cues, manifest.decodedBudgetBytes, manifest.sampleRate, manifest.sampleRate);
  assert.equal(plan.skipped.length, 0);
  assert.ok(plan.bytes <= manifest.decodedBudgetBytes);
  assert.equal(
    plan.bytes,
    cues.reduce((s, c) => s + (c.decodedBytes ?? 0), 0),
  );
  const order = plan.load.map((c) => c.class);
  assert.ok(order.lastIndexOf("ui") < order.indexOf("sfx") && order.lastIndexOf("sfx") < order.indexOf("loop"));

  const tight = planDecode(cues, 6_000_000, manifest.sampleRate, manifest.sampleRate);
  assert.ok(
    tight.load.every((c) => c.class !== "loop"),
    "no 5.76 MB loop fits after the one-shots",
  );
  assert.ok(tight.skipped.some((s) => s.id === "river-babble"));
  assert.ok(tight.bytes <= 6_000_000);
  // A 96 kHz context decodes twice the bytes, so fewer loops fit the same budget.
  const fast = planDecode(cues, 20_000_000, 48_000, 96_000);
  assert.ok(fast.load.length < planDecode(cues, 20_000_000, 48_000, 48_000).load.length);

  const stream: Cue = { id: "song", kind: "music", class: "music", status: "mastered", events: [], playback: "stream" };
  assert.deepEqual(planDecode([stream], 0, 48_000, 48_000).load, []);
});

// ---------- Focus mix ----------
test("attenuation follows the Web Audio inverse model", () => {
  close(attenuation(10, 40), 1);
  close(attenuation(40, 40), 1);
  close(attenuation(80, 40), 0.5);
  close(attenuation(400, 40), 0.1);
  assert.ok(attenuation(2500, 40) < attenuation(400, 40));
});

test("focus weight peaks at the focus point and widens with zoom", () => {
  close(focusWeight(0, 30, R), 1);
  assert.ok(focusWeight(30, 30, R) < focusWeight(10, 30, R));
  assert.ok(focusWeight(100, 1000, R) > focusWeight(100, 30, R), "a far camera hears a wider area");
});

const viewAt = (q: number, r: number, zoom: number): View => {
  const [x, z] = cellWorld(q, r, R);
  return { camera: [x, zoom * 0.8, z + zoom * 0.6], focus: [x, 0, z], zoom, radius: R };
};

test("zooming onto a river tile raises its emitter over a far one", () => {
  const [nx, nz] = cellWorld(-1, -5, R);
  const [fx, fz] = cellWorld(-1, 0, R);
  const close30 = viewAt(-1, -5, 30);
  const near = emitterGain(nx, nz, close30);
  const far = emitterGain(fx, fz, close30);
  assert.ok(near > 0.9, `focused tile near full gain (${near})`);
  assert.ok(far < near / 10, `far tile ${far} vs focused ${near}`);
  const strategic = viewAt(-1, -5, 800);
  assert.ok(emitterGain(nx, nz, strategic) < near / 5, "zooming out drops the focused emitter");
});

test("biomes come from the catalog edges", () => {
  const b = (id: string) => {
    const t = schema.catalog.tiles.find((x) => x.id === id)!;
    return biomeOf(t.id, t.edges);
  };
  assert.equal(b("meadow"), "meadow");
  assert.equal(b("road_straight"), "meadow");
  assert.equal(b("forest"), "forest");
  assert.equal(b("river_road_bridge"), "river");
  assert.equal(b("beach"), "coast");
  assert.equal(b("ocean"), "coast");
  assert.equal(b("house_cluster"), "town");
});

test("biome mix in view sums to one and falls back to the whole world", () => {
  const tiles: PlacedTile[] = [
    { x: 0, z: 0, biome: "meadow" },
    { x: 28, z: 0, biome: "coast" },
    { x: 5000, z: 0, biome: "forest" },
  ];
  const mix = biomeMix(tiles, { camera: [0, 40, 30], focus: [0, 0, 0], zoom: 50, radius: R });
  close(
    Object.values(mix).reduce((a, b) => a + b, 0),
    1,
  );
  assert.equal(mix.forest, 0, "out of view");
  assert.ok(mix.meadow > mix.coast, "the focused tile weighs more");
  const away = biomeMix(tiles, { camera: [-9000, 40, 0], focus: [-9000, 0, 0], zoom: 30, radius: R });
  close(away.meadow, 1 / 3);
  assert.deepEqual(biomeMix([], viewAt(0, 0, 30)), emptyMix());
  const beds = bedGains({ ...emptyMix(), meadow: 0.5, coast: 0.5 });
  assert.ok((beds.get("meadow-bed") ?? 0) > (beds.get("surf") ?? 0));
  assert.equal(beds.get("forest-birdsong"), 0);
});

test("emitter voices: loudest per cue, a floor, and a bias to keep current voices", () => {
  const c = (key: string, cue: string, gain: number) => ({ key, cue, gain });
  const cands = [c("a", "surf", 0.9), c("b", "surf", 0.5), c("c", "surf", 0.45), c("d", "river", 0.001)];
  assert.deepEqual([...pickEmitters(cands, new Set(), 2)].sort(), ["a", "b"]);
  assert.deepEqual([...pickEmitters(cands, new Set(["c"]), 2)].sort(), ["a", "c"], "c keeps its voice over b");
  assert.ok(!pickEmitters(cands, new Set(), 3).has("d"), "below the floor");
});

// ---------- Music crossfade ----------
const moodMix = (m: Partial<Mix>): Mix => ({ ...emptyMix(), ...m });
const SONGS = ["meadow-day", "coast-breeze", "hearth-town"];

test("music with zero cues stays idle and silent", () => {
  let st = musicIdle();
  for (let t = 0; t < 100; t += 10) st = musicStep(st, [], moodMix({ coast: 1 }), t);
  assert.deepEqual(st, musicIdle());
  assert.equal(musicGains(st, 100).size, 0);
});

test("the first cue fades in on the mood's pick", () => {
  const st = musicStep(musicIdle(), SONGS, moodMix({ coast: 0.7, meadow: 0.3 }), 10);
  assert.equal(st.current, "coast-breeze");
  close(musicGains(st, 10).get("coast-breeze")!, 0);
  close(musicGains(st, 10 + MUSIC_OPTS.fade).get("coast-breeze")!, 1);
});

test("a mood change waits for the dwell and the margin, then crossfades at equal power", () => {
  const o = MUSIC_OPTS;
  let st = musicStep(musicIdle(), SONGS, moodMix({ meadow: 1 }), 0);
  assert.equal(st.current, "meadow-day");
  st = musicStep(st, SONGS, moodMix({ coast: 0.9, meadow: 0.1 }), o.dwell - 1);
  assert.equal(st.current, "meadow-day", "inside the dwell");
  st = musicStep(st, SONGS, moodMix({ coast: 0.52, meadow: 0.48 }), o.dwell + 1);
  assert.equal(st.current, "meadow-day", "inside the margin");
  const t0 = o.dwell + 2;
  st = musicStep(st, SONGS, moodMix({ coast: 0.9, meadow: 0.1 }), t0);
  assert.deepEqual([st.current, st.previous], ["coast-breeze", "meadow-day"]);
  const mid = musicGains(st, t0 + o.fade / 2);
  close(mid.get("coast-breeze")!, Math.SQRT1_2);
  close(mid.get("meadow-day")!, Math.SQRT1_2);
  close(mid.get("coast-breeze")! ** 2 + mid.get("meadow-day")! ** 2, 1);
  st = musicStep(st, SONGS, moodMix({ coast: 0.9 }), t0 + o.fade);
  assert.equal(st.previous, null, "the outgoing cue is dropped after its fade");
  assert.deepEqual([...musicGains(st, t0 + o.fade).keys()], ["coast-breeze"]);
});

test("cues without a mood never join the playlist; losing every cue fades out", () => {
  let st = musicStep(musicIdle(), ["photo-mode", "title-theme"], moodMix({ meadow: 1 }), 0);
  assert.equal(st.current, null);
  st = musicStep(musicIdle(), ["meadow-day"], moodMix({ meadow: 1 }), 0);
  st = musicStep(st, [], moodMix({ meadow: 1 }), 50);
  assert.deepEqual([st.current, st.previous], [null, "meadow-day"]);
  close(musicGains(st, 50 + MUSIC_OPTS.fade).get("meadow-day")!, 0);
});

// ---------- Occasional bees ----------

const seq = (...xs: number[]) => {
  let i = 0;
  return () => xs[i++ % xs.length]!;
};
const flowerView: View = { camera: [0, 30, 20], focus: [0, 0, 0], zoom: 30, radius: R };
const flower = (q: number, r: number): FlowerTile => {
  const [x, z] = cellWorld(q, r, R);
  return { q, r, x, z };
};

test("meadow-bees ships as a decoded ambience one-shot; review candidates never reach the game", () => {
  const bees = manifest.cues.find((c) => c.id === BEE_CUE);
  assert.ok(bees && bees.status === "mastered" && !bees.loop && bees.playback === "decode");
  assert.equal(busOf(bees), "ambience");
  const cands = (manifestJson as unknown as { candidates?: Cue[] }).candidates ?? [];
  assert.deepEqual(
    cands.map((c) => c.id),
    ["meadow-bed-a", "meadow-bed-b", "meadow-bed-c", "meadow-bed-d"],
  );
  const ids = new Set(playable(manifest.cues, "webm").map((c) => c.id));
  for (const c of cands) assert.ok(!ids.has(c.id), `${c.id} is not playable`);
});

test("flower tiles: meadow defs whose scatter carries wildflowers", () => {
  const defs = schema.catalog.tiles.filter((t) => isFlowerTile(biomeOf(t.id, t.edges), t.scatter)).map((t) => t.id);
  assert.ok(defs.includes("meadow"), `flower defs: ${defs.join(", ")}`);
  assert.equal(isFlowerTile("forest", ["wildflower_clump"]), false);
  assert.equal(isFlowerTile("meadow", ["rock_small"]), false);
});

test("bee delay spans 45-120 s", () => {
  assert.equal(
    beeDelay(() => 0),
    BEE_DELAY_S[0],
  );
  close(
    beeDelay(() => 0.999999),
    BEE_DELAY_S[1],
    1e-3,
  );
  close(
    beeDelay(() => 0.5),
    82.5,
  );
});

test("bee tile: only flower tiles within three hex steps of the focus, weighted by the focus mix", () => {
  const near = flower(1, 0);
  const far = flower(6, 0);
  assert.equal(
    pickBeeTile([far], flowerView, () => 0.5),
    null,
  );
  assert.equal(
    pickBeeTile([], flowerView, () => 0.5),
    null,
  );
  assert.deepEqual(
    pickBeeTile([near, far], flowerView, () => 0.99),
    near,
  );
  // The focus tile outweighs a ring-two tile, so a low draw lands on it.
  const centre = flower(0, 0);
  assert.deepEqual(
    pickBeeTile([centre, flower(2, 0)], flowerView, () => 0.1),
    centre,
  );
});

test("bee gate: mute, a silent master or ambience bus, and an unloaded cue each hold the swarm", () => {
  const on = { muted: false, master: 0.8, ambience: 0.7, loaded: true };
  assert.equal(beeGate(on), null);
  assert.equal(beeGate({ ...on, muted: true }), "muted");
  assert.equal(beeGate({ ...on, master: 0 }), "muted");
  assert.equal(beeGate({ ...on, ambience: 0 }), "ambience-off");
  assert.equal(beeGate({ ...on, loaded: false }), "not-loaded");
});

test("bee schedule: first step arms, a due swarm plays once, a gated or tileless one is skipped and re-armed", () => {
  const tile = flower(0, 1);
  const rand = seq(0, 0.5, 1 - 1e-9, 0);
  let { state, play } = beeStep(beeIdle(), 10, null, () => tile, rand);
  assert.equal(play, null);
  assert.equal(state.nextAt, 55);
  ({ state, play } = beeStep(state, 54.9, null, () => tile, rand));
  assert.equal(play, null);
  ({ state, play } = beeStep(state, 55, null, () => tile, rand));
  assert.deepEqual(play, tile);
  assert.deepEqual(
    { plays: state.plays, lastAt: state.lastAt, lastTile: state.lastTile, gatedBy: state.gatedBy },
    { plays: 1, lastAt: 55, lastTile: { q: 0, r: 1 }, gatedBy: null },
  );
  assert.equal(state.nextAt, 55 + 82.5);
  ({ state, play } = beeStep(state, 200, "muted", () => tile, rand));
  assert.equal(play, null);
  assert.equal(state.gatedBy, "muted");
  assert.equal(state.plays, 1);
  close(state.nextAt!, 320, 1e-3);
  ({ state, play } = beeStep(state, 400, null, () => null, rand));
  assert.equal(play, null);
  assert.equal(state.gatedBy, "no-flower-tile");
  assert.equal(state.nextAt, 445);
});
