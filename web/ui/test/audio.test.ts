// The audio layer's pure parts: cue selection from the real manifest, format fallback, the decoded
// budget, the focus-mix and attenuation math, and the music crossfade with synthetic cues.
import assert from "node:assert/strict";
import { test } from "node:test";
import manifestJson from "../../../assets/audio/manifest.json" with { type: "json" };
import { type Cue, type Manifest, busOf, eventCues, pickFormat, planDecode, playable } from "../src/audio-cues.ts";
import {
  BED_CUE,
  bellIdle,
  bellStep,
  BOAT_CUES,
  BOAT_OAR_VOICES,
  BOAT_RATE_SPREAD,
  type BoatEvent,
  type BoatState,
  boatIdle,
  boatInRange,
  boatStep,
  CAR_CUES,
  CAR_DRIVE_GAP_S,
  CAR_EVENT_WAIT_S,
  CAR_RATE_SPREAD,
  CAR_VOICES,
  type CarRequest,
  type CarState,
  type DrivingCar,
  carIdle,
  carInRange,
  carRate,
  carStep,
  EMITTER_CUE,
  type Mix,
  MUSIC_OPTS,
  ONE_SHOTS,
  type OneShotGate,
  type OneShotRow,
  type OneShotTile,
  type PlacedTile,
  type View,
  attenuation,
  bedGains,
  biomeMix,
  biomeOf,
  emitterCues,
  cellWorld,
  emitterGain,
  emptyMix,
  focusWeight,
  isFlowerTile,
  musicGains,
  musicIdle,
  musicStep,
  nearestOneShotTile,
  PHOTO_CUE,
  oneShotDefs,
  oneShotDelay,
  oneShotGate,
  oneShotIdle,
  oneShotStep,
  pickEmitters,
  pickOneShotTile,
  PLACE_SLOSH,
  pickVariant,
  placementPool,
  resolveOneShot,
  SHOP_BELL,
  TRAIN_CUES,
  TRAIN_EVENT_WAIT_S,
  TRAIN_VOICES,
  type MovingTrain,
  type TrainRequest,
  type TrainState,
  trainIdle,
  trainInRange,
  trainStep,
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
  assert.deepEqual(map.get("tileReplaced"), ["place-rumble", "place-thunk"], "a replace sounds like a placement");
  for (const e of ["hoverChanged", "toolChanged", "inventoryToggled", "photoModeToggled", "actionRefused", "riseStage"])
    assert.equal(map.get(e), undefined, `${e} has no cue`);
  // Documentation only: the runtime routes trip events to one pick from the pool (CAR_CUES), never to every cue.
  assert.deepEqual(map.get("carPulledOut"), ["car-start-2"]);
  assert.deepEqual(map.get("carParked"), ["car-door-2", "car-door-3"]);
});

