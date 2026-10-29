import assert from "node:assert/strict";
import { test } from "node:test";
import type { PlayTile } from "../src/bridge.ts";
import {
  TILES,
  cutOff,
  diffTiles,
  fit,
  MAX_LEVEL,
  fitMeta,
  fitEntry,
  entryReadout,
  entryOf,
  isCover,
  tunnelCover,
  heightsMeet,
  isSlot,
  laneMeets,
  legalOrientations,
  legalRotations,
  meets,
  needLines,
  needsAt,
  nothingFitsWhy,
  rotationReadout,
  summary,
  tileDef,
  tileMap,
  turnAngle,
  turnHint,
  placementMeets,
} from "../src/rules.ts";
import { TILE_NAMES, actionText, bulldozeRefusal, coord } from "../src/words.ts";
import parity from "./rules-parity.json" with { type: "json" };

const def = (id: string) => tileDef(id)!;
const world = (...t: ([number, number, string, number] | [number, number, string, number, number])[]) =>
  tileMap(t.map(([q, r, tile, rotation, level]) => ({ q, r, tile, rotation, level: level ?? 0 })));
const start = world([0, 0, "meadow", 0]);

test("legal orientations match hex-core, in its order, at every frontier slot of the parity worlds", () => {
  let checked = 0;
  let raised = 0;
  for (const w of parity.worlds) {
    const tiles = world(...(w.tiles as [number, number, string, number, number][]));
    for (const [slot, perTile] of Object.entries(w.slots)) {
      const [q, r] = slot.split(",").map(Number) as [number, number];
      assert.ok(isSlot({ q, r }, tiles), `${slot} is a slot`);
      const needs = needsAt({ q, r }, tiles);
      if (needs.levels.some((l) => l > 0)) raised++;
      for (const [id, legal] of Object.entries(perTile as Record<string, number[][]>)) {
        const got = legalOrientations(def(id), needs).map((o) => [o.rotation, o.level]);
        assert.deepEqual(got, legal, `${id} at ${slot}`);
        checked++;
      }
    }
  }
  assert.ok(checked > 100, `checked ${checked}`);
  assert.ok(raised > 3, `slots beside raised tiles: ${raised}`);
});

test("heights meet seamless, as a cliff, or not at all (hex-core heights_meet)", () => {
  const flat = (h: number, lane = false) => ({ heights: [h, h] as [number, number], lane });
  assert.equal(heightsMeet(flat(1), 0, flat(0), 1), "Seamless");
  assert.equal(heightsMeet(flat(0), 1, flat(0), 0), "Cliff");
  assert.equal(heightsMeet(flat(0, true), 1, flat(0, true), 0), "Refused", "a lane never cliffs");
  assert.equal(heightsMeet(flat(0, true), 1, flat(0, true), 1), "Seamless");
  const rise = { heights: [0, 1] as [number, number], lane: false };
  const fall = { heights: [1, 0] as [number, number], lane: false };
  assert.equal(heightsMeet(rise, 0, fall, 0), "Seamless");
  assert.equal(heightsMeet(rise, 0, fall, 1), "Refused");
  for (const l of [0, 1, 2]) assert.equal(heightsMeet(rise, 0, flat(0), l), "Refused");
});

test("placement never defaults to a cliff, and lanes never cliff", () => {
  // East of the start meadow: the ground first, then every raised level as a cliff.
  const meadow = legalOrientations(def("meadow"), needsAt({ q: 1, r: 0 }, start));
  assert.deepEqual(meadow[0], { rotation: 0, level: 0 });
  assert.ok(meadow.some((o) => o.level === 3) && meadow.every((o) => o.level <= MAX_LEVEL));
  // A river is not raisable; a road beside a raised road end only joins it on its level.
  const raised = world([0, 0, "meadow", 0], [1, 0, "meadow", 0, 1]);
  assert.ok(legalOrientations(def("river_straight"), needsAt({ q: 2, r: 0 }, raised)).every((o) => o.level === 0));
  const road = world([0, 0, "meadow", 0], [1, 0, "road_straight", 1, 1]);
  const beyond = legalOrientations(def("road_straight"), needsAt({ q: 1, r: 1 }, road));
  assert.ok(beyond.length > 0 && beyond.every((o) => o.level === 1));
  // A stopped rail end beside a raised meadow is a cliff, so the ground comes first.
  const station = world([0, 0, "meadow", 0], [1, 0, "rail_station", 1]);
  const beside = legalOrientations(def("meadow"), needsAt({ q: 1, r: 1 }, station));
  assert.deepEqual(beside.slice(0, 2), [
    { rotation: 0, level: 0 },
    { rotation: 1, level: 0 },
  ]);
  assert.ok(beside.some((o) => o.level === 1));
});

