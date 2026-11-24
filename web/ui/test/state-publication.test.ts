import assert from "node:assert/strict";
import { test } from "node:test";
import { installShim, type WasmBindings } from "../src/shim.ts";

/** The wasm stub and the globals installShim touches (window, location, document), so the real
 * shim assembly runs under node --test; the globals are removed after each use. */
function bridge() {
  const patches: string[] = [];
  let inbox = 0;
  let events: unknown[] = [];
  const w: WasmBindings = {
    hex_world_ready: () => true,
    hex_world_state_json: () => patches.shift() ?? "",
    hex_world_diag_json: () => "{}",
    hex_world_events: () => JSON.stringify(events.splice(0)),
    hex_world_cell_rect: () => "null",
    hex_world_cmd: () => true,
    hex_world_inbox_len: () => inbox,
    hex_world_screenshot: () => {},
    hex_world_take_shot: () => null,
    hex_world_take_save: () => null,
    hex_world_save_now: () => null,
    hex_world_load: () => "{}",
    hex_world_inspect: () => "{}",
  };
  const g = globalThis as { window?: unknown; location?: unknown; document?: unknown };
  const window: { wasmBindings: WasmBindings; hexWorld?: unknown } = {
    wasmBindings: w,
    hexWorld: undefined,
  };
  g.window = window;
  g.location = { search: "" };
  g.document = { getElementById: () => null };
  installShim(w);
  return {
    hw: window.hexWorld as {
      state(): {
        frames: number;
        queued: number;
        events_pending: number;
        play: { tiles: unknown[]; focus: number[] } | null;
      };
      dumpState(): unknown;
      pumpEvents(): void;
      events(): unknown[];
    },
    patches,
    setInbox: (n: number) => {
      inbox = n;
    },
    setEvents: (e: unknown[]) => {
      events = e;
    },
    restore: () => {
      delete g.window;
      delete g.location;
      delete g.document;
    },
  };
}

test("state merges only changed fields, preserves prior snapshots and never accumulates queue counts", () => {
  const { hw, patches, setInbox, setEvents, restore } = bridge();
  try {
    assert.deepEqual(JSON.parse(JSON.stringify(hw.state())), {});
    patches.push(
      JSON.stringify({
        state: { frames: 1, queued: 2, events_pending: 0, play: {} },
        play: { tiles: [{ q: 0 }], focus: [0, 0, 0] },
      }),
    );
    const first = hw.state();
    assert.ok(first.play);
    const tiles = first.play.tiles;
    setInbox(3);
    setEvents([{ type: "placed" }]);
    hw.pumpEvents();
    assert.equal(hw.state().queued, 5);
    assert.equal(hw.state().queued, 5);
    assert.equal(hw.state().events_pending, 1);
    assert.equal(hw.state().events_pending, 1);
    hw.events();
    assert.equal(hw.state().events_pending, 0);
    patches.push(JSON.stringify({ state: { frames: 2 }, play: { focus: [1, 0, 0] } }));
    const second = hw.state();
    assert.ok(second.play);
    assert.equal(second.play.tiles, tiles, "unchanged world arrays are not reparsed");
    assert.equal(JSON.stringify(first.play.focus), "[0,0,0]", "retained snapshots do not mutate");
    assert.equal(JSON.stringify(second.play.focus), "[1,0,0]");
    assert.equal(second.frames, 2);
    assert.deepEqual(hw.dumpState(), second);
    patches.push(JSON.stringify({ state: { play: null }, play: {} }));
    assert.equal(hw.state().play, null);
    patches.push(JSON.stringify({ state: { play: {} }, play: { tiles: [], focus: [2, 0, 0] } }));
    assert.equal(JSON.stringify(hw.state().play?.tiles), "[]");
  } finally {
    restore();
  }
});

test("play.falls arrives with the world fields and is kept until the world changes", () => {
  const { hw, patches, restore } = bridge();
  try {
    const fall = { q: 1, r: -1, edge: 0, drop: 2, x: 13.9, z: -24 };
    patches.push(
      JSON.stringify({ state: { frames: 1, play: {} }, play: { tiles: [], falls: [fall], focus: [0, 0, 0] } }),
    );
    const first = hw.state().play as unknown as { falls: (typeof fall)[] };
    assert.deepEqual(first.falls, [fall]);
    patches.push(JSON.stringify({ state: { frames: 2 }, play: { focus: [1, 0, 0] } }));
    const second = hw.state().play as unknown as { falls: (typeof fall)[] };
    assert.equal(second.falls, first.falls, "an unchanged world keeps its falls array");
    patches.push(JSON.stringify({ state: { frames: 3 }, play: { falls: [] } }));
    assert.deepEqual((hw.state().play as unknown as { falls: unknown[] }).falls, []);
  } finally {
    restore();
  }
});
