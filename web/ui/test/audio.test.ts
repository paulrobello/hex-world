// The audio layer's pure parts: cue selection from the real manifest, format fallback, the decoded
// budget, the focus-mix and attenuation math, and the music crossfade with synthetic cues.
import assert from "node:assert/strict";
import { test } from "node:test";
import manifestJson from "../../../assets/audio/manifest.json" with { type: "json" };
import { type Cue, type Manifest, busOf, eventCues, pickFormat, planDecode, playable } from "../src/audio-cues.ts";
import {
  BED_CUE,
  EMITTER_CUE,
  type Mix,
  MUSIC_OPTS,
  ONE_SHOTS,
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
  pickVariant,
  resolveOneShot,
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