test("every catalog tile has a display name", () => {
  assert.deepEqual(TILES.map((t) => t.id).sort(), Object.keys(TILE_NAMES).sort());
});

test("needs read the neighbour's facing edge", () => {
  // East of the start meadow, edge 3 (west) touches grass.
  assert.deepEqual(needsAt({ q: 1, r: 0 }, start).kinds, [null, null, null, "Grass", null, null]);
  const w = world([0, 0, "meadow", 0], [1, 0, "road_straight", 1]);
  // road_straight turned once carries road on world edges 1 and 4. (1,1) lies across edge 1 of (1,0),
  // so its own edge 4 meets that road.
  assert.equal(needsAt({ q: 1, r: 1 }, w).kinds[4], "Road");
});

test("fit counts distinct orientations and words refusals", () => {
  const slot = needsAt({ q: 1, r: 0 }, start);
  const meadow = fit(def("meadow"), slot);
  assert.equal(meadow.legal.length, 6);
  assert.deepEqual(meadow.distinct, [0]);
  assert.equal(fitMeta(def("meadow"), meadow), "Fits any way round");
  const road = fit(def("road_straight"), slot);
  assert.deepEqual(road.legal, [1, 2, 4, 5]);
  assert.deepEqual(road.distinct, [1, 2]);
  assert.equal(fitMeta(def("road_straight"), road), "2 rotations fit");
  const ocean = fit(def("ocean"), slot);
  assert.equal(ocean.ok, false);
  assert.equal(ocean.why, "Needs grass or forest to meet the grass.");
  const river = world([0, 0, "river_straight", 0]);
  const off = fit(def("meadow"), needsAt({ q: 1, r: 0 }, river));
  assert.equal(off.why, "Needs a river edge to meet the river.");
});

test("a stopped rail end takes a tile without rail, and a rail tile must continue the line", () => {
  // A station east of the start meadow turned once: rail ends on world edges 1 and 4. (1,1) faces
  // the end on edge 1 across its own edge 4 (owner, 2026-10-25; hex-core world tests mirror this).
  const w = world([0, 0, "meadow", 0], [1, 0, "rail_station", 1]);
  const beyond = needsAt({ q: 1, r: 1 }, w);
  assert.equal(beyond.kinds[4], "Rail");
  assert.equal(beyond.stopped[4], true);
  for (const id of ["meadow", "forest", "windmill"]) assert.equal(fit(def(id), beyond).legal.length, 6, id);
  assert.deepEqual(fit(def("house_cluster"), beyond).legal, [0, 1, 2, 3, 5]);
  assert.deepEqual(fit(def("road_straight"), beyond).legal, [0, 2, 3, 5]);
  assert.equal(fit(def("ocean"), beyond).ok, false);
  assert.deepEqual(fit(def("rail_straight"), beyond).legal, [1, 4]);
  // A rail tile never turns an end toward a placed meadow.
  assert.deepEqual(fit(def("rail_station"), needsAt({ q: 1, r: 0 }, start)).legal, [1, 2, 4, 5]);
  // The slot key names what fits the stopped end; a rail end that is not stopped still needs rail.
  assert.deepEqual(needLines(beyond.kinds, beyond.stopped), [
    { kind: "Rail", text: "Rail, grass or forest on one edge" },
  ]);
  assert.deepEqual(
    needLines(beyond.kinds).map((l) => l.text),
    ["Rail on one edge"],
  );
  const both = world([0, 0, "meadow", 0], [1, 0, "rail_station", 1], [2, 0, "meadow", 0]);
  assert.deepEqual(
    needLines(needsAt({ q: 1, r: 1 }, both).kinds, needsAt({ q: 1, r: 1 }, both).stopped).map((l) => l.text),
    ["Rail, grass or forest on one edge", "Grass or forest beside it"],
  );
  // A rail edge with no stop still meets only rail.
  const stub = { ...def("rail_straight"), stopped: [false, false, false, false, false, false] };
  assert.deepEqual(fit(stub, needsAt({ q: 1, r: 0 }, start)).legal, [1, 2, 4, 5]);
});

