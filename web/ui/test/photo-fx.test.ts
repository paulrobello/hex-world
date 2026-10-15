// The photo panel's Effects model: its fields match the bridge contract, the defaults are every
// effect off, the off rule matches the engine's, and the slider percent round-trips.
import assert from "node:assert/strict";
import { test } from "node:test";
import type { PhotoState } from "../src/bridge.ts";
import schema from "../src/bridge-schema.json" with { type: "json" };
import {
  FILTERS,
  FX_DEFAULTS,
  FX_FIELDS,
  type FxValues,
  fxAnyOn,
  fxDisabled,
  fxFrom,
  fxIsDefault,
  pctLabel,
  toPercent,
  toUnit,
} from "../src/photo-fx-model.ts";

test("every effect field is a photo command field and a photo state field", () => {
  for (const f of FX_FIELDS) {
    assert.ok(schema.commands.photo.includes(f), `photo command lacks ${f}`);
    assert.ok(schema.state.photo.includes(f), `photo state lacks ${f}`);
  }
  assert.ok(schema.state.photo.includes("fx_active"));
});

test("the filter buttons are the engine's filters, None first", () => {
  assert.deepEqual(FILTERS.map((f) => f.value).sort(), [...schema.enums.photo_filter].sort());
  assert.equal(FILTERS[0]?.value, "normal");
});

test("the defaults are every effect off, and Reset effects has nothing to do there", () => {
  assert.equal(fxAnyOn(FX_DEFAULTS), false);
  assert.equal(fxIsDefault(FX_DEFAULTS), true);
  assert.equal(fxIsDefault({ ...FX_DEFAULTS, vignette_softness: 0.501 }), true, "rounding noise is not a change");
  assert.equal(fxIsDefault({ ...FX_DEFAULTS, vignette_softness: 0.2 }), false, "softness alone still resets");
  assert.equal(fxAnyOn({ ...FX_DEFAULTS, vignette_softness: 0.2 }), false, "but shows nothing");
});

test("an effect is on as the engine counts it", () => {
  const on = (v: Partial<FxValues>) => fxAnyOn({ ...FX_DEFAULTS, ...v });
  assert.equal(on({ filter: "mono" }), true);
  assert.equal(on({ filter: "sepia", filter_strength: 0 }), false, "a filter at strength 0 is off");
  assert.equal(on({ filter_strength: 0.4 }), false, "strength without a filter is off");
  assert.equal(on({ vignette: 0.3 }), true);
  assert.equal(on({ grain: 0.01 }), true);
  assert.equal(on({ aberration: 0.004 }), false, "at or below 0.005 is off");
});

test("inert controls: strength without a filter, softness without a vignette", () => {
  assert.deepEqual(fxDisabled(FX_DEFAULTS), { strength: true, softness: true });
  assert.deepEqual(fxDisabled({ ...FX_DEFAULTS, filter: "sepia", vignette: 0.5 }), {
    strength: false,
    softness: false,
  });
});

test("slider percent round-trips and clamps", () => {
  assert.equal(toPercent(0.35), 35);
  assert.equal(toPercent(1.7), 100);
  assert.equal(toPercent(-1), 0);
  assert.equal(toUnit(35), 0.35);
  assert.equal(toUnit(140), 1);
  assert.equal(pctLabel(35), "35%");
});

test("the panel reads the effect values out of the photo state", () => {
  const state = {
    on: true,
    filter: "sepia",
    filter_strength: 0.7,
    vignette: 0.4,
    vignette_softness: 0.6,
    grain: 0.2,
    aberration: 0.1,
    fx_active: true,
  } as unknown as PhotoState;
  assert.deepEqual(fxFrom(state), {
    filter: "sepia",
    filter_strength: 0.7,
    vignette: 0.4,
    vignette_softness: 0.6,
    grain: 0.2,
    aberration: 0.1,
  });
});
