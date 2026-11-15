// What a game key does once the HUD owns it (key-model.ts): the shortcuts, the repeat rules and the
// Escape chain in the order hud.ts has always offered it.
import assert from "node:assert/strict";
import { test } from "node:test";
import { type KeyFlags, escapeStep, keyAction, normalKey } from "../src/key-model.ts";

const OPEN: KeyFlags = { worldsOpen: false, settingsOpen: false, hidden: false };
const st = (o: object = {}) => ({ pinned: null, tool: "place" as const, ghost: null, ...o }) as never;
const press = (key: string, o: { shiftKey?: boolean; repeat?: boolean; code?: string } = {}) => ({
  key,
  code: o.code ?? `Key${key.toUpperCase()}`,
  shiftKey: o.shiftKey ?? false,
  repeat: o.repeat ?? false,
});
const act = (key: string, o = {}, flags: KeyFlags = OPEN, state = st(), mod = false, onRow = false) =>
  keyAction(state, flags, press(key, o), mod, onRow);

test("keys are normalised: a character lower-cased, a named key as it is", () => {
  assert.equal(normalKey({ key: "R" }), "r");
  assert.equal(normalKey({ key: "Escape" }), "Escape");
  assert.equal(normalKey({ key: "F6" }), "F6");
});

test("the game keys map to their actions, with Shift and repeat carried through", () => {
  assert.deepEqual(act("r"), { kind: "rotate", back: false, repeat: false });
  assert.deepEqual(act("R", { shiftKey: true }), { kind: "rotate", back: true, repeat: false });
  assert.deepEqual(act("r", { repeat: true }), { kind: "rotate", back: false, repeat: true });
  assert.deepEqual(act("b"), { kind: "toggle_bulldoze", repeat: false });
  assert.deepEqual(act("p", { repeat: true }), { kind: "toggle_photo", repeat: true });
  assert.deepEqual(act("h"), { kind: "toggle_hidden", repeat: false });
  assert.deepEqual(act("f"), { kind: "enter_walk", repeat: false });
  assert.deepEqual(act("F6", { shiftKey: true }), { kind: "cycle_focus", back: true });
  assert.deepEqual(act("x"), { kind: "leave" }, "any other key is not the HUD's");
});

test("Ctrl or Cmd with Z is undo, with Shift redo; any other chord does nothing", () => {
  assert.deepEqual(act("z", {}, OPEN, st(), true), { kind: "history", redo: false });
  assert.deepEqual(act("Z", { shiftKey: true }, OPEN, st(), true), { kind: "history", redo: true });
  assert.deepEqual(act("r", {}, OPEN, st(), true), { kind: "ignore" });
  assert.deepEqual(act("Escape", {}, OPEN, st(), true), { kind: "ignore" });
});

test("Tab is its own key whatever the character, and the HUD leaves it", () => {
  assert.deepEqual(keyAction(st(), OPEN, press("Tab", { code: "Tab" }), false, false), { kind: "leave" });
});

test("Enter places the ghost where it stands, nothing without one, and a row or button keeps it", () => {
  const ghost = { q: 3, r: -1 };
  assert.deepEqual(act("Enter", {}, OPEN, st({ ghost })), { kind: "place_ghost", at: { q: 3, r: -1 } });
  assert.deepEqual(act("Enter"), { kind: "place_ghost", at: null });
  assert.deepEqual(act("Enter", {}, OPEN, st({ ghost, tool: "bulldoze" })), { kind: "place_ghost", at: null });
  assert.deepEqual(act("Enter", {}, OPEN, st({ ghost }), false, true), { kind: "leave" });
});

test("Escape goes to the first of: unpin, Worlds, Settings, leave bulldoze, show the HUD, clear the slot", () => {
  const every: KeyFlags = { worldsOpen: true, settingsOpen: true, hidden: true };
  const chain = [
    [st({ pinned: { q: 1, r: 0 }, tool: "bulldoze" }), every, "unpin"],
    [st({ tool: "bulldoze" }), every, "close_worlds"],
    [st({ tool: "bulldoze" }), { ...every, worldsOpen: false }, "close_settings"],
    [st({ tool: "bulldoze" }), { worldsOpen: false, settingsOpen: false, hidden: true }, "leave_bulldoze"],
    [st(), { worldsOpen: false, settingsOpen: false, hidden: true }, "show_hud"],
    [st(), OPEN, "clear_slot"],
  ] as const;
  for (const [state, flags, step] of chain) {
    assert.equal(escapeStep(state, flags), step);
    assert.deepEqual(act("Escape", {}, flags, state), { kind: "escape", step });
  }
});
