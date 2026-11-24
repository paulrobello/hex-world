// The tag beside the ghost (tag-model.ts): the six states, the signature that gates the HUD's DOM
// writes, and the parts of the line. The `sig` literals were captured from hud.ts's renderTag before
// it was extracted: if one changes, the tag rewrites its DOM every frame, so they must stay as is.
import assert from "node:assert/strict";
import { test } from "node:test";
import type { Cell, PlayState } from "../src/bridge.ts";
import { tileMap } from "../src/rules.ts";
import { type PlacedNote, type RefusalNote, type TagModel, type TagPart, tagModel } from "../src/tag-model.ts";

const tile = (q: number, r: number, id: string, rotation = 0, level = 0) => ({ q, r, tile: id, rotation, level });
const world = tileMap([tile(0, 0, "meadow")]);
const road = tileMap([tile(0, 0, "meadow"), tile(1, 0, "road_straight", 1), tile(-1, 0, "meadow")]);
const NONE: ReadonlySet<string> = new Set();

/** The engine's published fits for a slot where only `entry` is refused (the sheet's tag reads it). */
const fits = (slot: Cell, id: string, refusal: string) => ({
  slot,
  needs: [] as (string | null)[],
  stopped: [] as boolean[],
  nearest: null,
  entries: [{ id, orientations: [], refusal, best: null }],
});

const ghost = (q: number, r: number, id: string, rotation: number, legal: number[], extra: object = {}) => ({
  q,
  r,
  tile: id,
  rotation,
  level: 0,
  valid: true,
  legal_rotations: legal,
  edges: ["Neutral", "Neutral", "Neutral", "Neutral", "Neutral", "Neutral"],
  outer_edges: null,
  footprint: null,
  flow: [],
  waterfalls: [],
  ...extra,
});
const state = (o: object): PlayState =>
  ({
    tool: "place",
    selected: "meadow",
    ghost: null,
    pinned: null,
    hovered: null,
    hovered_kind: null,
    bulldoze: null,
    mega_slots: [],
    ...o,
  }) as unknown as PlayState;
const tag = (
  st: PlayState,
  map = world,
  o: { placed?: PlacedNote; refusal?: RefusalNote; cover?: ReadonlySet<string>; now?: number } = {},
): TagModel | null => tagModel(st, map, o.placed ?? null, o.refusal ?? null, () => o.cover ?? NONE, o.now ?? 0);

/** The line as a person reads it: icons and keys named, texts as they are. */
const read = (parts: TagPart[]): string[] =>
  parts.flatMap((p) => {
    if (p.kind === "icon") return [`icon:${p.name}`];
    if (p.kind === "kbd") return [`kbd:${p.key}`];
    if (p.kind === "stack") return [p.head, p.sub];
    return [p.text];
  });
const at = (q: number, r: number): Cell => ({ q, r });

test("nothing pointed at: no tag", () => {
  assert.equal(tag(state({})), null);
});

test("bulldoze: a removable tile reads Remove with a quiet Click", () => {
  const m = tag(state({ tool: "bulldoze", bulldoze: { q: 0, r: 0, allowed: true, reason: null } }))!;
  assert.equal(m.sig, "rm|0,0");
  assert.deepEqual([m.bad, m.stacked], [false, false]);
  assert.deepEqual(read(m.parts), ["icon:bulldoze", "Remove Meadow", "Click"]);
  assert.deepEqual(m.parts[2], { kind: "fit", text: "Click", tone: "quiet" });
});

test("bulldoze: a refusal is a stacked bad tag, headline over reason", () => {
  const last = tag(
    state({ tool: "bulldoze", bulldoze: { q: 0, r: 0, allowed: false, reason: "That is the last tile" } }),
  )!;
  assert.equal(last.sig, "no|0,0|Place another before removing this one");
  assert.deepEqual([last.bad, last.stacked], [true, true]);
  assert.deepEqual(read(last.parts), [
    "icon:alert",
    "The world needs at least one tile",
    "Place another before removing this one",
  ]);
  const split = tag(
    state({ tool: "bulldoze", bulldoze: { q: 0, r: 0, allowed: false, reason: "Removing it would split the world" } }),
    road,
  )!;
  assert.equal(split.sig, "no|0,0|1 meadow tile would be cut off");
  const tunnel = tag(state({ tool: "bulldoze", bulldoze: { q: 0, r: 0, allowed: false, reason: "covers a tunnel" } }))!;
  assert.equal(tunnel.sig, "no|0,0|Remove one of its portals first");
});