test("mastered music is playable and a pending source is not", () => {
  const ids = playable(manifest.cues, "webm").map((c) => c.id);
  for (const id of ["meadow-day", "coast-breeze", "hearth-town", "photo-mode", "river-babble"])
    assert.ok(ids.includes(id), id);
  const music = manifest.cues.filter((c) => c.kind === "music" && c.id !== "discovery-stinger");
  assert.ok(music.length >= 5 && music.every((c) => c.playback === "stream"), "music streams");
  const pending: Cue = { id: "song", kind: "music", class: "music", status: "pending-source", events: [] };
  assert.deepEqual(playable([pending], "webm"), []);
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
  assert.ok(
    cues.some((c) => c.playback === "stream"),
    "the real manifest streams music",
  );
  assert.equal(
    plan.bytes,
    cues.filter((c) => c.playback !== "stream").reduce((s, c) => s + (c.decodedBytes ?? 0), 0),
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
  const cues = (id: string) => {
    const t = schema.catalog.tiles.find((x) => x.id === id)!;
    return emitterCues(t.id, t.edges);
  };
  // A river mouth carries both the river's babble and the surf; a spring is river water.
  assert.deepEqual(cues("river_mouth"), ["river-babble", "surf"]);
  assert.deepEqual(cues("river_mouth_l"), ["river-babble", "surf"]);
  assert.deepEqual(cues("river_spring"), ["river-babble"]);
  assert.deepEqual(cues("beach"), ["surf"]);
  assert.deepEqual(cues("meadow"), []);
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
});

test("beds and loop emitters: river and coast only; meadow and forest are quiet between one-shots", () => {
  assert.equal(BED_CUE.meadow, null);
  assert.equal(BED_CUE.forest, null);
  assert.equal(EMITTER_CUE.meadow, null);
  assert.equal(EMITTER_CUE.forest, null);
  assert.equal(EMITTER_CUE.river, "river-babble");
  assert.equal(EMITTER_CUE.coast, "surf");
  const beds = bedGains({ ...emptyMix(), meadow: 0.4, forest: 0.2, river: 0.2, coast: 0.2 });
  assert.deepEqual([...beds.keys()].sort(), ["river-babble", "surf"]);
  close(beds.get("river-babble")!, 0.25 * 0.2);
  close(beds.get("surf")!, 0.3 * 0.2);
  assert.equal(bedGains({ ...emptyMix(), meadow: 1 }).get("surf"), 0, "an all-meadow view has no bed");
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

test("photo mode's cue takes over at once and the mood's pick returns at once when it clears", () => {
  const ids = [...SONGS, PHOTO_CUE, "title-theme"];
  const o = MUSIC_OPTS;
  let st = musicStep(musicIdle(), ids, moodMix({ meadow: 1 }), 0);
  assert.equal(st.current, "meadow-day");
  st = musicStep(st, ids, moodMix({ meadow: 1 }), 5, o, PHOTO_CUE);
  assert.deepEqual([st.current, st.previous], [PHOTO_CUE, "meadow-day"], "inside the dwell, photo mode still wins");
  close(musicGains(st, 5 + o.fade / 2).get(PHOTO_CUE)!, Math.SQRT1_2);
  st = musicStep(st, ids, moodMix({ coast: 1 }), 8, o, PHOTO_CUE);
  assert.equal(st.current, PHOTO_CUE, "the mood does not move the photo cue");
  st = musicStep(st, ids, moodMix({ coast: 1 }), 9);
  assert.deepEqual([st.current, st.previous], ["coast-breeze", PHOTO_CUE], "leaving photo mode skips the dwell");
  st = musicStep(musicIdle(), SONGS, moodMix({ meadow: 1 }), 0, o, PHOTO_CUE);
  assert.equal(st.current, "meadow-day", "an unplayable photo cue leaves the playlist alone");
});

// ---------- Occasional one-shots (bees, forest birds and leaves) ----------

const seq = (...xs: number[]) => {
  let i = 0;
  return () => xs[i++ % xs.length]!;
};
/** mulberry32: a repeatable stream for the long random runs. */
const seeded = (seed: number) => () => {
  seed = (seed + 0x6d2b79f5) >>> 0;
  let t = seed;
  t = Math.imul(t ^ (t >>> 15), t | 1);
  t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
  return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
};
const noDraw = () => {
  throw new Error("drew a random number");
};
const rowNamed = (name: string): OneShotRow => ONE_SHOTS.find((r) => r.name === name)!;
const BEES = rowNamed("bees");
const BIRDS = rowNamed("forest-birds");
const LEAVES = rowNamed("forest-leaves");
const flowerView: View = { camera: [0, 30, 20], focus: [0, 0, 0], zoom: 30, radius: R };
const cell = (q: number, r: number): OneShotTile => {
  const [x, z] = cellWorld(q, r, R);
  return { q, r, x, z };
};

test("meadow-bees ships as a decoded ambience one-shot; review candidates never reach the game", () => {
  const bees = manifest.cues.find((c) => c.id === BEES.pool);
  assert.ok(bees && bees.status === "mastered" && !bees.loop && bees.playback === "decode");
  assert.equal(busOf(bees), "ambience");
  assert.deepEqual(resolveOneShot(BEES.pool, manifest.cues)?.variants, [BEES.pool], "a one-variant pool");
  const cands = (manifestJson as unknown as { candidates?: Cue[] }).candidates ?? [];
  const ids = new Set(playable(manifest.cues, "webm").map((c) => c.id));
  for (const c of cands) assert.ok(!ids.has(c.id), `${c.id} is not playable`);
});

test("the retired forest-birdsong and meadow-bed loops are gone from the manifest", () => {
  const ids = manifest.cues.map((c) => c.id);
  assert.ok(!ids.includes("forest-birdsong"), "forest-birdsong retired");
  assert.ok(!ids.includes("meadow-bed"), "meadow-bed retired");
});

test("forest birds and leaves ship as pools of decoded ambience one-shots", () => {
  for (const pool of [BIRDS.pool, LEAVES.pool]) {
    const variants = manifest.cues.filter((c) => c.pool === pool);
    assert.ok(variants.length >= 1, `${pool} has variants`);
    assert.deepEqual(
      resolveOneShot(pool, manifest.cues)?.variants,
      variants.map((c) => c.id),
    );
    for (const c of variants) {
      assert.ok(c.status === "mastered" && !c.loop && c.playback === "decode", `${c.id} is a decoded one-shot`);
      assert.equal(busOf(c), "ambience", `${c.id} plays on the ambience bus`);
    }
  }
});

test("one-shot rows: bees at flower tiles, forest birds and leaves at forest tiles", () => {
  assert.deepEqual(
    ONE_SHOTS.map((r) => [r.name, r.pool, r.delay, r.rangeSteps, r.level]),
    [
      ["bees", "meadow-bees", [45, 120], 3, 0.35],
      ["forest-birds", "forest-birds", [12, 40], 3, 0.4],
      ["forest-leaves", "forest-leaves", [20, 60], 3, 0.3],
      ["farm-cow", "farm-cow", [30, 90], 3, 0.25],
      ["farm-sheep", "farm-sheep", [30, 90], 3, 0.25],
      ["windmill-creak", "windmill-creak", [25, 70], 3, 0.3],
      ["wheat-rustle", "wheat-rustle", [20, 60], 3, 0.3],
      ["well-windlass", "well-windlass", [40, 110], 3, 0.25],
    ],
  );
  assert.ok(oneShotDefs(BEES, schema.catalog.tiles).has("meadow"));
  for (const row of [BIRDS, LEAVES]) {
    const defs = oneShotDefs(row, schema.catalog.tiles);
    assert.ok(defs.has("forest"), `${row.name} plays at forest`);
    assert.ok(defs.has("oak"), `${row.name} plays at the oak tile`);
    for (const t of schema.catalog.tiles.filter((t) => defs.has(t.id)))
      assert.equal(biomeOf(t.id, t.edges), "forest", `${row.name} def ${t.id}`);
    for (const id of [
      "meadow",
      "road_straight",
      "river_straight",
      "river_road_bridge",
      "beach",
      "ocean",
      "house_cluster",
    ])
      assert.ok(!defs.has(id), `${row.name} never plays at ${id}`);
  }
});

test("flower tiles: meadow defs whose scatter carries wildflowers", () => {
  const defs = schema.catalog.tiles.filter((t) => isFlowerTile(biomeOf(t.id, t.edges), t.scatter)).map((t) => t.id);
  assert.ok(defs.includes("meadow"), `flower defs: ${defs.join(", ")}`);
  assert.equal(isFlowerTile("forest", ["wildflower_clump"]), false);
  assert.equal(isFlowerTile("meadow", ["rock_small"]), false);
});

test("one-shot delays span each row's range", () => {
  for (const [row, lo, hi] of [
    [BEES, 45, 120],
    [BIRDS, 12, 40],
    [LEAVES, 20, 60],
  ] as const) {
    assert.equal(
      oneShotDelay(row, () => 0),
      lo,
    );
    close(
      oneShotDelay(row, () => 0.999999),
      hi,
      1e-3,
    );
  }
  close(
    oneShotDelay(BEES, () => 0.5),
    82.5,
  );
});

test("bee tile: only flower tiles within three hex steps of the focus, weighted by the focus mix", () => {
  const near = cell(1, 0);
  const far = cell(6, 0);
  assert.equal(
    pickOneShotTile(BEES, [far], flowerView, () => 0.5),
    null,
  );
  assert.equal(
    pickOneShotTile(BEES, [], flowerView, () => 0.5),
    null,
  );
  assert.deepEqual(
    pickOneShotTile(BEES, [near, far], flowerView, () => 0.99),
    near,
  );
  // The focus tile outweighs a ring-two tile, so a low draw lands on it.
  const centre = cell(0, 0);
  assert.deepEqual(
    pickOneShotTile(BEES, [centre, cell(2, 0)], flowerView, () => 0.1),
    centre,
  );
});

test("forest rows pick only forest tiles within three hex steps of the focus", () => {
  const placed = [
    { id: "forest", ...cell(1, 0) },
    { id: "forest", ...cell(2, 1) },
    { id: "forest", ...cell(4, -1) },
    { id: "meadow", ...cell(0, 0) },
    { id: "river_straight", ...cell(-1, 0) },
  ];
  // A wide view, so the focus weight leaves both in-range forest tiles likely.
  const wide: View = { camera: [0, 240, 180], focus: [0, 0, 0], zoom: 300, radius: R };
  for (const row of [BIRDS, LEAVES]) {
    const defs = oneShotDefs(row, schema.catalog.tiles);
    const tiles = placed.filter((t) => defs.has(t.id)).map(({ q, r, x, z }) => ({ q, r, x, z }));
    assert.equal(tiles.length, 3, "the three forest tiles match");
    const rand = seeded(7);
    const seen = new Set<string>();
    for (let i = 0; i < 200; i++) {
      const t = pickOneShotTile(row, tiles, wide, rand)!;
      seen.add(`${t.q},${t.r}`);
    }
    assert.deepEqual([...seen].sort(), ["1,0", "2,1"], "never the meadow, the river or the forest four steps out");
  }
});

test("variants: never the same one twice in a row while the pool has another", () => {
  const pool = ["a", "b", "c", "d"];
  const rand = seeded(42);
  let last: string | null = null;
  const seen = new Set<string>();
  for (let i = 0; i < 500; i++) {
    const v: string = pickVariant(pool, last, rand)!;
    assert.notEqual(v, last);
    seen.add(v);
    last = v;
  }
  assert.deepEqual([...seen].sort(), pool, "every variant plays");
  assert.equal(pickVariant(["a", "b"], "a", noDraw), "b", "two variants alternate without a draw");
  // A one-variant pool draws nothing, so the bee schedule's delays stay where they were.
  assert.equal(pickVariant(["meadow-bees"], "meadow-bees", noDraw), "meadow-bees");
  assert.equal(pickVariant([], null, noDraw), null);
});

test("one-shot gate: mute, a silent master or ambience bus, and no decoded variant each hold the play", () => {
  const on = { muted: false, master: 0.8, ambience: 0.7, loaded: true };
  assert.equal(oneShotGate(on), null);
  assert.equal(oneShotGate({ ...on, muted: true }), "muted");
  assert.equal(oneShotGate({ ...on, master: 0 }), "muted");
  assert.equal(oneShotGate({ ...on, ambience: 0 }), "ambience-off");
  assert.equal(oneShotGate({ ...on, loaded: false }), "not-loaded");
  assert.deepEqual(oneShotIdle(), {
    nextAt: null,
    lastAt: null,
    lastTile: null,
    lastCue: null,
    plays: 0,
    gatedBy: null,
  });
  const armed = { ...oneShotIdle(), nextAt: 20 };
  const empty = oneShotStep(
    BIRDS,
    armed,
    20,
    null,
    () => cell(1, 0),
    [],
    () => 0,
  );
  assert.equal(empty.play, null);
  assert.equal(empty.state.gatedBy, "not-loaded");
});

test("bee schedule: first step arms, a due swarm plays once, a gated or tileless one is skipped and re-armed", () => {
  const tile = cell(0, 1);
  const bees = [BEES.pool];
  const rand = seq(0, 0.5, 1 - 1e-9, 0);
  let { state, play } = oneShotStep(BEES, oneShotIdle(), 10, null, () => tile, bees, rand);
  assert.equal(play, null);
  assert.equal(state.nextAt, 55);
  ({ state, play } = oneShotStep(BEES, state, 54.9, null, () => tile, bees, rand));
  assert.equal(play, null);
  ({ state, play } = oneShotStep(BEES, state, 55, null, () => tile, bees, rand));
  assert.deepEqual(play, { tile, cue: "meadow-bees" });
  assert.deepEqual(
    {
      plays: state.plays,
      lastAt: state.lastAt,
      lastTile: state.lastTile,
      lastCue: state.lastCue,
      gatedBy: state.gatedBy,
    },
    { plays: 1, lastAt: 55, lastTile: { q: 0, r: 1 }, lastCue: "meadow-bees", gatedBy: null },
  );
  assert.equal(state.nextAt, 55 + 82.5);
  ({ state, play } = oneShotStep(BEES, state, 200, "muted", () => tile, bees, rand));
  assert.equal(play, null);
  assert.equal(state.gatedBy, "muted");
  assert.equal(state.plays, 1);
  close(state.nextAt!, 320, 1e-3);
  ({ state, play } = oneShotStep(BEES, state, 400, null, () => null, bees, rand));
  assert.equal(play, null);
  assert.equal(state.gatedBy, "no-tile");
  assert.equal(state.nextAt, 445);
});

test("forest birds schedule: each due play is a new variant, recorded with the count", () => {
  const variants = ["forest-birds-1", "forest-birds-2", "forest-birds-3"];
  const rand = seeded(3);
  const tile = cell(1, 0);
  let st = oneShotStep(BIRDS, oneShotIdle(), 0, null, () => tile, variants, rand).state;
  assert.ok(st.nextAt! >= 12 && st.nextAt! <= 40, `armed 12-40 s out (${st.nextAt})`);
  let prev: string | null = null;
  for (let i = 1; i <= 50; i++) {
    const due = st.nextAt!;
    const step = oneShotStep(BIRDS, st, due, null, () => tile, variants, rand);
    assert.ok(step.play && variants.includes(step.play.cue));
    assert.notEqual(step.play.cue, prev);
    assert.equal(step.state.lastCue, step.play.cue);
    assert.equal(step.state.plays, i);
    const gap = step.state.nextAt! - due;
    assert.ok(gap >= 12 && gap <= 40, `next play 12-40 s later (${gap})`);
    prev = step.play.cue;
    st = step.state;
  }
});

test("on demand: a pool resolves to its variants in manifest order, a cue id to itself, an unknown name to null", () => {
  const cues = [
    { id: "ui-select" },
    { id: "forest-birds-2", pool: "forest-birds" },
    { id: "forest-leaves-1", pool: "forest-leaves" },
    { id: "forest-birds-1", pool: "forest-birds" },
    { id: "meadow-bees" },
  ];
  assert.deepEqual(resolveOneShot("forest-birds", cues), {
    pool: "forest-birds",
    variants: ["forest-birds-2", "forest-birds-1"],
    row: BIRDS,
  });
  const bees = { pool: "meadow-bees", variants: ["meadow-bees"], row: BEES };
  assert.deepEqual(resolveOneShot("meadow-bees", cues), bees);
  assert.deepEqual(resolveOneShot("bees", cues), bees, "a row name plays its pool");
  assert.deepEqual(resolveOneShot("forest-birds-1", cues), {
    pool: "forest-birds",
    variants: ["forest-birds-1"],
    row: BIRDS,
  });
  assert.deepEqual(resolveOneShot("ui-select", cues), { pool: "ui-select", variants: ["ui-select"], row: null });
  assert.equal(resolveOneShot("no-such-cue", cues), null);
  assert.equal(resolveOneShot("forest-birds", []), null);
});

test("on demand: the matching tile nearest the focus, at any distance", () => {
  const far = cell(8, 0);
  const near = cell(-5, 1);
  assert.deepEqual(nearestOneShotTile([far, near], flowerView.focus), near);
  assert.deepEqual(nearestOneShotTile([far], flowerView.focus), far, "out of the scheduler's range still counts");
  assert.equal(nearestOneShotTile([], flowerView.focus), null);
});

// ---------- Cars ----------
const carView: View = { camera: [0, 96, 72], focus: [0, 0, 0], zoom: 120, radius: R };
const carPools: Record<string, string[]> = {
  "car-start": ["start-a"],
  "car-door": ["door-a", "door-b"],
  "car-drive": ["drive-a"],
};
const carVariants = (pool: string) => carPools[pool] ?? [];
const carSeconds = (cue: string) => (cue.startsWith("door") ? 1.5 : cue.startsWith("start") ? 4.5 : 2.2);
const trip = (car: number, kind: "start" | "door", q: number, r: number, at = 0): CarRequest => {
  const [x, z] = cellWorld(q, r, R);
  return { car, kind, x, z, at };
};
const onRoad = (id: number, q: number, r: number): DrivingCar => {
  const [x, z] = cellWorld(q, r, R);
  return { id, phase: "road", x, z };
};
const step = (st: CarState, now: number, events: CarRequest[] = [], driving: DrivingCar[] = [], rand = () => 0.5) =>
  carStep(st, now, events, driving, carView, null, carVariants, carSeconds, rand);

test("cars: trip events play their pool at the spot, only within three hex steps of the focus", () => {
  const r1 = step(carIdle(), 10, [trip(1, "start", 1, 0, 10), trip(2, "door", 4, 0, 10)]);
  assert.deepEqual(
    r1.play.map((p) => [p.car, p.kind, p.cue]),
    [[1, "start", "start-a"]],
  );
  const [x, z] = cellWorld(1, 0, R);
  assert.deepEqual([r1.play[0]!.x, r1.play[0]!.z], [x, z]);
  close(r1.play[0]!.gain, CAR_CUES.start.level * emitterGain(x, z, carView));
  assert.equal(r1.state.pending.length, 0, "the out-of-range door is dropped, not queued");
  assert.equal(r1.state.gatedBy, "out-of-range");
  assert.ok(carInRange(...cellWorld(3, 0, R), carView) && !carInRange(...cellWorld(4, 0, R), carView));
  close(r1.state.voices[0]!.endsAt, 10 + 4.5 / carRate(1));
});

test("cars: at most CAR_VOICES at once; a trip event waits for a voice, then is dropped", () => {
  const four = [1, 2, 3, 4].map((id) => trip(id, "door", 0, 0, 0));
  const r1 = step(carIdle(), 0, four);
  assert.equal(r1.play.length, CAR_VOICES);
  assert.deepEqual(
    r1.state.pending.map((e) => e.car),
    [4],
  );
  assert.equal(r1.state.gatedBy, "cap");
  const r2 = step(r1.state, 1);
  assert.equal(r2.play.length, 0, "still capped while the doors sound");
  const r3 = step(r2.state, 1.6);
  assert.deepEqual(
    r3.play.map((p) => p.car),
    [4],
    "a voice ended inside the wait",
  );
  const late = step(step(carIdle(), 0, four).state, CAR_EVENT_WAIT_S + 0.1);
  assert.equal(late.play.length, 0, "past the wait the event is gone");
  assert.equal(late.state.pending.length, 0);
});

test("cars: one voice per car, one pass-by per trip, pass-bys spaced, never the same door twice running", () => {
  const r1 = step(carIdle(), 0, [], [onRoad(7, 1, 0)]);
  assert.deepEqual(
    r1.play.map((p) => [p.car, p.kind]),
    [[7, "drive"]],
  );
  assert.equal(step(r1.state, 3, [], [onRoad(7, 1, 0)]).play.length, 0, "one pass-by per trip");
  // The car parks (it leaves car_list) while its pass-by sounds: the door waits for that voice to end.
  const r2 = step(r1.state, 1, [trip(7, "door", 1, 0, 1)]);
  assert.equal(r2.play.length, 0);
  const r3 = step(r2.state, 2.5);
  assert.deepEqual(
    r3.play.map((p) => [p.car, p.kind]),
    [[7, "door"]],
  );
  // Parked, it lost its pass-by mark, so its next trip gets one again once it is on the road.
  const r4 = step(r3.state, 10, [trip(7, "start", 4, 0, 10)]);
  assert.equal(r4.play.length, 0, "the pull-out is out of range");
  assert.deepEqual(
    step(r4.state, 12, [], [onRoad(7, 1, 0)]).play.map((p) => [p.car, p.kind]),
    [[7, "drive"]],
  );
  // Two cars entering range together: the louder first, the other after the gap.
  const both = [onRoad(1, 2, 0), onRoad(2, 0, 1)];
  const r5 = step(carIdle(), 0, [], both);
  assert.deepEqual(
    r5.play.map((p) => p.car),
    [2],
  );
  assert.equal(step(r5.state, CAR_DRIVE_GAP_S - 0.1, [], both).play.length, 0);
  assert.deepEqual(
    step(r5.state, CAR_DRIVE_GAP_S, [], both).play.map((p) => p.car),
    [1],
  );
  assert.equal(step(carIdle(), 0, [], [{ ...onRoad(3, 0, 0), phase: "pulling_out" }]).play.length, 0, "off-road");
  // Doors alternate between the pool's variants whatever the draw.
  let st = carIdle();
  const doors: string[] = [];
  for (let i = 0; i < 6; i++) {
    const r = step(st, i * 10, [trip(10 + i, "door", 0, 0, i * 10)], [], () => 0);
    doors.push(r.play[0]!.cue);
    st = r.state;
  }
  assert.deepEqual(doors, ["door-a", "door-b", "door-a", "door-b", "door-a", "door-b"]);
});

test("cars: a gated step (mute, no ambience) drops waiting trip events and plays nothing", () => {
  for (const gate of ["muted", "ambience-off"] as const) {
    const r = carStep(
      carIdle(),
      0,
      [trip(1, "start", 0, 0)],
      [onRoad(2, 0, 0)],
      carView,
      gate,
      carVariants,
      carSeconds,
      () => 0.5,
    );
    assert.equal(r.play.length, 0);
    assert.equal(r.state.pending.length, 0);
    assert.equal(r.state.gatedBy, gate);
  }
  const unloaded = carStep(
    carIdle(),
    0,
    [trip(1, "start", 0, 0)],
    [],
    carView,
    null,
    () => [],
    carSeconds,
    () => 0.5,
  );
  assert.equal(unloaded.play.length, 0, "an undecoded pool plays nothing");
});

test("cars: no pass-by starts while photo mode holds the agents, and none replays after", () => {
  const road = [onRoad(1, 0, 1), onRoad(2, 1, 0)];
  const first = step(carIdle(), 0, [], [road[0]!]);
  assert.deepEqual(
    first.play.map((p) => p.car),
    [1],
  );
  const held = carStep(first.state, 5, [], road, carView, null, carVariants, carSeconds, () => 0.5, true);
  assert.equal(held.play.length, 0, "a frozen car on the road plays nothing");
  assert.deepEqual(held.state.drove, [1], "the pass-by mark survives the pause");
  const resumed = step(held.state, 6, [], road);
  assert.deepEqual(
    resumed.play.map((p) => p.car),
    [2],
    "only the car that never had its pass-by",
  );
});

test("cars: each car keeps one rate within the spread", () => {
  const rates = Array.from({ length: 200 }, (_, i) => carRate(i));
  assert.ok(rates.every((r) => Math.abs(r - 1) <= CAR_RATE_SPREAD + 1e-9));
  assert.equal(carRate(42), carRate(42));
  assert.ok(new Set(rates.map((r) => r.toFixed(3))).size > 50, "rates differ between cars");
});

// ---------- P3 items: farm, windmill, wheat field, bakery bell, train ----------
const P3_ROWS: [string, string][] = [
  ["farm-cow", "farm"],
  ["farm-sheep", "farm"],
  ["windmill-creak", "windmill"],
  ["wheat-rustle", "wheat_field"],
  ["well-windlass", "house_cluster"],
];

test("P3 rows play only at their own tile def, not at meadows that share the biome and scatter", () => {
  for (const [name, def] of P3_ROWS) {
    const row = rowNamed(name);
    assert.deepEqual([...oneShotDefs(row, schema.catalog.tiles)], [def], `${name} plays only at ${def}`);
  }
  for (const def of ["farm", "windmill", "wheat_field"]) {
    const t = schema.catalog.tiles.find((x) => x.id === def)!;
    assert.equal(biomeOf(t.id, t.edges), "meadow", `${def} is a meadow-biome tile`);
  }
  const tile = pickOneShotTile(rowNamed("farm-cow"), [cell(0, 0), cell(4, 0)], flowerView, () => 0.5);
  assert.deepEqual(tile && [tile.q, tile.r], [0, 0], "a farm in range, not one four steps out");
});

test("P3 pools ship as decoded ambience one-shots; the train pools are mastered but in no row or event", () => {
  const pools = [
    "farm-cow",
    "farm-sheep",
    "windmill-creak",
    "wheat-rustle",
    "shop-bell",
    "train-whistle",
    "train-chuff",
  ];
  for (const pool of pools) {
    const variants = manifest.cues.filter((c) => c.pool === pool);
    assert.ok(variants.length >= 1, `${pool} has variants`);
    for (const c of variants) {
      assert.ok(c.status === "mastered" && !c.loop && c.playback === "decode", `${c.id} is a decoded one-shot`);
      assert.equal(busOf(c), "ambience", `${c.id} plays on the ambience bus`);
      assert.deepEqual(c.events, [], `${c.id} is routed by the runtime, not by an event list`);
    }
  }
  for (const pool of ["train-whistle", "train-chuff", "shop-bell"])
    assert.ok(!ONE_SHOTS.some((r) => r.pool === pool), `${pool} has no timer row`);
  assert.equal(resolveOneShot("train-whistle", manifest.cues)?.row, null, "on demand only, non-positional");
});

test("the stone well's windlass: a quiet, rare row at house_cluster tiles over a two-variant ambience pool", () => {
  const row = rowNamed("well-windlass");
  assert.deepEqual([...oneShotDefs(row, schema.catalog.tiles)], ["house_cluster"], "plays only at house_cluster");
  const mill = rowNamed("windmill-creak");
  assert.ok(row.level < mill.level, "quieter than the windmill");
  assert.ok(row.delay[0] > mill.delay[0] && row.delay[1] > mill.delay[1], "rarer than the windmill");
  const variants = manifest.cues.filter((c) => c.pool === "well-windlass");
  assert.deepEqual(
    variants.map((c) => c.id),
    ["well-windlass-1", "well-windlass-2"],
  );
  assert.deepEqual(resolveOneShot("well-windlass", manifest.cues)?.variants, ["well-windlass-1", "well-windlass-2"]);
  assert.equal(resolveOneShot("well-windlass", manifest.cues)?.row, row, "the pool resolves to its timer row");
  for (const c of variants) {
    assert.ok(c.status === "mastered" && !c.loop && c.playback === "decode", `${c.id} is a decoded one-shot`);
    assert.equal(c.channels, 1, `${c.id} is mono`);
    assert.equal(busOf(c), "ambience", `${c.id} plays on the ambience bus`);
    assert.deepEqual(c.events, [], `${c.id} is routed by the scheduler, not by an event list`);
  }
  const tile = pickOneShotTile(row, [cell(0, 0), cell(4, 0)], flowerView, () => 0.5);
  assert.deepEqual(tile && [tile.q, tile.r], [0, 0], "a well in range, not one four steps out");
});

test("placement: a water tile selects the slosh pool and a land tile does not", () => {
  const edges = new Map(schema.catalog.tiles.map((t) => [t.id, t.edges]));
  const poolOf = (id: string) => placementPool(edges.get(id)!);
  const water = ["river_straight", "river_bend", "river_spring", "river_road_bridge", "river_rail_bridge"];
  water.push("river_fork", "beach", "beach_outer", "beach_inner", "river_mouth", "river_mouth_l", "ocean");
  water.push("river_bend_60");
  for (const id of water) assert.equal(poolOf(id), PLACE_SLOSH.pool, `${id} sloshes`);
  const land = ["meadow", "forest", "oak", "farm", "windmill", "wheat_field", "house_cluster", "bakery"];
  land.push("road_straight", "road_bend_120", "road_bend_60", "road_junction");
  land.push("rail_straight", "rail_bend_60", "rail_bend_120", "rail_junction", "rail_station", "rail_road_crossing");
  for (const id of land) assert.equal(poolOf(id), null, `${id} has no slosh`);
  assert.equal(water.length + land.length, schema.catalog.tiles.length, "every catalog tile is classified");
  const variants = manifest.cues.filter((c) => c.pool === PLACE_SLOSH.pool);
  assert.ok(variants.length >= 2, "the slosh pool has variants");
  for (const c of variants) {
    assert.ok(c.status === "mastered" && !c.loop && c.playback === "decode", `${c.id} is a decoded one-shot`);
    assert.equal(busOf(c), "effects", `${c.id} plays on the effects bus with the thunk`);
    assert.deepEqual(c.events, [], `${c.id} is routed by the runtime, so a land placement never plays it`);
  }
  assert.ok(!ONE_SHOTS.some((r) => r.pool === PLACE_SLOSH.pool), "no timer row");
});

const park = (q: number, r: number) => {
  const [x, z] = cellWorld(q, r, R);
  return { q, r, x: x + 1.8, z: z - 8.8 };
};
const bakeryAt = (cells: [number, number][]) => (q: number, r: number) => cells.some(([a, b]) => a === q && b === r);
const BELLS = ["bell-a", "bell-b"];
const ring = (
  st: ReturnType<typeof bellIdle>,
  now: number,
  parks: ReturnType<typeof park>[] = [],
  gate: OneShotGate = null,
  shops: [number, number][] = [[1, 0]],
) => bellStep(st, now, parks, bakeryAt(shops), carView, gate, BELLS, () => 0);

test("shop bell: a park at a bakery rings after the door, at the spot; a park at a house does not", () => {
  const r0 = ring(bellIdle(), 10, [park(1, 0), park(0, 1)]);
  assert.equal(r0.play.length, 0, "not at once: the door closes first");
  assert.equal(r0.state.pending.length, 1, "only the bakery park waits for a bell");
  const early = ring(r0.state, 10 + SHOP_BELL.delayS - 0.1);
  assert.equal(early.play.length, 0);
  const due = ring(early.state, 10 + SHOP_BELL.delayS);
  assert.equal(due.play.length, 1);
  const p = due.play[0]!;
  const { x, z } = park(1, 0);
  assert.deepEqual([p.q, p.r, p.x, p.z], [1, 0, x, z]);
  close(p.gain, SHOP_BELL.level * emitterGain(x, z, carView));
  assert.equal(due.state.pending.length, 0);
  assert.deepEqual(due.state.lastTile, { q: 1, r: 0 });
  assert.equal(due.state.plays, 1);
  assert.ok(
    schema.catalog.tiles.some((t) => t.id === SHOP_BELL.tile),
    "the bakery is a catalog tile",
  );
});

test("shop bell: dropped when muted, with the ambience bus at 0 or out of range; never the same bell twice running", () => {
  for (const gate of ["muted", "ambience-off"] as const) {
    const r0 = ring(bellIdle(), 0, [park(1, 0)]);
    const r1 = ring(r0.state, SHOP_BELL.delayS, [], gate);
    assert.equal(r1.play.length, 0, gate);
    assert.equal(r1.state.gatedBy, gate);
    assert.equal(r1.state.pending.length, 0, `${gate}: dropped, not queued`);
  }
  const far = ring(bellIdle(), 0, [park(5, 0)], null, [[5, 0]]);
  const farDue = ring(far.state, SHOP_BELL.delayS, [], null, [[5, 0]]);
  assert.equal(farDue.play.length, 0);
  assert.equal(farDue.state.gatedBy, "out-of-range");
  let st = bellIdle();
  const cues: string[] = [];
  for (let i = 0; i < 6; i++) {
    st = ring(st, i * 10, [park(1, 0)]).state;
    const r = ring(st, i * 10 + SHOP_BELL.delayS);
    st = r.state;
    cues.push(r.play[0]!.cue);
  }
  for (let i = 1; i < cues.length; i++) assert.notEqual(cues[i], cues[i - 1], `bell ${i} repeats ${cues[i]}`);
});

// ---------- Trains ----------
const trainPools: Record<string, string[]> = {
  "train-whistle": ["whistle-a", "whistle-b"],
  "train-chuff": ["chuff-a"],
};
const trainVariants = (pool: string) => trainPools[pool] ?? [];
const trainSeconds = (cue: string) => (cue.startsWith("whistle") ? 3 : 6);
const departs = (train: number, q: number, r: number, at = 0): TrainRequest => {
  const [x, z] = cellWorld(q, r, R);
  return { train, x, z, at };
};
const running = (id: number, q: number, r: number, speed = 6): MovingTrain => {
  const [x, z] = cellWorld(q, r, R);
  return { id, x, z, speed };
};
const tstep = (st: TrainState, now: number, events: TrainRequest[] = [], trains: MovingTrain[] = [], paused = false) =>
  trainStep(st, now, events, trains, carView, null, trainVariants, trainSeconds, () => 0.5, paused);

test("trains: the manifest has the whistle and chuff pools the train cues name", () => {
  const m = manifestJson as unknown as Manifest;
  for (const { pool } of Object.values(TRAIN_CUES))
    assert.ok(resolveOneShot(pool, m.cues, [])?.variants.length, `${pool} has mastered variants`);
});

test("trains: a departure whistles at the front in range, and a moving train chuffs where it is", () => {
  const r = tstep(trainIdle(), 10, [departs(0, 1, 0, 10), departs(1, 6, 0, 10)], [running(0, 1, 0)]);
  assert.deepEqual(
    r.play.map((p) => [p.train, p.kind, p.cue]),
    [
      [0, "whistle", "whistle-b"],
      [0, "chuff", "chuff-a"],
    ],
  );
  const [x, z] = cellWorld(1, 0, R);
  close(r.play[0]!.gain, TRAIN_CUES.whistle.level * emitterGain(x, z, carView));
  close(r.play[1]!.gain, TRAIN_CUES.chuff.level * emitterGain(x, z, carView));
  assert.equal(r.state.gatedBy, null);
  assert.equal(r.state.pending.length, 0, "the far departure is dropped");
  assert.ok(trainInRange(...cellWorld(4, 0, R), carView) && !trainInRange(...cellWorld(5, 0, R), carView));
});

test("trains: a chuff restarts while its train runs on, and a stopped or far train stays quiet", () => {
  let st = tstep(trainIdle(), 0, [], [running(0, 0, 0)]).state;
  assert.equal(st.plays, 1);
  st = tstep(st, 3, [], [running(0, 1, 0)]).state;
  assert.equal(st.plays, 1, "no second chuff while the first sounds");
  const again = tstep(st, 6.5, [], [running(0, 2, 0)]);
  assert.deepEqual(
    again.play.map((p) => p.kind),
    ["chuff"],
    "the train still runs: it chuffs again",
  );
  assert.equal(tstep(trainIdle(), 0, [], [running(0, 0, 0, 0)]).play.length, 0, "standing");
  assert.equal(tstep(trainIdle(), 0, [], [running(0, 6, 0)]).play.length, 0, "out of range");
  assert.equal(tstep(trainIdle(), 0, [], [running(0, 0, 0)], true).play.length, 0, "paused");
});

test("trains: voices are capped, a departure waits for one, then is dropped", () => {
  const busy = [running(0, 0, 0), running(1, 1, 0), running(2, -1, 0), running(3, 0, 1)];
  const r = tstep(trainIdle(), 0, [departs(4, 0, 0, 0)], busy);
  assert.equal(r.state.voices.length, TRAIN_VOICES);
  assert.equal(r.play[0]!.kind, "whistle", "the departure goes first");
  assert.equal(r.state.gatedBy, "cap");
  const full = tstep(r.state, 1, [departs(5, 0, 0, 1)], busy);
  assert.equal(full.play.length, 0);
  assert.equal(full.state.pending.length, 1, "the new departure waits");
  const late = tstep(full.state, 1 + TRAIN_EVENT_WAIT_S + 0.1, [], []);
  assert.equal(late.state.pending.length, 0, "and is dropped after the wait");
  const gated = trainStep(
    trainIdle(),
    0,
    [departs(0, 0, 0)],
    busy,
    carView,
    "muted",
    trainVariants,
    trainSeconds,
    () => 0.5,
  );
  assert.deepEqual([gated.play.length, gated.state.gatedBy], [0, "muted"]);
});

test("trains: an arrival at a buffer stop clunks once from the buffer-clunk pool, no whistle", () => {
  const pools: Record<string, string[]> = { ...trainPools, "buffer-clunk": ["clunk-a", "clunk-b"] };
  const arrive = { ...departs(0, 1, 0, 10), kind: "clunk" as const };
  const step = (st: TrainState, events: TrainRequest[]) =>
    trainStep(
      st,
      10,
      events,
      [],
      carView,
      null,
      (p) => pools[p] ?? [],
      () => 1,
      () => 0.5,
    );
  const r = step(trainIdle(), [arrive]);
  assert.deepEqual(
    r.play.map((p) => [p.train, p.kind]),
    [[0, "clunk"]],
  );
  assert.ok(pools["buffer-clunk"]!.includes(r.play[0]!.cue));
  close(r.play[0]!.gain, TRAIN_CUES.clunk.level * emitterGain(...cellWorld(1, 0, R), carView));
  assert.equal(step(r.state, [arrive]).play.length, 0, "no second clunk while the first sounds");
});

// ---------- Boats ----------
const boatPools: Record<string, string[]> = { "boat-oar": ["oar-a", "oar-b", "oar-c"], "boat-sail": ["sail-a"] };
const boatVariants = (pool: string) => boatPools[pool] ?? [];
const boatSeconds = (cue: string) => (cue.startsWith("oar") ? 1.6 : 2.5);
const boatAt = (boat: number, kind: "oar" | "sail", q: number, r: number): BoatEvent => {
  const [x, z] = cellWorld(q, r, R);
  return { boat, kind, x, z };
};
const bstep = (st: BoatState, now: number, events: BoatEvent[], gate: OneShotGate = null, rand = () => 0.5) =>
  boatStep(st, now, events, carView, gate, boatVariants, boatSeconds, rand);

test("boats: the oar and sail pools ship as decoded ambience one-shots routed by the runtime", () => {
  for (const { pool } of Object.values(BOAT_CUES)) {
    const variants = manifest.cues.filter((c) => c.pool === pool);
    assert.ok(variants.length >= 1, `${pool} has variants`);
    for (const c of variants) {
      assert.ok(c.status === "mastered" && !c.loop && c.playback === "decode", `${c.id} is a decoded one-shot`);
      assert.equal(busOf(c), "ambience", `${c.id} plays on the ambience bus`);
      assert.deepEqual(c.events, [], `${c.id} is routed by the runtime, not by an event list`);
    }
  }
  assert.ok(!ONE_SHOTS.some((r) => r.pool === "boat-oar" || r.pool === "boat-sail"), "no timer row");
});

test("boats: a stroke plays an oar at the boat under the focus mix, a flap a sail, each within the rate spread", () => {
  const r = bstep(boatIdle(), 5, [boatAt(1, "oar", 1, 0), boatAt(2, "sail", 0, 1)], null, () => 0.99);
  assert.deepEqual(
    r.play.map((p) => [p.boat, p.kind, p.cue]),
    [
      [1, "oar", "oar-c"],
      [2, "sail", "sail-a"],
    ],
  );
  const [x, z] = cellWorld(1, 0, R);
  assert.deepEqual([r.play[0]!.x, r.play[0]!.z], [x, z]);
  close(r.play[0]!.gain, BOAT_CUES.oar.level * emitterGain(x, z, carView));
  close(r.play[1]!.gain, BOAT_CUES.sail.level * emitterGain(...cellWorld(0, 1, R), carView));
  close(r.state.voices[0]!.endsAt, 5 + 1.6 / r.play[0]!.rate);
  assert.deepEqual([r.state.plays, r.state.dropped, r.state.gatedBy], [2, 0, null]);
  for (const rand of [() => 0, () => 0.5, () => 0.999999]) {
    const rate = bstep(boatIdle(), 0, [boatAt(1, "oar", 0, 0)], null, rand).play[0]!.rate;
    assert.ok(Math.abs(rate - 1) <= BOAT_RATE_SPREAD + 1e-9, `rate ${rate} within the spread`);
  }
  close(bstep(boatIdle(), 0, [boatAt(1, "oar", 0, 0)], null, () => 0).play[0]!.rate, 1 - BOAT_RATE_SPREAD);
});

test("boats: dropped while muted, with the ambience bus at 0, or beyond three hex steps of the focus", () => {
  const both = [boatAt(1, "oar", 0, 0), boatAt(2, "sail", 0, 0)];
  for (const gate of ["muted", "ambience-off"] as const) {
    const r = bstep(boatIdle(), 0, both, gate);
    assert.deepEqual([r.play.length, r.state.gatedBy, r.state.dropped], [0, gate, 2], gate);
  }
  assert.ok(boatInRange(...cellWorld(3, 0, R), carView) && !boatInRange(...cellWorld(4, 0, R), carView));
  const far = bstep(boatIdle(), 0, [boatAt(1, "oar", 4, 0), boatAt(2, "sail", 0, 4)]);
  assert.deepEqual([far.play.length, far.state.gatedBy, far.state.dropped], [0, "out-of-range", 2]);
  const empty = boatStep(
    boatIdle(),
    0,
    [boatAt(1, "oar", 0, 0)],
    carView,
    null,
    () => [],
    boatSeconds,
    () => 0.5,
  );
  assert.deepEqual([empty.play.length, empty.state.gatedBy], [0, "not-loaded"]);
});

test("boats: never the same oar twice running, and every take of the pool is used", () => {
  let seed = 7;
  const rand = () => {
    seed = (seed * 1103515245 + 12345) % 2147483648;
    return seed / 2147483648;
  };
  let st = boatIdle();
  const cues: string[] = [];
  for (let i = 0; i < 30; i++) {
    const r = bstep(st, i * 3, [boatAt(i % 3, "oar", 0, 0)], null, rand);
    st = r.state;
    cues.push(r.play[0]!.cue);
  }
  for (let i = 1; i < cues.length; i++) assert.notEqual(cues[i], cues[i - 1], `stroke ${i} repeats ${cues[i]}`);
  assert.deepEqual([...new Set(cues)].sort(), ["oar-a", "oar-b", "oar-c"]);
  assert.equal(st.lastCue["boat-oar"], cues.at(-1));
});

test("boats: at most BOAT_OAR_VOICES oars at once; a stroke beyond them is dropped, a sail still plays", () => {
  const strokes = [1, 2, 3].map((id) => boatAt(id, "oar", 0, 0));
  const r1 = bstep(boatIdle(), 0, [...strokes, boatAt(4, "sail", 0, 0)]);
  assert.deepEqual(
    r1.play.map((p) => [p.boat, p.kind]),
    [
      [1, "oar"],
      [2, "oar"],
      [4, "sail"],
    ],
  );
  assert.equal(BOAT_OAR_VOICES, 2);
  assert.equal(r1.state.dropped, 1, "the third stroke is dropped");
  assert.equal(r1.state.gatedBy, null, "the sail played after the drop");
  const r2 = bstep(r1.state, 1, [boatAt(3, "oar", 0, 0)]);
  assert.deepEqual([r2.play.length, r2.state.gatedBy, r2.state.dropped], [0, "cap", 2], "still capped, not queued");
  const r3 = bstep(r2.state, 2, [boatAt(3, "oar", 0, 0)]);
  assert.deepEqual(
    r3.play.map((p) => p.boat),
    [3],
    "the oars ended: a new stroke plays",
  );
});
