// The Settings sheet's model: which quality row is selected from `state().tier`, the rows offered,
// the detected tier with its reason, the Ultra note, roving keys, the audio channel table, and the
// colour grade rows with the startup choice between the stored grade and the page address, and the
// grass density slider the same way (disabled below High, stepped choices migrating to percents).
import assert from "node:assert/strict";
import { test } from "node:test";
import type { GradeState, GrassState, TierState } from "../src/bridge.ts";
import {
  CHANNELS,
  GRADE_KEY,
  GRASS_KEY,
  GRASS_SLIDER,
  grassCommand,
  grassPercent,
  grassView,
  startupGrass,
  storedGrass,
  throttle,
  autoTier,
  gradeCommand,
  gradeView,
  isGrade,
  percent,
  startupGrade,
  qualityView,
  roveIndex,
  selectedQuality,
  tierBusy,
  tierCommand,
  ultraAllowed,
} from "../src/settings-model.ts";

const WEB_HIGH = "web: adapter masked; BC textures and limits meet High (warm-up probe may step down)";

function tier(over: Partial<TierState> = {}): TierState {
  return {
    tier: "high",
    source: "detected",
    reason: WEB_HIGH,
    detected: { tier: "high", reason: WEB_HIGH },
    probe: { median_ms: 8.34, kept: 60, hitches: 0, tier: "high", reason: "warm-up median 8.3 ms <= 25 ms" },
    changes: 1,
    drawn: 1,
    requests: 0,
    rebuilding: false,
    ...over,
  };
}

test("the tier-change layer holds until the request is handled and its rebuild is drawn", () => {
  assert.equal(tierBusy(undefined, null), false);
  assert.equal(tierBusy(tier(), null), false);
  // Sent at 3 requests: up until the engine has handled it.
  assert.equal(tierBusy(tier({ requests: 3 }), 3), true);
  assert.equal(tierBusy(undefined, 3), true);
  // Handled and rebuilding (changes ahead of drawn), then drawn.
  assert.equal(tierBusy(tier({ requests: 4, changes: 2, drawn: 1 }), 3), true);
  assert.equal(tierBusy(tier({ requests: 4, changes: 2, drawn: 2 }), 3), false);
  // A no-op request (same tier) moves only the request count.
  assert.equal(tierBusy(tier({ requests: 4 }), 3), false);
  // A rebuild nobody here asked for (a loaded save, the probe) also holds it.
  assert.equal(tierBusy(tier({ changes: 3, drawn: 2 }), null), true);
});

test("Auto is selected while detection owns the tier; a picked or keyed tier selects its row", () => {
  assert.equal(selectedQuality(undefined), "auto");
  for (const source of ["default", "detected", "probe"] as const)
    assert.equal(selectedQuality(tier({ source })), "auto");
  assert.equal(selectedQuality(tier({ source: "runtime", tier: "low" })), "low");
  assert.equal(selectedQuality(tier({ source: "explicit", tier: "medium" })), "medium");
});

test("rows: Auto names the tier it uses, the detected row is tagged, Ultra only where detection lands on High", () => {
  const v = qualityView(tier());
  assert.deepEqual(
    v.rows.map((r) => r.id),
    ["auto", "low", "medium", "high", "ultra"],
  );
  assert.equal(v.rows[0]!.tag, "Uses High here");
  assert.equal(v.rows[0]!.detail, "Chosen from this GPU and a 8.3 ms warm-up");
  assert.deepEqual(
    v.rows.filter((r) => r.detected).map((r) => [r.id, r.tag]),
    [["high", "Detected"]],
  );
  // The probe stepped down: Auto uses Medium, Ultra is not offered.
  const slow = tier({
    tier: "medium",
    source: "probe",
    probe: { median_ms: 31.2, kept: 60, hitches: 2, tier: "medium", reason: "warm-up median 31.2 ms > 25 ms" },
  });
  assert.equal(autoTier(slow), "medium");
  assert.equal(ultraAllowed(slow), false);
  assert.deepEqual(
    qualityView(slow).rows.map((r) => r.id),
    ["auto", "low", "medium", "high"],
  );
  assert.equal(qualityView(slow).rows[0]!.tag, "Uses Medium here");
  // A `tier=ultra` key still shows its row.
  assert.equal(ultraAllowed(tier({ ...slow, tier: "ultra", source: "explicit" })), true);
});

