// Photo mode's click to focus: clicks are short, nearly still left presses; drags (even ones that
// come back), slow presses and other buttons are not; the click lands in viewport fractions.
import assert from "node:assert/strict";
import { test } from "node:test";
import {
  CLICK_MAX_MS,
  CLICK_MAX_PX,
  ClickDetector,
  FOCUS_MAX,
  FOCUS_MIN,
  focusAt,
  focusSliderValue,
} from "../src/photo-focus.ts";

const at = (x: number, y: number, t: number, button = 0) => ({ x, y, t, button });

test("a quick left press that barely moves is a click", () => {
  const c = new ClickDetector();
  c.press(at(100, 200, 0));
  c.move({ x: 102, y: 201 });
  assert.equal(c.release(at(103, 202, 120)), true);
  c.press(at(100, 200, 0));
  assert.equal(c.release(at(100 + CLICK_MAX_PX, 200, CLICK_MAX_MS)), true, "the limits themselves still click");
});

test("a drag is not a click, even one that comes back to where it started", () => {
  const c = new ClickDetector();
  c.press(at(100, 200, 0));
  c.move({ x: 140, y: 210 });
  assert.equal(c.release(at(140, 210, 150)), false, "a 40 px look");
  c.press(at(100, 200, 0));
  c.move({ x: 100 + CLICK_MAX_PX + 1, y: 200 });
  c.move({ x: 100, y: 200 });
  assert.equal(c.release(at(100, 200, 100)), false, "out and back");
  c.press(at(100, 200, 0));
  assert.equal(c.release(at(110, 200, 50)), false, "a jump between move events still counts at release");
});

test("a slow press, another button, or a release with no press is not a click", () => {
  const c = new ClickDetector();
  c.press(at(100, 200, 0));
  assert.equal(c.release(at(100, 200, CLICK_MAX_MS + 1)), false, "held too long");
  c.press(at(100, 200, 0, 2));
  assert.equal(c.release(at(100, 200, 50, 2)), false, "the right button looks, it never focuses");
  assert.equal(c.release(at(100, 200, 60)), false, "no press");
  c.press(at(100, 200, 0));
  c.cancel();
  assert.equal(c.release(at(100, 200, 60)), false, "a cancelled press");
});

test("focus_at is the click across the canvas from its top left", () => {
  const rect = { left: 0, top: 0, width: 1440, height: 900 };
  assert.deepEqual(focusAt(720, 450, rect), [0.5, 0.5]);
  assert.deepEqual(focusAt(360, 810, rect), [0.25, 0.9]);
  assert.deepEqual(focusAt(110, 70, { left: 10, top: 20, width: 200, height: 100 }), [0.5, 0.5]);
  assert.equal(focusAt(-1, 450, rect), null);
  assert.equal(focusAt(10, 10, { left: 0, top: 0, width: 0, height: 0 }), null);
});

test("the focus slider spans the engine's focus range in whole metres", () => {
  assert.equal(FOCUS_MIN, 1);
  assert.equal(FOCUS_MAX, 1000);
  assert.equal(focusSliderValue(23.47), 23);
  assert.equal(focusSliderValue(0.4), 1);
  assert.equal(focusSliderValue(640.2), 640, "past the old 400 m end");
  assert.equal(focusSliderValue(5000), 1000);
});
