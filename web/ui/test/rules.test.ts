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
  meets,
  needLines,
  needsAt,
  nothingFitsWhy,
  rotationReadout,
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
  const t: PlayTile = { q: -1, r: 2, tile: "forest", rotation: 0 };
  assert.equal(coord(t), "(−1, 2)");
});
