// The walk speed setting's model: the range and step the slider and the engine share, the stored
// value read back at startup, the bridge command, and the slider's fill and readout.
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  clampWalkSpeed,
  SPRINT_FACTOR,
  speedLabel,
  storedWalkSpeed,
  WALK_SPEED_KEY,
  WALK_SPEED_MAX,
  WALK_SPEED_MIN,
  walkSpeedCommand,
  walkSpeedPct,
} from "../src/walk-speed.ts";

test("the range starts at the new default walk speed and sprint is twice it", () => {
  assert.equal(WALK_SPEED_MIN, 2.8);
  assert.equal(WALK_SPEED_MAX, 8);
  assert.equal(SPRINT_FACTOR, 2);
  assert.equal(WALK_SPEED_KEY, "hexWorld.walkSpeed");
});

test("speeds clamp to the range on the 0.1 m/s step", () => {
  assert.equal(clampWalkSpeed(1), 2.8);
  assert.equal(clampWalkSpeed(20), 8);
  assert.equal(clampWalkSpeed(5), 5);
  assert.equal(clampWalkSpeed(4.26), 4.3);
  assert.equal(clampWalkSpeed(2.8000000001), 2.8);
});

test("a stored speed is read back clamped; nothing or junk stored reads as none", () => {
  assert.equal(storedWalkSpeed(null), null);
  assert.equal(storedWalkSpeed(""), null);
  assert.equal(storedWalkSpeed("fast"), null);
  assert.equal(storedWalkSpeed("NaN"), null);
  assert.equal(storedWalkSpeed("5"), 5);
  assert.equal(storedWalkSpeed("12"), 8);
  assert.equal(storedWalkSpeed("0.5"), 2.8);
});

test("the command carries the clamped speed; the slider fills and reads in m/s", () => {
  assert.deepEqual(walkSpeedCommand(6.04), { cmd: "set_walk_speed", speed: 6 });
  assert.deepEqual(walkSpeedCommand(99), { cmd: "set_walk_speed", speed: 8 });
  assert.equal(walkSpeedPct(2.8), 0);
  assert.equal(walkSpeedPct(8), 100);
  assert.ok(Math.abs(walkSpeedPct(5.4) - 50) < 1e-9);
  assert.equal(speedLabel(2.8), "2.8 m/s");
  assert.equal(speedLabel(5), "5.0 m/s");
});