test("rotation readout counts distinct orientations (the engine's R steps through them)", () => {
  const slot = needsAt({ q: 1, r: 0 }, start);
  const road = def("road_straight");
  assert.deepEqual(rotationReadout(road, [1, 2, 4, 5], 4), { index: 0, count: 2 });
  assert.deepEqual(rotationReadout(road, [1, 2, 4, 5], 2), { index: 1, count: 2 });
  // Every meadow rotation looks the same: one orientation, so the tag reads "Fits" with no R.
  assert.deepEqual(rotationReadout(def("meadow"), legalRotations(def("meadow"), slot), 0), { index: 0, count: 1 });
  // The windmill's edges are all grass, but its sails turn with it: six orientations, as hex-core.
  const mill = fit(def("windmill"), slot);
  assert.deepEqual(mill.distinct, [0, 1, 2, 3, 4, 5]);
  assert.equal(fitMeta(def("windmill"), mill), "6 rotations fit");
  assert.deepEqual(rotationReadout(def("windmill"), mill.legal, 3), { index: 3, count: 6 });
  assert.deepEqual(fit(def("bakery"), slot).distinct.length, fit(def("bakery"), slot).legal.length);
  // The wheat field's rows turn with it and a half turn lays them on themselves: three orientations.
  const wheat = fit(def("wheat_field"), slot);
  assert.deepEqual(wheat.distinct, [0, 1, 2]);
  assert.deepEqual(rotationReadout(def("wheat_field"), wheat.legal, 4), { index: 1, count: 3 });
  // road_junction repeats every 120 degrees: legal 0, 2, 4 are one orientation; 1, 3, 5 another.
  const j = def("road_junction");
  assert.deepEqual(rotationReadout(j, [0, 2, 4], 4), { index: 0, count: 1 });
  assert.deepEqual(rotationReadout(j, [0, 1, 2, 3, 4, 5], 3), { index: 1, count: 2 });
});

test("the slot key lists lanes first", () => {
  assert.deepEqual(needLines(["River", "Grass", null, null, null, null]), [
    { kind: "River", text: "River on one edge" },
    { kind: "Grass", text: "Grass or forest beside it" },
  ]);
  assert.deepEqual(needLines(["Road", "Grass", null, null, null, null])[1], {
    kind: "Grass",
    text: "Grass or forest beside it",
  });
  assert.deepEqual(needLines([null, null, null, "Grass", "Grass", null]), [
    { kind: "Grass", text: "Grass or forest on 2 edges" },
  ]);
  assert.equal(summary(def("road_straight")), "Road and grass edges");
  assert.equal(summary(def("meadow")), "Grass all round");
});

test("a slot that needs a road and a beach fits nothing (the e2e S3.4 world)", () => {
  // A river and a beach no longer: the river mouths carry both (water contract §3a).
  const road = world([0, 0, "meadow", 0], [-1, 0, "road_straight", 1]);
  // The e2e places the beach through auto-rotate: the first legal rotation, its grass on the road's verge.
  assert.deepEqual(legalRotations(def("beach"), needsAt({ q: -2, r: 1 }, road)), [1, 2]);
  const w = world([0, 0, "meadow", 0], [-1, 0, "road_straight", 1], [-2, 1, "beach", 1]);
  const needs = needsAt({ q: -1, r: 1 }, w);
  assert.deepEqual(needs.kinds, [null, null, null, "Beach", "Road", "Grass"]);
  assert.ok(TILES.every((t) => !fit(t, needs).ok));
  assert.equal(nothingFitsWhy(needs.kinds), "No tile carries both a road and a beach.");
  assert.deepEqual(
    needLines(needs.kinds).map((l) => l.text),
    ["Road on one edge", "Beach beside it", "Grass or forest beside it"],
  );
  // The river version of this slot fits the river mouth.
  const river = world([0, 0, "meadow", 0], [-1, 0, "river_straight", 1], [-2, 1, "beach", 1]);
  assert.deepEqual(
    TILES.filter((t) => fit(t, needsAt({ q: -1, r: 1 }, river)).ok).map((t) => t.id),
    ["river_mouth"],
  );
});

