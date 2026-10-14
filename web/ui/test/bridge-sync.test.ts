// The TypeScript bridge types against the Rust contract. bridge-schema.json is regenerated from the
// real enums by the hex-app test `bridge::schema::schema_matches_web_ui` (make ui-schema), so a
// command, event or state field added in Rust fails here until bridge.ts learns it.
import assert from "node:assert/strict";
import { test } from "node:test";
import { COMMAND_FIELDS, ENUMS, EVENT_FIELDS, STATE_FIELDS } from "../src/bridge.ts";
import schema from "../src/bridge-schema.json" with { type: "json" };

const sorted = (o: Record<string, string[]>): Record<string, string[]> =>
  Object.fromEntries(Object.entries(o).sort(([a], [b]) => a.localeCompare(b)));

test("every Rust command and its fields are typed", () => {
  assert.deepEqual(sorted(COMMAND_FIELDS), sorted(schema.commands));
});

test("every Rust game event and its fields are typed", () => {
  assert.deepEqual(sorted(EVENT_FIELDS), sorted(schema.events));
});

test("enum spellings match serde (tool snake_case, EdgeStatus PascalCase)", () => {
  assert.deepEqual(sorted(ENUMS), sorted(schema.enums));
});

test("play state keys and nested shapes match", () => {
  assert.deepEqual(sorted(STATE_FIELDS), sorted(schema.state));
});
