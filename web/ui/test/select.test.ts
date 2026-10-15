// The compact select's keyboard model (the colour grade): closed keys that open it, moving the
// highlight without picking, Enter and Space picking, Esc and Tab closing with no change, and
// type-ahead by name with its buffer and timeout.
import assert from "node:assert/strict";
import { test } from "node:test";
import { selectKey, TYPE_AHEAD_MS, typeAhead, typeBuffer, typesAhead } from "../src/select-model.ts";

const NAMES = ["ACES", "Tony", "Rich 1", "Rich 2", "Rich 3"];

test("closed: Down and Up open on the current option, Home and End on the ends; Enter and Space are the button's", () => {
  assert.deepEqual(selectKey(false, 4, 5, "ArrowDown"), { kind: "open", active: 4 });
  assert.deepEqual(selectKey(false, 2, 5, "ArrowUp"), { kind: "open", active: 2 });
  assert.deepEqual(selectKey(false, 2, 5, "Home"), { kind: "open", active: 0 });
  assert.deepEqual(selectKey(false, 2, 5, "End"), { kind: "open", active: 4 });
  assert.deepEqual(
    selectKey(false, -1, 5, "ArrowDown"),
    { kind: "open", active: 0 },
    "no current option opens on the first",
  );
  assert.equal(selectKey(false, 2, 5, "Enter"), null, "Enter clicks the button, which opens the list");
  assert.equal(selectKey(false, 2, 5, " "), null, "Space clicks the button too");
  assert.equal(selectKey(false, 2, 5, "Escape"), null);
  assert.equal(selectKey(false, 2, 0, "ArrowDown"), null, "nothing opens without options");
});

test("open: arrows move the highlight without picking or wrapping; Home, End and the page keys jump", () => {
  assert.deepEqual(selectKey(true, 3, 5, "ArrowUp"), { kind: "move", active: 2 });
  assert.deepEqual(selectKey(true, 3, 5, "ArrowDown"), { kind: "move", active: 4 });
  assert.deepEqual(selectKey(true, 4, 5, "ArrowDown"), { kind: "move", active: 4 }, "the last option stays");
  assert.deepEqual(selectKey(true, 0, 5, "ArrowUp"), { kind: "move", active: 0 }, "the first option stays");
  assert.deepEqual(selectKey(true, 3, 5, "Home"), { kind: "move", active: 0 });
  assert.deepEqual(selectKey(true, 1, 5, "End"), { kind: "move", active: 4 });
  assert.deepEqual(selectKey(true, 3, 5, "PageUp"), { kind: "move", active: 0 });
  assert.deepEqual(selectKey(true, 1, 5, "PageDown"), { kind: "move", active: 4 });
  assert.deepEqual(selectKey(true, -1, 5, "ArrowDown"), { kind: "move", active: 0 });
  assert.deepEqual(selectKey(true, -1, 5, "ArrowUp"), { kind: "move", active: 4 });
});

test("open: Enter and Space pick the highlighted option; Esc and Tab close with no change", () => {
  assert.deepEqual(selectKey(true, 2, 5, "Enter"), { kind: "pick", index: 2 });
  assert.deepEqual(selectKey(true, 2, 5, " "), { kind: "pick", index: 2 });
  assert.equal(selectKey(true, -1, 5, "Enter"), null, "nothing highlighted, nothing picked");
  assert.deepEqual(selectKey(true, 2, 5, "Escape"), { kind: "close" });
  assert.deepEqual(selectKey(true, 2, 5, "Tab"), { kind: "close" });
  assert.equal(selectKey(true, 2, 5, "x"), null, "letters belong to type-ahead");
});

test("type-ahead: a letter finds the next name starting with it, ignoring case, wrapping", () => {
  assert.equal(typeAhead(NAMES, 4, "t"), 1);
  assert.equal(typeAhead(NAMES, 0, "A"), 0, "wraps back round to the only match");
  assert.equal(typeAhead(NAMES, 1, "a"), 0);
  assert.equal(typeAhead(NAMES, 1, "z"), null, "no match leaves the highlight");
  assert.equal(typeAhead(NAMES, -1, "a"), 0, "nothing highlighted searches from the top");
  assert.equal(typeAhead([], 0, "a"), null);
  assert.equal(typeAhead(NAMES, 0, ""), null);
});

test("type-ahead: a repeated letter cycles Rich 1, 2 and 3; a longer query narrows", () => {
  assert.equal(typeAhead(NAMES, 0, "r"), 2);
  assert.equal(typeAhead(NAMES, 2, "rr"), 3);
  assert.equal(typeAhead(NAMES, 3, "rrr"), 4);
  assert.equal(typeAhead(NAMES, 4, "rrrr"), 2, "and round again");
  assert.equal(typeAhead(NAMES, 2, "ri"), 2, "a longer query may stay on the current option");
  assert.equal(typeAhead(NAMES, 2, "rich 3"), 4);
  assert.equal(typeAhead(NAMES, 4, "rich 2"), 3, "searching wraps");
  assert.equal(typeAhead(NAMES, 0, "to"), 1);
});

test("type-ahead buffer: keys within the timeout extend it, a pause restarts it", () => {
  assert.equal(typeBuffer("", 0, "r", 1000), "r");
  assert.equal(typeBuffer("r", 1000, "i", 1000 + TYPE_AHEAD_MS), "ri");
  assert.equal(typeBuffer("ri", 1000, "t", 1001 + TYPE_AHEAD_MS), "t", "a pause starts a new search");
});

test("type-ahead takes printable keys, and Space only while a search runs", () => {
  assert.equal(typesAhead("r", "", 0, 5000), true);
  assert.equal(typesAhead("2", "rich", 4900, 5000), true);
  assert.equal(typesAhead(" ", "rich", 4900, 5000), true, "Space inside a search is part of the name");
  assert.equal(typesAhead(" ", "rich", 5000 - TYPE_AHEAD_MS - 1, 5000), false, "after a pause Space picks");
  assert.equal(typesAhead(" ", "", 0, 5000), false, "with no search Space picks");
  assert.equal(typesAhead("Enter", "rich", 4900, 5000), false);
  assert.equal(typesAhead("ArrowDown", "", 0, 5000), false);
});