test("beach edges meet only a coast running the same way (owner O1, hex-core world.rs)", () => {
  // Land sides oppose across a seam: the two tiles look out across it from opposite sides.
  assert.ok(meets("Beach", "Right", "Beach", "Left"));
  assert.ok(!meets("Beach", "Right", "Beach", "Right"));
  assert.ok(!meets("Beach", "Right", "Grass", null));
  assert.ok(!meets("Ocean", null, "Beach", "Left"));
  assert.ok(meets("Grass", null, "Forest", null));
  // (0,1) touches the meadow on its edge 4 and the beach's edge 2 (land on the right) on its edge 5.
  const coast = world([0, 0, "meadow", 0], [1, 0, "beach", 0]);
  const next = needsAt({ q: 0, r: 1 }, coast);
  assert.deepEqual(next.kinds, [null, null, null, null, "Grass", "Beach"]);
  assert.deepEqual(next.land, [null, null, null, null, null, "Right"]);
  // The straight beach carries on, the headland turns toward the land, the bay away from it.
  assert.deepEqual(legalRotations(def("beach"), next), [0]);
  assert.deepEqual(legalRotations(def("beach_outer"), next), [1]);
  assert.deepEqual(legalRotations(def("beach_inner"), next), [0]);
  assert.equal(fit(def("meadow"), next).why, "Needs a beach edge to meet the beach.");
  assert.equal(fit(def("ocean"), next).why, "Needs grass or forest to meet the grass.");
  // A beach whose land runs the wrong way into another beach.
  const mirrored = world([0, 0, "meadow", 0], [1, 0, "beach", 0], [-1, 2, "beach", 3]);
  assert.equal(fit(def("beach"), needsAt({ q: 0, r: 1 }, mirrored)).why, "The coast here runs the other way.");
});

test("a beach edge against grass is refused in the player's words", () => {
  // (1,0) touches three meadows: every beach rotation puts a beach or ocean edge against grass.
  const land = world([0, 0, "meadow", 0], [0, 1, "meadow", 0], [1, 1, "meadow", 0]);
  const needs = needsAt({ q: 1, r: 0 }, land);
  const beach = fit(def("beach"), needs);
  assert.equal(beach.ok, false);
  assert.equal(beach.bestK, 4);
  assert.equal(beach.why, "Its beach would end at the grass.");
  // Only the bay wraps three grass edges.
  assert.deepEqual(
    TILES.filter((t) => t.id.startsWith("beach") && fit(t, needs).ok).map((t) => t.id),
    ["beach_inner"],
  );
});

test("bulldoze cut-off and its words", () => {
  // The larger part stays; the smaller one is what gets cut off.
  const w = world(
    [0, 0, "meadow", 0],
    [-1, 0, "meadow", 0],
    [0, 1, "meadow", 0],
    [1, 0, "road_straight", 0],
    [2, 0, "ocean", 0],
    [3, 0, "ocean", 0],
  );
  const cut = cutOff(w, { q: 1, r: 0 });
  assert.deepEqual(cut.map((t) => t.q).sort(), [2, 3]);
  assert.deepEqual(bulldozeRefusal("removing this tile splits the world", cut), {
    head: "Removing this would split your world",
    sub: "2 ocean tiles would be cut off",
  });
  assert.equal(bulldozeRefusal("the last tile cannot be removed", []).head, "The world needs at least one tile");
  assert.deepEqual(cutOff(w, { q: 3, r: 0 }), []);
});

test("undo and redo text comes from the tile diff", () => {
  const before = world([0, 0, "meadow", 0], [1, 0, "river_bend", 0]);
  const after = world([0, 0, "meadow", 0]);
  assert.equal(actionText(diffTiles(before, after), false), "placed River bend at (1, 0)");
  assert.equal(actionText(diffTiles(after, before), true), "placed River bend at (1, 0)");
  // Undoing a bulldoze brings the tile back.
  assert.equal(actionText(diffTiles(after, before), false), "removed River bend");
  // A replace swaps the tile on one cell; undo and redo both name the swap.
  const replaced = world([0, 0, "meadow", 0], [1, 0, "forest", 2]);
  const meadow = world([0, 0, "meadow", 0], [1, 0, "meadow", 0]);
  assert.equal(actionText(diffTiles(replaced, meadow), false), "replaced Meadow with Pine forest");
  assert.equal(actionText(diffTiles(meadow, replaced), true), "replaced Meadow with Pine forest");
  const t: PlayTile = { q: -1, r: 2, tile: "forest", rotation: 0, level: 0 };
  assert.equal(coord(t), "(−1, 2)");
});

test("turnAngle: a socket hex turns the short way, also across rotation 5 to 0", () => {
  assert.equal(turnAngle(0, 1), 60);
  assert.equal(turnAngle(300, 0), 360, "5 to 0 is one step clockwise");
  assert.equal(turnAngle(0, 5), -60, "0 to 5 is one step back");
  assert.equal(turnAngle(360, 2), 480);
  for (let from = -720; from <= 720; from += 60)
    for (let k = 0; k < 6; k++) {
      const to = turnAngle(from, k);
      assert.ok(Math.abs(to - from) <= 180, `${from} to k=${k}: ${to}`);
      assert.equal((((to - k * 60) % 360) + 360) % 360, 0, `${from} to k=${k} lands on rotation k`);
    }
});

