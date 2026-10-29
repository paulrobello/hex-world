import assert from "node:assert/strict";
import { test } from "node:test";
import {
  PICKER,
  TILES,
  entryMeta,
  entryOf,
  entryReadout,
  entrySummary,
  fitEntry,
  needsAt,
  tileMap,
} from "../src/rules.ts";
import { tileName } from "../src/words.ts";

const world = (...t: [number, number, string, number][]) =>
  tileMap(t.map(([q, r, tile, rotation]) => ({ q, r, tile, rotation, level: 0 })));
const curve = () => PICKER.find((e) => e.id === "beach_curve")!;

test("the picker lists one Beach Curve and neither the headland nor the bay", () => {
  const ids = PICKER.map((e) => e.id);
  assert.ok(!ids.includes("beach_outer") && !ids.includes("beach_inner"));
  assert.equal(ids.filter((id) => id === "beach_curve").length, 1);
  assert.deepEqual(
    curve().tiles.map((t) => t.id),
    ["beach_outer", "beach_inner"],
  );
  // The row name is the first member's display name; no row reads Headland or Bay.
  const names = PICKER.map((e) => tileName(e.tiles[0]!.id));
  assert.equal(names.filter((n) => n === "Beach Curve").length, 1);
  assert.ok(!names.includes("Headland") && !names.includes("Bay"));
  // In the straight beach's place, after it; every other tile is its own entry.
  assert.equal(ids[ids.indexOf("beach_curve") - 1], "beach");
  // The Beach Curve folds two tiles into one entry, the river mouths two, the Hillside twelve.
  assert.equal(PICKER.length, TILES.length - 1 - 1 - 11);
  assert.equal(entryOf("beach_inner")!.id, "beach_curve");
  assert.equal(entryOf("meadow")!.id, "meadow");
});

test("the picker lists one River mouth for both handednesses", () => {
  const ids = PICKER.map((e) => e.id);
  assert.ok(!ids.includes("river_mouth") && !ids.includes("river_mouth_l"));
  const mouth = PICKER.find((e) => e.id === "river_mouths")!;
  assert.deepEqual(
    mouth.tiles.map((t) => t.id),
    ["river_mouth", "river_mouth_l"],
  );
  const names = PICKER.map((e) => tileName(e.tiles[0]!.id));
  assert.equal(names.filter((n) => n === "River mouth").length, 1);
  assert.ok(!names.includes("River mouth mirrored"));
  assert.equal(entryOf("river_mouth_l")!.id, "river_mouths");
});

test("Beach Curve fits where either member fits and counts both members' turns", () => {
  // East of the start meadow: the headland one way, the bay three ways, as hex-core cycles them.
  const f = fitEntry(curve(), needsAt({ q: 1, r: 0 }, world([0, 0, "meadow", 0])));
  assert.ok(f.ok);
  assert.deepEqual(
    f.seq.map((t) => [t.def.id, t.k]),
    [
      ["beach_outer", 0],
      ["beach_inner", 0],
      ["beach_inner", 1],
      ["beach_inner", 5],
    ],
  );
  assert.equal(entryMeta(curve(), f), "4 rotations fit");
  assert.deepEqual(entryReadout(f, "beach_inner", 1), { index: 2, count: 4 });
  assert.deepEqual(entryReadout(f, "beach_outer", 0), { index: 0, count: 4 });
  // Three meadows around (1, 0): only the bay fits, and that is enough for the row to fit.
  const land = world([0, 0, "meadow", 0], [0, 1, "meadow", 0], [1, 1, "meadow", 0]);
  const bay = fitEntry(curve(), needsAt({ q: 1, r: 0 }, land));
  assert.ok(bay.ok);
  assert.ok(bay.seq.every((t) => t.def.id === "beach_inner"));
  // Ocean on two opposite edges: neither member has opposite ocean edges, so the row is off, with
  // a reason in the player's words.
  const none = fitEntry(curve(), needsAt({ q: 1, r: 0 }, world([0, 0, "ocean", 0], [2, 0, "ocean", 0])));
  assert.equal(none.ok, false);
  assert.ok(none.why.length > 0, none.why);
});

test("a lone tile's entry reads as its tile did", () => {
  const meadow = PICKER.find((e) => e.id === "meadow")!;
  const f = fitEntry(meadow, needsAt({ q: 1, r: 0 }, world([0, 0, "meadow", 0])));
  assert.equal(entryMeta(meadow, f), "Fits any way round");
  assert.equal(entrySummary(meadow), "Grass all round");
  assert.equal(entrySummary(curve()), "Ocean, beach and grass edges");
});