test("the reason line states the detected tier, the rules' reason and the warm-up", () => {
  assert.equal(qualityView(tier()).reason, `Detected High: ${WEB_HIGH}. Warm-up 8.3 ms a frame`);
  const stepped = qualityView(
    tier({ probe: { median_ms: 31.2, kept: 60, hitches: 0, tier: "medium", reason: "slow" }, source: "probe" }),
  );
  assert.match(stepped.reason!, /^Detected Medium: .*Warm-up 31\.2 ms a frame, one tier down$/);
  const native = qualityView(
    tier({
      probe: null,
      detected: { tier: "high", reason: "Apple M4 Max: Apple silicon, the reference desktop class" },
    }),
  );
  assert.equal(native.reason, "Detected High: Apple M4 Max: Apple silicon, the reference desktop class");
  assert.equal(native.rows[0]!.detail, "Chosen from this GPU");
  const keyed = qualityView(tier({ source: "explicit", tier: "medium", probe: null }));
  assert.match(keyed.reason!, /The page address sets Medium$/);
});

test("before the render device exists there is no reason line and Auto says it is detecting", () => {
  const v = qualityView(tier({ source: "default", detected: null, probe: null, reason: "default" }));
  assert.equal(v.reason, null);
  assert.equal(v.autoTier, null);
  assert.equal(v.rows[0]!.tag, "Detecting…");
  assert.equal(v.selected, "auto");
  assert.equal(qualityView(undefined).rows.length, 4, "no Ultra before detection");
});

test("the Ultra note shows only while Ultra is selected", () => {
  assert.equal(qualityView(tier()).caution, null);
  const ultra = qualityView(tier({ tier: "ultra", source: "runtime" }));
  assert.equal(ultra.selected, "ultra");
  assert.equal(ultra.caution?.title, "Detection chose High for this GPU");
  assert.match(ultra.caution!.body, /High ran 8\.3 ms a frame in the warm-up\.$/);
});

test("set_tier commands and roving keys", () => {
  assert.deepEqual(tierCommand("auto"), { cmd: "set_tier", tier: "auto" });
  assert.deepEqual(tierCommand("low"), { cmd: "set_tier", tier: "low" });
  assert.equal(roveIndex(0, 5, "ArrowDown", "y"), 1);
  assert.equal(roveIndex(0, 5, "ArrowUp", "y"), 4, "wraps");
  assert.equal(roveIndex(4, 5, "ArrowDown", "y"), 0);
  assert.equal(roveIndex(1, 2, "ArrowRight", "x"), 0);
  assert.equal(roveIndex(0, 2, "ArrowLeft", "x"), 1);
  assert.equal(roveIndex(2, 5, "Home", "y"), 0);
  assert.equal(roveIndex(2, 5, "End", "y"), 4);
  assert.equal(roveIndex(0, 2, "ArrowDown", "x"), null, "the other axis is left alone");
});

test("audio: the mockup's channels in order, levels as whole percent", () => {
  assert.deepEqual(
    CHANNELS.map((c) => c.label),
    ["Master", "Music", "Ambience", "Effects", "Interface"],
  );
  assert.equal(CHANNELS.find((c) => c.hint)?.bus, "ambience");
  assert.equal(percent(0.55), 55);
  assert.equal(percent(0.755), 76);
  assert.equal(percent(1.4), 100);
  assert.equal(percent(-1), 0);
});

const GRADE: GradeState = {
  name: "rich-3",
  default: "rich-3",
  presets: [
    { name: "aces", detail: "The first look" },
    { name: "tony", detail: "Every wall readable" },
    { name: "rich-1", detail: "A fifth more colour" },
    { name: "rich-2", detail: "Richer greens" },
    { name: "rich-3", detail: "Shadows lifted" },
  ],
};

test("colour grade: one row per engine preset in its order, readable names, the engine's details", () => {
  const v = gradeView(GRADE);
  assert.deepEqual(
    v.rows.map((r) => [r.id, r.name, r.detail]),
    [
      ["aces", "ACES", "The first look"],
      ["tony", "Tony", "Every wall readable"],
      ["rich-1", "Rich 1", "A fifth more colour"],
      ["rich-2", "Rich 2", "Richer greens"],
      ["rich-3", "Rich 3", "Shadows lifted"],
    ],
  );
  assert.equal(v.selected, "rich-3", "the default grade is checked when nothing else is chosen");
  assert.deepEqual(
    v.rows.filter((r) => r.tag).map((r) => [r.id, r.tag]),
    [["rich-3", "Default"]],
    "only the engine's default row is labelled",
  );
  const picked = gradeView({ ...GRADE, name: "aces" });
  assert.equal(picked.selected, "aces", "another grade in force is checked");
  assert.equal(picked.rows.find((r) => r.tag)?.id, "rich-3", "and the default label stays on the default row");
  assert.equal(gradeView(GRADE, "rich-2").selected, "rich-2", "a pending pick shows until the engine follows");
  assert.deepEqual(gradeView(undefined), { selected: null, rows: [] }, "no rows before the engine reports grades");
  assert.deepEqual(gradeCommand("aces"), { cmd: "set_grade", grade: "aces" });
});