test("placed: a just-placed tile shows its note until it expires, and only without a ghost elsewhere", () => {
  const placed = { cell: at(1, 0), level: 0, text: "Placed Meadow", until: 1000 };
  const m = tag(state({}), world, { placed, now: 500 })!;
  assert.equal(m.sig, "placed|1,0|Placed Meadow");
  assert.deepEqual([m.bad, m.stacked], [false, false]);
  assert.deepEqual(read(m.parts), ["icon:check", "Placed Meadow"]);
  assert.deepEqual(m.parts[0], { kind: "icon", name: "check", cls: "ic ok-ic" });
  assert.equal(tag(state({}), world, { placed, now: 1001 }), null, "past its time the note is gone");
  const elsewhere = state({ ghost: ghost(2, 0, "meadow", 0, [0, 1, 2, 3, 4, 5]) });
  assert.equal(tag(elsewhere, world, { placed, now: 500 })!.sig, "ok|2,0|meadow|Fits|0|false|false|false|false");
});

test("refused swap: names the selected tile and why it cannot replace the placed one", () => {
  const refusal = { cell: at(0, 0), tile: "forest", text: "A tunnel runs under it", until: 1000 };
  const m = tag(state({}), world, { refusal, now: 500 })!;
  assert.equal(m.sig, "swap|0,0|forest|A tunnel runs under it");
  assert.equal(m.bad, true);
  assert.deepEqual(read(m.parts), ["icon:alert", "Pine forest", "A tunnel runs under it"]);
  assert.equal(tag(state({}), world, { refusal, now: 1001 }), null);
});

/** Published fits whose lone entry has `steps` (member, rotation, level) as its R walk. */
const stepsAt = (slot: Cell, id: string, steps: [string, number, number][]) => ({
  slot,
  needs: [] as (string | null)[],
  stopped: [] as boolean[],
  nearest: null,
  entries: [{ id, orientations: steps, refusal: null, best: null }],
});

test("cover: the pointer on a tunnel's cover says so, unless the ghost is pinned there", () => {
  const over = state({ hovered_kind: "tile", hovered: at(0, 0) });
  const m = tag(over, world, { cover: new Set(["0,0"]) })!;
  assert.equal(m.sig, "cover|0,0");
  assert.equal(m.bad, true);
  assert.deepEqual(read(m.parts), ["icon:alert", "Meadow", "Covers a tunnel"], "the “Covers a tunnel” wording");
  const pinned = state({
    hovered_kind: "tile",
    hovered: at(0, 0),
    pinned: at(1, 0),
    fits: stepsAt(at(1, 0), "meadow", [
      ["meadow", 0, 0],
      ["meadow", 0, 1],
      ["meadow", 0, 2],
      ["meadow", 0, 3],
    ]),
    ghost: ghost(1, 0, "meadow", 0, [0, 1, 2, 3, 4, 5]),
  });
  assert.equal(tag(pinned, world, { cover: new Set(["0,0"]) })!.sig, "ok|1,0|meadow|Fits|0|false|false|true|true");
  assert.equal(tag(over, world, { cover: NONE }), null, "off a cover there is nothing to say");
});

test("ghost fits: one orientation reads Fits, several read Rotation i of n with the R hint, other levels the E/Q hint", () => {
  const meadowSteps: [string, number, number][] = [
    ["meadow", 0, 0],
    ["meadow", 0, 1],
    ["meadow", 0, 2],
    ["meadow", 0, 3],
  ];
  const meadow = tag(
    state({ fits: stepsAt(at(1, 0), "meadow", meadowSteps), ghost: ghost(1, 0, "meadow", 0, [0, 1, 2, 3, 4, 5]) }),
  )!;
  // One orientation per level: R has nowhere to go on the ground, E has three levels up.
  assert.equal(meadow.sig, "ok|1,0|meadow|Fits|0|false|false|true|false");
  assert.deepEqual(read(meadow.parts), ["Meadow", "Fits", "kbd:E/Q"]);
  const roadSteps: [string, number, number][] = [
    ["road_straight", 1, 0],
    ["road_straight", 2, 0],
  ];
  const first = tag(
    state({
      fits: stepsAt(at(1, 0), "road_straight", roadSteps),
      ghost: ghost(1, 0, "road_straight", 1, [1, 2, 4, 5]),
    }),
  )!;
  assert.equal(first.sig, "ok|1,0|road_straight|Rotation 1 of 2|0|true|false|false|false");
  assert.deepEqual(read(first.parts), ["Road straight", "Rotation 1 of 2", "kbd:R"], "rotation count 2");
  const second = tag(
    state({
      pinned: at(1, 0),
      fits: stepsAt(at(1, 0), "road_straight", roadSteps),
      ghost: ghost(1, 0, "road_straight", 2, [1, 2, 4, 5]),
    }),
  )!;
  assert.equal(second.sig, "ok|1,0|road_straight|Rotation 2 of 2|0|true|false|false|true");
  assert.deepEqual(read(second.parts), ["Road straight", "Rotation 2 of 2", "kbd:R", "Pinned · click to place"]);
});

