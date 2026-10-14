import assert from "node:assert/strict";
import { test } from "node:test";
import type { PlayTile } from "../src/bridge.ts";
import {
  TILES,
  cutOff,
  diffTiles,
  fit,
  fitMeta,
  isSlot,
  legalRotations,
  needLines,
  needsAt,
  nothingFitsWhy,
  rotationReadout,
  stepsToNextOrientation,
  summary,
  tileDef,
  tileMap,
} from "../src/rules.ts";
import { TILE_NAMES, actionText, bulldozeRefusal, coord } from "../src/words.ts";
import parity from "./rules-parity.json" with { type: "json" };

const def = (id: string) => tileDef(id)!;
const world = (...t: [number, number, string, number][]) =>
  tileMap(t.map(([q, r, tile, rotation]) => ({ q, r, tile, rotation })));
const start = world([0, 0, "meadow", 0]);

test("legal rotations match hex-core at every frontier slot of the parity worlds", () => {
  let checked = 0;
  for (const w of parity.worlds) {
    const tiles = world(...(w.tiles as [number, number, string, number][]));
    for (const [slot, perTile] of Object.entries(w.slots)) {
      const [q, r] = slot.split(",").map(Number) as [number, number];
      assert.ok(isSlot({ q, r }, tiles), `${slot} is a slot`);
      const needs = needsAt({ q, r }, tiles);
      for (const [id, legal] of Object.entries(perTile as Record<string, number[]>)) {
        assert.deepEqual(legalRotations(def(id), needs), legal, `${id} at ${slot}`);
        checked++;
      }
    }
  }
  assert.ok(checked > 100, `checked ${checked}`);
});

test("every catalog tile has a display name", () => {
  assert.deepEqual(TILES.map((t) => t.id).sort(), Object.keys(TILE_NAMES).sort());
});

test("needs read the neighbour's facing edge", () => {
  // East of the start meadow, edge 3 (west) touches grass.
  assert.deepEqual(needsAt({ q: 1, r: 0 }, start), [null, null, null, "Grass", null, null]);
  const w = world([0, 0, "meadow", 0], [1, 0, "road_straight", 1]);
  // road_straight turned once carries road on world edges 1 and 4. (1,1) lies across edge 1 of (1,0),
  // so its own edge 4 meets that road.
  assert.equal(needsAt({ q: 1, r: 1 }, w)[4], "Road");
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
  assert.equal(ocean.why, "Needs grass, forest or beach to meet the grass.");
  const river = world([0, 0, "river_straight", 0]);
  const off = fit(def("meadow"), needsAt({ q: 1, r: 0 }, river));
  assert.equal(off.why, "Needs a river edge to meet the river.");
});

test("rotation readout and the steps R takes to a new orientation", () => {
  const slot = needsAt({ q: 1, r: 0 }, start);
  const road = def("road_straight");
  assert.deepEqual(rotationReadout(road, [1, 2, 4, 5], 4), { index: 0, count: 2 });
  assert.deepEqual(rotationReadout(road, [1, 2, 4, 5], 2), { index: 1, count: 2 });
  assert.equal(stepsToNextOrientation(road, [1, 2, 4, 5], 1, false), 1);
  // Every meadow rotation looks the same: R has nothing to show.
  assert.equal(stepsToNextOrientation(def("meadow"), legalRotations(def("meadow"), slot), 0, false), 0);
  // road_junction repeats every 120 degrees: legal 0, 2, 4 are one orientation; 1, 3, 5 another.
  const j = def("road_junction");
  assert.equal(stepsToNextOrientation(j, [0, 2, 4], 0, false), 0);
  assert.equal(stepsToNextOrientation(j, [0, 1, 2, 3, 4, 5], 0, true), 1);
});

test("the slot key lists lanes first", () => {
  assert.deepEqual(needLines(["River", "Grass", null, null, null, null]), [
    { kind: "River", text: "River on one edge" },
    { kind: "Grass", text: "Grass or forest beside it" },
  ]);
  assert.deepEqual(needLines(["Road", "Grass", null, null, null, null])[1], {
    kind: "Grass",
    text: "Grass, forest or beach beside it",
  });
  assert.deepEqual(needLines([null, null, null, "Grass", "Grass", null]), [
    { kind: "Grass", text: "Grass, forest or beach on 2 edges" },
  ]);
  assert.equal(summary(def("road_straight")), "Road and grass edges");
  assert.equal(summary(def("meadow")), "Grass all round");
});

test("a slot that needs a river and the sea fits nothing (the e2e S3.4 world)", () => {
  const w = world([0, 0, "meadow", 0], [-1, 0, "river_straight", 1], [-2, 1, "beach", 0]);
  const needs = needsAt({ q: -1, r: 1 }, w);
  assert.deepEqual(needs, [null, null, null, "Ocean", "River", "Grass"]);
  assert.ok(TILES.every((t) => !fit(t, needs).ok));
  assert.equal(nothingFitsWhy(needs), "No tile carries both a river and the sea.");
  assert.deepEqual(
    needLines(needs).map((l) => l.text),
    ["River on one edge", "Ocean beside it", "Grass or forest beside it"],
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
  const t: PlayTile = { q: -1, r: 2, tile: "forest", rotation: 0 };
  assert.equal(coord(t), "(−1, 2)");
});
