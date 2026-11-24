import assert from "node:assert/strict";
import { test } from "node:test";
import type { PlayTile } from "../src/bridge.ts";
import {
  TILES,
  cutOff,
  diffTiles,
  entryOf,
  isCover,
  isSheetSlot,
  isSlot,
  type Kinds,
  neighbor,
  needLines,
  nothingFitsWhy,
  rotationReadout,
  summary,
  tileDef,
  tileMap,
  tunnelCover,
  turnAngle,
  turnHint,
} from "../src/rules.ts";
import {
  MASSIF_SWAP,
  SINGLE_SWAP,
  TILE_NAMES,
  TUNNEL_COVER,
  actionText,
  bulldozeRefusal,
  coord,
  refusalWord,
  replaceRefusal,
} from "../src/words.ts";

const def = (id: string) => tileDef(id)!;
const world = (...t: ([number, number, string, number] | [number, number, string, number, number])[]) =>
  tileMap(t.map(([q, r, tile, rotation, level]) => ({ q, r, tile, rotation, level: level ?? 0 })));

test("river_uphill has a wording", () => assert.equal(refusalWord("river_uphill"), "Rivers can't run uphill."));
test("a flat edge against a hillside's slope words as a slope, not as grass meeting grass", () => {
  assert.equal(
    refusalWord("slope_grass"),
    "Grass can't meet a slope. Turn the hillside, or place another hillside here.",
  );
  assert.match(refusalWord("own_slope"), /^Its slope can't meet/);
  assert.equal(refusalWord("clash_grass_grass"), "Grass can't meet grass.");
});

test("every catalog tile has a display name", () => {
  assert.deepEqual(TILES.map((t) => t.id).sort(), Object.keys(TILE_NAMES).sort());
});

test("a stopped rail end takes a tile without rail, and the key names both", () => {
  // A station turned once has rail ends on world edges 1 and 4 (owner, 2026-10-25); the key's
  // stopped flag per edge names what such an end takes a tile without rail as.
  const rail: Kinds = [null, null, null, null, "Rail", null];
  assert.deepEqual(needLines(rail, [false, false, false, false, true, false]), [
    { kind: "Rail", text: "Rail, grass or forest on one edge" },
  ]);
  assert.deepEqual(needLines(rail), [{ kind: "Rail", text: "Rail on one edge" }]);
  const both: Kinds = [null, null, null, null, "Rail", "Grass"];
  assert.deepEqual(
    needLines(both, [false, false, false, false, true, false]).map((l) => l.text),
    ["Rail, grass or forest on one edge", "Grass or forest beside it"],
  );
});

test("a slot that needs a road and a beach fits nothing (the e2e S3.4 world's needs)", () => {
  assert.equal(
    nothingFitsWhy([null, null, null, "Beach", "Road", "Grass"]),
    "No tile carries both a road and a beach.",
  );
});

test("rotation readout counts distinct orientations (the engine's R steps through them)", () => {
  const road = def("road_straight");
  assert.deepEqual(rotationReadout(road, [1, 2, 4, 5], 4), { index: 0, count: 2 });
  assert.deepEqual(rotationReadout(road, [1, 2, 4, 5], 2), { index: 1, count: 2 });
  // Every meadow rotation looks the same: one orientation, so the tag reads "Fits" with no R.
  assert.deepEqual(rotationReadout(def("meadow"), [0, 1, 2, 3, 4, 5], 0), { index: 0, count: 1 });
  // The windmill's edges are all grass, but its sails turn with it: six orientations, as hex-core.
  assert.deepEqual(rotationReadout(def("windmill"), [0, 1, 2, 3, 4, 5], 3), { index: 3, count: 6 });
  // The wheat field's rows turn with it and a half turn lays them on themselves: three orientations.
  assert.deepEqual(rotationReadout(def("wheat_field"), [0, 1, 2, 3, 4, 5], 4), { index: 1, count: 3 });
  // road_junction repeats every 120 degrees: legal 0, 2, 4 are one orientation; 1, 3, 5 another.
  const j = def("road_junction");
  assert.deepEqual(rotationReadout(j, [0, 2, 4], 4), { index: 0, count: 1 });
  assert.deepEqual(rotationReadout(j, [0, 1, 2, 3, 4, 5], 3), { index: 1, count: 2 });
  assert.equal(summary(def("road_straight")), "Road and grass edges");
  assert.equal(summary(def("meadow")), "Grass all round");
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

test("a completed tunnel's cover is refused for bulldoze and replace, and cuts nothing off", () => {
  // Two portals face each other along a straight line, the meadow between one level higher.
  const tiles = world([2, 2, "road_tunnel_portal", 0], [3, 2, "meadow", 0, 1], [4, 2, "road_tunnel_portal", 3]);
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

test("turnHint: the R hint shows while the ghost's member has another rotation on its level", () => {
  assert.equal(turnHint(1, 1), false, "a lone orientation has nowhere to go");
  assert.equal(turnHint(1, 0), false);
  assert.equal(turnHint(2, 0), true, "several rotations");
  assert.equal(turnHint(1, 2), true, "one distinct rotation but another orientation of the member");
  assert.equal(turnHint(3, 5), true);
});

test("cutOff treats a massif as one tile", () => {
  const tiles = world([0, 0, "meadow", 0], [2, 0, "massif_granite", 2, 1], [4, 0, "river_mouth", 0]);
  const centre = { q: 2, r: 0 };
  for (const target of [centre, neighbor(centre, 3)]) {
    const cut = cutOff(tiles, target);
    assert.deepEqual(
      cut.map((t) => [t.q, t.r, t.tile]),
      [[4, 0, "river_mouth"]],
      `bulldozing the massif at ${target.q},${target.r}`,
    );
  }
  assert.equal(
    bulldozeRefusal("removing this tile splits the world", cutOff(tiles, centre)).sub,
    "1 river mouth tile would be cut off",
  );
  assert.deepEqual(cutOff(tiles, { q: 4, r: 0 }), []);
  assert.equal(tunnelCover(tiles).size, 0);
  // A massif comes and goes as one tile in the undo and redo words.
  const before = world([0, 0, "meadow", 0]);
  const after = world([0, 0, "meadow", 0], [2, 0, "massif_granite", 2, 1]);
  assert.equal(actionText(diffTiles(after, before), false), "placed Granite massif at (2, 0)");
});

test("the sheet keeps a legal massif centre that is not a frontier slot", () => {
  const tiles = world([0, 0, "meadow", 0], [2, 0, "massif_granite", 2, 1]);
  const megaSlots = [{ q: 2, r: 0, tile: "massif_granite", orientations: [] }];
  // The centre sits inside the world ring, so isSlot reads false and the engine's slot answers.
  assert.equal(isSlot({ q: 2, r: 0 }, tiles), false);
  assert.equal(isSheetSlot({ q: 2, r: 0 }, tiles, megaSlots, "massif_granite"), true);
  assert.equal(isSheetSlot({ q: 2, r: 0 }, tiles, megaSlots, "meadow"), false);
  assert.equal(isSheetSlot({ q: 2, r: 0 }, tiles, [], "massif_granite"), false);
  // A frontier slot beside the start meadow passes for any selection.
  assert.equal(isSheetSlot({ q: 0, r: 1 }, tiles, [], "meadow"), true);
});

test("the replace-refusal wording chain (the sheet mirror's refusedWhy, retired)", () => {
  const tiles = world([0, 0, "meadow", 0], [2, 0, "massif_granite", 2, 1]);
  // A single tile aimed at a massif says the massif words; the massif over a single tile, its own.
  assert.equal(replaceRefusal("meadow", tiles.get("2,0"), "edges [3] do not fit", false, null), MASSIF_SWAP);
  assert.equal(replaceRefusal("massif_granite", tiles.get("0,0"), "edges [3] do not fit", false, null), SINGLE_SWAP);
  // Same class on both sides falls to the engine's code, and to the edge fallback with none.
  assert.equal(
    replaceRefusal("meadow", tiles.get("0,0"), "edges [3] do not fit", false, null),
    "An edge doesn't match its neighbour.",
  );
  // The engine's level reason is shown verbatim, capitalized; a tunnel cover wins over everything.
  assert.equal(
    replaceRefusal("meadow", undefined, "this tile cannot stand at level 5", false, null),
    "This tile cannot stand at level 5.",
  );
  assert.equal(replaceRefusal("meadow", undefined, "edges [3] do not fit", true, null), TUNNEL_COVER);
  assert.equal(
    replaceRefusal("meadow", undefined, "edges [3] do not fit", false, "missing_ocean"),
    "Needs ocean to meet the sea.",
  );
  assert.equal(entryOf("beach_inner")!.id, "beach_curve");
});