test("colour grade at startup: the stored choice, unless the address names one or it is unknown", () => {
  assert.equal(startupGrade("", "rich-2"), "rich-2");
  assert.equal(startupGrade("?scenario=play&tilt=off", "aces"), "aces");
  assert.equal(startupGrade("?scenario=play&grade=aces", "rich-2"), null, "?grade= wins");
  assert.equal(startupGrade("?grade=", "rich-2"), null, "an empty grade key still belongs to the engine");
  assert.equal(startupGrade("", null), null, "nothing stored (or storage blocked)");
  assert.equal(startupGrade("", "vivid"), null, "a stale name never reaches the engine");
  assert.equal(GRADE_KEY, "hexWorld.grade");
  assert.equal(isGrade("rich-3"), true);
  assert.equal(isGrade(undefined), false);
});

test("grass: a 0-150% slider in 5% steps, the engine's share shown, disabled below High", () => {
  assert.deepEqual(GRASS_SLIDER, { min: 0, max: 150, step: 5, normal: 100 });
  const g = { density: 1 } as GrassState;
  const v = grassView(g, tier());
  assert.equal(v.value, 100);
  assert.equal(v.enabled, true);
  assert.equal(grassView({ density: 1.35 } as GrassState, tier()).value, 135);
  assert.equal(grassView(g, tier({ tier: "ultra" })).enabled, true);
  assert.equal(grassView(g, tier({ tier: "medium" })).enabled, false, "Medium draws no grass");
  assert.equal(grassView(g, tier({ tier: "low" })).enabled, false, "Low draws no grass");
  assert.equal(grassView(g, tier(), 150).value, 150, "a pending value shows until the engine follows");
  assert.equal(grassView(undefined, undefined).value, null);
  assert.deepEqual(grassCommand(55), { cmd: "set_grass", density: 0.55 });
  assert.deepEqual(grassCommand(0), { cmd: "set_grass", density: 0 });
  assert.deepEqual(grassCommand(400), { cmd: "set_grass", density: 1.5 }, "clamped to the slider");
  assert.equal(grassPercent(97), 95);
  assert.equal(grassPercent(-3), 0);
});

test("grass at startup: the stored percent, a stepped choice migrated, unless the address names one", () => {
  assert.equal(startupGrass("", "150"), 150);
  assert.equal(startupGrass("", "0"), 0, "0% is a stored choice (off), not a missing one");
  assert.equal(startupGrass("?scenario=play&grass=0.5", "150"), null, "?grass= wins for the visit");
  assert.equal(startupGrass("", null), null);
  assert.equal(startupGrass("", "thick"), null, "a stale name never reaches the engine");
  // The stepped choice the slider replaced: off 0, sparse 50, normal 100, lush 150.
  assert.deepEqual(
    ["off", "sparse", "normal", "lush"].map((s) => storedGrass(s)),
    [0, 50, 100, 150],
  );
  assert.equal(storedGrass("120"), 120);
  assert.equal(storedGrass("-5"), null);
  assert.equal(GRASS_KEY, "hexWorld.grass");
});

test("grass slider sends at most every 150 ms while dragging and the last value on release", () => {
  let t = 0;
  const sent: number[] = [];
  const s = throttle(
    (v) => sent.push(v),
    150,
    () => t,
  );
  s.push(100);
  s.push(105);
  s.push(110);
  assert.deepEqual(sent, [100], "the first move goes at once, the rest wait");
  t = 200;
  s.push(115);
  assert.deepEqual(sent, [100, 115]);
  t = 250;
  s.push(120);
  s.flush();
  assert.deepEqual(sent, [100, 115, 120], "release sends the last value at once");
  s.flush();
  assert.deepEqual(sent, [100, 115, 120], "nothing pending, nothing sent");
});
