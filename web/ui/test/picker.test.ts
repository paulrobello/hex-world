import assert from "node:assert/strict";
import { test } from "node:test";
import { PICKER, TILES, entryMeta, entryOf, entrySummary, tileDef, type Turn } from "../src/rules.ts";
import { tileName } from "../src/words.ts";

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
  // The Beach Curve folds two tiles into one entry, the river mouths two, the Hillside twelve and
  // the Rail viaduct three.
  assert.equal(PICKER.length, TILES.length - 1 - 1 - 11 - 2);
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

test("a lone tile's entry reads as its tile did, over the engine's steps", () => {
  const turn = (id: string, k: number, level: number): Turn => ({ def: tileDef(id)!, k, level });
  // A meadow fits any way round; the Beach Curve row counts its members' steps together.
  assert.equal(entryMeta(PICKER.find((e) => e.id === "meadow")!, [turn("meadow", 0, 0)]), "Fits any way round");
  assert.equal(
    entryMeta(curve(), [
      turn("beach_outer", 0, 0),
      turn("beach_inner", 0, 0),
      turn("beach_inner", 1, 0),
      turn("beach_inner", 5, 0),
    ]),
    "4 rotations fit",
  );
  // The count reads on the first step's level: steps on other levels do not enter it.
  assert.equal(entryMeta(curve(), [turn("beach_outer", 0, 0), turn("beach_inner", 0, 1)]), "1 rotation fits");
  assert.equal(entrySummary(curve()), "Ocean, beach and grass edges");
});
