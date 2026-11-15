// The shim's pure parts (ARC-003): the state patch merge, the 256-event queue and the capture
// pollers, which ran as an untested inline script before. The full page loads the built shim.js
// and drives it in a browser (e2e/ui.mjs, web-smoke), not here.
import assert from "node:assert/strict";
import { test } from "node:test";
import type { GameEvent } from "../src/bridge.ts";
import { EventQueue, exportPhoto, mergePatch, screenshot } from "../src/shim.ts";

const exported = (name: string, width: number): GameEvent =>
  ({ type: "photoExported", name, width, height: 1800 }) as GameEvent;

test("mergePatch replaces top-level keys and merges play patches onto cached play", () => {
  const state: Record<string, unknown> = { tier: { source: "detected" }, play: { tiles: 3, cars: [1] } };
  mergePatch(state, { state: { tier: { source: "runtime" } }, play: { cars: [2, 3] } });
  assert.deepEqual(state.tier, { source: "runtime" });
  // A play key the patch omits keeps its cached value.
  const play = state.play as { tiles: number; cars: number[] };
  assert.equal(play.tiles, 3);
  assert.deepEqual(play.cars, [2, 3]);
});

test("an empty play patch leaves the cached play untouched", () => {
  const state: Record<string, unknown> = { play: { tiles: 3 } };
  mergePatch(state, { state: {}, play: {} });
  assert.deepEqual(state.play, { tiles: 3 });
});

test("the event queue keeps the last 256 events and drops the oldest", () => {
  const q = new EventQueue();
  const batch = Array.from({ length: 300 }, (_, i) => ({ type: "tileSelected", tile: `t${i}` })) as GameEvent[];
  q.push(batch);
  const out = q.drain();
  assert.equal(out.length, 256);
  assert.equal((out[0] as Extract<GameEvent, { type: "tileSelected" }>).tile, "t44");
  assert.equal((out[255] as Extract<GameEvent, { type: "tileSelected" }>).tile, "t299");
  assert.equal(q.length, 0);
});

test("watchers see every batch without taking it from events() callers", () => {
  const q = new EventQueue();
  const seen: GameEvent[] = [];
  q.watch((batch) => seen.push(...batch));
  const first = [{ type: "undone" }] as GameEvent[];
  const second = [{ type: "redone" }] as GameEvent[];
  q.push(first);
  q.push(second);
  assert.deepEqual(seen, [...first, ...second]);
  // The watcher saw both batches; the queue still hands them to a drain.
  assert.equal(q.drain().length, 2);
});

test("exportPhoto resolves with the width from the photo slot while another caller drains events()", async () => {
  const q = new EventQueue();
  let ticks = 0;
  const result = await exportPhoto(
    {
      cmd: () => {},
      pump: () => {
        ticks++;
        if (ticks === 2) {
          // The engine's export lands; a concurrent HUD events() call drains the shared queue
          // before the poller reads it. The photo slot still holds the event.
          q.push([exported("p", 2880)]);
          q.drain();
        }
      },
      takeShot: () => (ticks >= 2 ? "data:image/png;base64,UE5H" : null),
      events: q,
      now: () => ticks * 1000,
      schedule: (tick) => tick(),
    },
    2,
    "p",
    60000,
  );
  assert.equal(result.name, "p");
  assert.equal(result.width, 2880);
  assert.equal(result.height, 1800);
  assert.match(result.url, /^data:image\/png/);
});

test("exportPhoto rejects with the engine's reason on photoExportFailed", async () => {
  const q = new EventQueue();
  await assert.rejects(
    exportPhoto(
      {
        cmd: () => {},
        pump: () => {
          q.push([{ type: "photoExportFailed", name: "p", reason: "too large" }] as GameEvent[]);
        },
        takeShot: () => null,
        events: q,
        now: () => 0,
        schedule: () => {},
      },
      undefined,
      "p",
    ),
    /too large/,
  );
});

test("exportPhoto defaults the name from the clock and sends scale 1", async () => {
  const q = new EventQueue();
  const sent: Record<string, unknown>[] = [];
  let ticks = 0;
  const result = await exportPhoto(
    {
      cmd: (c) => sent.push(c),
      pump: () => {
        ticks++;
      },
      takeShot: () => (ticks >= 1 ? "data:image/png;base64,UE5H" : null),
      events: q,
      now: () => 1750000000000,
      schedule: (tick) => tick(),
    },
    undefined,
    undefined,
  );
  assert.equal(sent.length, 1);
  assert.equal(sent[0]?.["scale"], 1);
  assert.equal(result.name, "hex-world-1750000000000");
  assert.equal(result.width, null);
});

test("screenshot resolves the PNG data URL once the capture lands", async () => {
  let taken: string | null = null;
  const url = await screenshot(
    {
      screenshot: (name) => {
        taken = name;
      },
      takeShot: () => (taken ? "data:image/png;base64,UE5H" : null),
      now: () => 0,
      schedule: () => {
        throw new Error("must not need a second tick");
      },
    },
    "shot-a",
  );
  assert.match(url, /^data:image\/png/);
});

test("screenshot times out past the deadline", async () => {
  let clock = 20000;
  await assert.rejects(
    screenshot(
      {
        screenshot: () => {},
        takeShot: () => null,
        now: () => clock,
        schedule: (tick, ms) => {
          clock += ms;
          tick();
        },
      },
      "shot-b",
      10000,
    ),
    /timed out/,
  );
});