/** The elevation world is the parity world holding the tunnel portal. */
const elevationWorld = () =>
  parity.worlds.find((w) => (w.tiles as [number, number, string][]).some((t) => t[2] === "road_tunnel_portal"))!;
const elevation = () => world(...(elevationWorld().tiles as [number, number, string, number, number][]));

test("a completed tunnel's cover is refused for bulldoze and replace, and cuts nothing off (hex-core TunnelCover)", () => {
  const tiles = elevation();
  assert.deepEqual([...tunnelCover(tiles)], ["3,2"]);
  assert.ok(isCover(tiles, { q: 3, r: 2 }));
  assert.ok(!isCover(tiles, { q: 2, r: 2 }), "a portal is not cover");
  assert.deepEqual(cutOff(tiles, { q: 3, r: 2 }), []);
  // One portal gone: no tunnel, so the meadow above it is ordinary ground again.
  const open = new Map(tiles);
  open.delete("4,2");
  assert.equal(tunnelCover(open).size, 0);
  // A cover that is not one level above the portals is no tunnel either.
  const low = new Map(tiles);
  low.set("3,2", { ...low.get("3,2")!, level: 0 });
  assert.equal(tunnelCover(low).size, 0);
});

test("fit and the entry readout follow the ghost's level (hex-core ghost legal rotations)", () => {
  const tiles = elevation();
  // Beside the raised road end a road only fits on level 1: the ground-only reading said it did not fit.
  const needs = needsAt({ q: 8, r: 2 }, tiles);
  const road = fit(def("road_straight"), needs);
  assert.ok(road.ok && road.level === 1 && road.legal.length > 0);
  assert.deepEqual(
    road.legal,
    legalOrientations(def("road_straight"), needs)
      .filter((o) => o.level === 1)
      .map((o) => o.rotation)
      .sort(),
  );
  // A meadow on flat ground: R walks the ground first, then every raised level as a cliff.
  const flat = needsAt({ q: 1, r: 0 }, start);
  const f = fitEntry(entryOf("meadow")!, flat);
  assert.deepEqual(
    f.seq.map((t) => t.level),
    [0, 1, 2, 3],
  );
  assert.deepEqual(entryReadout(f, "meadow", 0, 2), { index: 0, count: 1 });
  assert.equal(fit(def("meadow"), flat, 2).level, 2);
});

test("the R hint holds across levels: a meadow over a flat slot has more than one orientation at level 0 and at 1", () => {
  const f = fitEntry(entryOf("meadow")!, needsAt({ q: 1, r: 0 }, start));
  assert.ok(f.seq.length > 1, "R has levels to step through");
  for (const level of [0, 1]) assert.equal(entryReadout(f, "meadow", 0, level).count, 1, `level ${level}`);
});

test("lanes meet like hex-core LaneSpec::meets: equal width, mirrored offset, or both absent", () => {
  assert.equal(laneMeets(null, null), true);
  assert.equal(laneMeets([0, 6], [0, 6]), true);
  assert.equal(laneMeets([1.5, 6], [-1.5, 6]), true);
  assert.equal(laneMeets([1.5, 6], [1.5, 6]), false, "offsets mirror across a seam");
  assert.equal(laneMeets([0, 6], [0, 8]), false, "widths differ");
  assert.equal(laneMeets([0, 6], null), false);
  assert.equal(laneMeets(null, [0, 6]), false);
  const road = (lane: readonly [number, number] | null) => ({
    kind: "Road" as const,
    land: null,
    stopped: false,
    rail: false,
    heights: [0, 0] as [number, number],
    lane,
  });
  assert.equal(placementMeets(road([0, 6]), road([0, 6])), true);
  assert.equal(placementMeets(road([0, 6]), road([0, 5])), false, "a lane of another width is refused");
  assert.equal(placementMeets(road([0, 6]), road(null)), false);
});

test("the R hint shows while any rotation or orientation is left to turn to", () => {
  assert.equal(turnHint(1, 1), false, "a lone orientation has nowhere to go");
  assert.equal(turnHint(1, 0), false);
  assert.equal(turnHint(2, 0), true, "several rotations");
  assert.equal(turnHint(1, 2), true, "one rotation but another level or group member");
  assert.equal(turnHint(3, 5), true);
});
