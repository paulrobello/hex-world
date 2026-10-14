// The Settings sheet's model: which quality row is selected from `state().tier`, the rows offered,
// the detected tier with its reason, the Ultra note, roving keys, and the audio channel table.
import assert from "node:assert/strict";
import { test } from "node:test";
import type { TierState } from "../src/bridge.ts";
import {
  CHANNELS,
  autoTier,
  percent,
  qualityView,
  roveIndex,
  selectedQuality,
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
    ...over,
  };
}

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