test("ghost fits: a group counts only the ghost's member on its level, with the T hint while another fits", () => {
  // East of the start meadow: the headland fits one way, the bay three (hex-core order).
  const steps: [string, number, number][] = [
    ["beach_outer", 0, 0],
    ["beach_inner", 0, 0],
    ["beach_inner", 1, 0],
    ["beach_inner", 5, 0],
  ];
  const headland = tag(
    state({ fits: stepsAt(at(1, 0), "beach_curve", steps), ghost: ghost(1, 0, "beach_outer", 0, [0]) }),
  )!;
  assert.equal(headland.sig, "ok|1,0|beach_outer|Fits|0|false|true|false|false");
  assert.deepEqual(read(headland.parts), ["Beach Curve", "Fits", "kbd:T"], "R has nowhere to go, T has the bay");
  const bay = tag(
    state({ fits: stepsAt(at(1, 0), "beach_curve", steps), ghost: ghost(1, 0, "beach_inner", 1, [0, 1, 5]) }),
  )!;
  assert.equal(bay.sig, "ok|1,0|beach_inner|Rotation 2 of 3|0|true|true|false|false");
  assert.deepEqual(read(bay.parts), ["Beach Curve", "Rotation 2 of 3", "kbd:R", "kbd:T"]);
  const alone = tag(
    state({ fits: stepsAt(at(1, 0), "beach_curve", steps.slice(1)), ghost: ghost(1, 0, "beach_inner", 0, [0, 1, 5]) }),
  )!;
  assert.deepEqual(read(alone.parts), ["Beach Curve", "Rotation 1 of 3", "kbd:R"], "no T where only the bay fits");
});

test("ghost fits: a raised ghost adds its level and anchors there", () => {
  const m = tag(state({ ghost: ghost(1, 0, "river_bend", 2, [0, 2, 4, 5], { level: 1 }) }))!;
  assert.equal(m.sig, "ok|1,0|river_bend|Rotation 2 of 4|1|true|false|false|false");
  assert.deepEqual(read(m.parts), ["River bend", "Rotation 2 of 4", "Level 1", "kbd:R"]);
  assert.equal(m.level, 1);
});

test("ghost does not fit: the tile's own reason, a swap refusal on that cell, or the edge-clash count", () => {
  // The engine's published refusal for the entry words the tag ("missing_grass" → words.ts).
  const ocean = tag(
    state({ fits: fits(at(1, 0), "ocean", "missing_grass"), ghost: ghost(1, 0, "ocean", 0, [], { valid: false }) }),
  )!;
  assert.equal(ocean.sig, "bad|1,0|ocean|Needs grass or forest to meet the grass.|0");
  assert.deepEqual([ocean.bad, ocean.stacked], [true, false]);
  assert.deepEqual(read(ocean.parts), ["icon:alert", "Ocean", "Needs grass or forest to meet the grass."]);
  const refusal = { cell: at(1, 0), tile: "ocean", text: "Not here", until: 1000 };
  const refused = tag(
    state({ fits: fits(at(1, 0), "ocean", "missing_grass"), ghost: ghost(1, 0, "ocean", 0, [], { valid: false }) }),
    world,
    { refusal, now: 500 },
  )!;
  assert.equal(refused.sig, "bad|1,0|ocean|Not here|0");
  // A tile that fits at some rotation falls to the ghost's own edge-clash count.
  const clash = tag(
    state({
      ghost: ghost(1, 0, "meadow", 0, [0, 1], {
        valid: false,
        edges: ["Incompatible", "Incompatible", "Neutral", "Neutral", "Neutral", "Neutral"],
      }),
    }),
  )!;
  assert.equal(clash.sig, "bad|1,0|meadow|2 edges clash|0");
  assert.deepEqual(read(clash.parts), ["icon:alert", "Meadow", "2 edges clash", "kbd:R"], "a fitting tile keeps R");
});

test("ghost of a footprint tile: its refusal is the entry's, not the single-tile fit's", () => {
  const outer = new Array(18).fill("Neutral");
  const m = tag(
    state({
      selected: "massif_granite",
      fits: fits(at(1, 0), "massif_granite", "footprint_edges"),
      ghost: ghost(1, 0, "massif_granite", 0, [], { valid: false, outer_edges: outer, footprint: [] }),
    }),
  )!;
  assert.equal(m.sig, "bad|1,0|massif_granite|Its edges don't meet the tiles beside it.|0");
  assert.deepEqual(read(m.parts), ["icon:alert", "Granite massif", "Its edges don't meet the tiles beside it."]);
});

test("the cover set is read only when the pointer is on a placed tile", () => {
  let reads = 0;
  const cover = () => {
    reads++;
    return NONE;
  };
  tagModel(state({}), world, null, null, cover, 0);
  tagModel(state({ ghost: ghost(1, 0, "meadow", 0, [0]) }), world, null, null, cover, 0);
  assert.equal(reads, 0);
  tagModel(state({ hovered_kind: "tile", hovered: at(0, 0) }), world, null, null, cover, 0);
  assert.equal(reads, 1);
});
