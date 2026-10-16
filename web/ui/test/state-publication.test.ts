import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { runInNewContext } from "node:vm";

function bridge() {
  const html = readFileSync(new URL("../../../index.html", import.meta.url), "utf8");
  const script = html.match(/<script>\s*(addEventListener\("TrunkApplicationStarted"[\s\S]*?)<\/script>/)?.[1];
  assert.ok(script);
  const patches: string[] = [];
  let inbox = 0;
  let events: unknown[] = [];
  const window = {
    wasmBindings: {
      hex_world_state_json: () => patches.shift() ?? "",
      hex_world_inbox_len: () => inbox,
      hex_world_events: () => JSON.stringify(events.splice(0)),
    },
    hexWorld: undefined as unknown as {
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
  };
  runInNewContext(script, {
    window,
    addEventListener: (_name: string, callback: () => void) => callback(),
    URLSearchParams,
    location: { search: "" },
  });
  return {
    hw: window.hexWorld,
    patches,
    setInbox: (n: number) => {
      inbox = n;
    },
    setEvents: (e: unknown[]) => {
      events = e;
    },
  };
}

test("state merges only changed fields, preserves prior snapshots and never accumulates queue counts", () => {
  const { hw, patches, setInbox, setEvents } = bridge();
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
});
