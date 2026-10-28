// Saved worlds: slot summaries, the autosave debounce, the storage-unavailable fallback and the
// autosave/resume/import flow, against an in-memory slot backend and a fake bridge.
import assert from "node:assert/strict";
import { mock, test } from "node:test";
import type { Command, GameEvent, InspectResult, LoadResult, PlayTile } from "../src/bridge.ts";
import { LOAD_HOLD_MS, type Note, Persistence, refusalText, stateWords, whenText } from "../src/persist.ts";
import {
  Debouncer,
  MAP_LIMIT,
  MemoryBackend,
  type SlotMeta,
  byRecent,
  cleanName,
  fileName,
  mapOf,
  mixOf,
  nameFromFile,
  nextName,
  openSlots,
} from "../src/slots.ts";

const tile = (q: number, r: number, id: string, rotation = 0): PlayTile => ({ q, r, tile: id, rotation });

test("slot summary: the socket mix counts every edge, the map keeps the tiles nearest the start", () => {
  const tiles = [tile(0, 0, "meadow"), tile(1, 0, "road_straight", 1), tile(-1, 0, "forest")];
  const mix = mixOf(tiles);
  assert.equal(
    Object.values(mix).reduce((a, b) => a + b, 0),
    18,
    "six edges per tile",
  );
  assert.equal(mix.Road, 2);
  assert.equal(mix.Forest, 6);
  assert.equal(mix.Grass, 10);
  const far = Array.from({ length: MAP_LIMIT + 50 }, (_, i) => tile(i, 0, "meadow"));
  const map = mapOf(far.reverse());
  assert.equal(map.length, MAP_LIMIT);
  assert.deepEqual(map[0], [0, 0, "meadow", 0]);
});

test("slot names: next free World n, clean names, file names and names from files", () => {
  assert.equal(nextName([]), "World 1");
  assert.equal(nextName(["World 1", "world 2", "Riverside"]), "World 3");
  assert.equal(cleanName("  River   side \n"), "River side");
  assert.equal(cleanName("x".repeat(80)).length, 40);
  assert.equal(fileName("Riverside"), "Riverside.hexworld");
  assert.equal(fileName('a/b:c*?"<>|'), "a-b-c------.hexworld");
  assert.equal(fileName("   "), "world.hexworld");
  assert.equal(nameFromFile("harbour_town.hexworld"), "harbour town");
  assert.equal(nameFromFile(".hexworld"), "Imported world");
  const metas = [3, 9, 1].map((t) => ({ id: `w${t}`, updatedAt: t }) as SlotMeta);
  assert.deepEqual(
    byRecent(metas).map((m) => m.id),
    ["w9", "w3", "w1"],
  );
});

test("bar words and save times", () => {
  assert.equal(stateWords("saved"), "Saved");
  assert.equal(stateWords("saving"), "Saving…");
  assert.equal(stateWords("off"), "Autosave off");
  const now = new Date(2026, 9, 14, 12, 0).getTime();
  assert.equal(whenText(now - 12_000, now), "12 s ago");
  assert.equal(whenText(now - 4 * 60_000, now), "4 min ago");
  assert.equal(whenText(new Date(2026, 9, 13, 21, 40).getTime(), now), "Yesterday, 21:40");
  assert.equal(whenText(new Date(2026, 9, 11, 18, 2).getTime(), now), "11 Oct, 18:02");
});

test("refusals read as plain words", () => {
  assert.equal(refusalText("bad_magic", "not a .hexworld file").text, "This isn't a Hex World file");
  assert.match(refusalText("newer_version", "format version 9").sub, /Reload the page/);
  assert.equal(refusalText("truncated", "").text, "This file is cut short");
  assert.equal(refusalText("invalid_world", "the world is not connected").sub, "the world is not connected");
});

test("the debounce coalesces a burst into one call and flushes on demand", () => {
  mock.timers.enable({ apis: ["setTimeout"] });
  try {
    let calls = 0;
    const d = new Debouncer(600, () => calls++);
    d.poke();
    mock.timers.tick(400);
    d.poke();
    mock.timers.tick(400);
    d.poke();
    assert.equal(calls, 0, "still inside the window");
    mock.timers.tick(600);
    assert.equal(calls, 1, "one call after the burst");
    assert.equal(d.flush(), false, "nothing pending");
    d.poke();
    assert.equal(d.flush(), true);
    assert.equal(calls, 2);
    mock.timers.tick(1000);
    assert.equal(calls, 2, "a flushed call does not fire again");
  } finally {
    mock.timers.reset();
  }
});

test("storage unavailable: no IndexedDB, an open that throws, errors, is blocked or never answers", async () => {
  assert.equal(await openSlots(undefined, 50), null);
  const throws = {
    open() {
      throw new DOMException("The operation is insecure.", "SecurityError");
    },
  } as unknown as IDBFactory;
  assert.equal(await openSlots(throws, 50), null);
  const request = (fire: "onerror" | "onblocked" | null) =>
    ({
      open() {
        const req: Record<string, unknown> = {};
        if (fire) setTimeout(() => (req[fire] as () => void)(), 5);
        return req;
      },
    }) as unknown as IDBFactory;
  assert.equal(await openSlots(request("onerror"), 200), null);
  assert.equal(await openSlots(request("onblocked"), 200), null);
  const t0 = Date.now();
  assert.equal(await openSlots(request(null), 60), null, "a stalled open times out");
  assert.ok(Date.now() - t0 >= 50);
});

/** The bridge as persistence sees it: commands recorded, saves answered with fixed bytes. */
class FakeHost {
  sent: Command[] = [];
  loads: Uint8Array[] = [];
  result: LoadResult = { ok: true, tiles: 3, version: 1 };
  private saves = new Map<string, Uint8Array>();
  cmd(c: Command): void {
    this.sent.push(c);
    if (c.cmd === "save" && c.tag) this.saves.set(c.tag, new Uint8Array([72, 69, 88, c.tag.length]));
  }
  takeSave(tag: string): Uint8Array | null {
    const b = this.saves.get(tag) ?? null;
    this.saves.delete(tag);
    return b;
  }
  load(bytes: Uint8Array): LoadResult {
    this.loads.push(bytes);
    return this.result;
  }
  inspected: InspectResult = { ok: true, version: 3, supported: 3, tiles: [] };
  inspect(_bytes: Uint8Array): InspectResult {
    return this.inspected;
  }
  nowCalls = 0;
  saveNow(): Uint8Array | null {
    this.nowCalls++;
    return new Uint8Array([72, 69, 88, 0]);
  }
  /** The `saved` events the engine would fire for every save sent so far. */
  savedEvents(): GameEvent[] {
    return this.sent.flatMap((c) =>
      c.cmd === "save" ? [{ type: "saved" as const, tag: c.tag, tiles: 3, bytes: 4 }] : [],
    );
  }
}

const settle = () => new Promise((r) => setTimeout(r, 0));

function setup(store: MemoryBackend | null) {
  const host = new FakeHost();
  const notes: Note[] = [];
  let now = 1_000;
  const p = new Persistence(host, store, {
    now: () => now,
    debounceMs: 30,
    notify: (n) => notes.push(n),
    volumes: () => ({ master: 0.5, music: 0.5, sfx: 0.5, ambience: 0.5, ui: 0.5 }),
  });
  return { host, notes, p, tick: (ms: number) => (now += ms) };
}

const placed: GameEvent = { type: "tilePlaced", q: 1, r: 0, tile: "meadow", rotation: 0 };

test("autosave: a burst of edits is one debounced save, written to the current world's slot", async () => {
  const store = new MemoryBackend();
  const { host, p } = setup(store);
  await p.start();
  assert.equal(p.current.name, "World 1");
  assert.equal(p.ready, true, "no slot: the start world, ready at once");
  p.observe([tile(0, 0, "meadow"), tile(1, 0, "meadow"), tile(2, 0, "forest")]);
  p.onEvents([placed, placed, { type: "undone" }, { type: "redone" }]);
  assert.equal(p.state, "saving");
  assert.equal(host.sent.length, 0, "debounced");
  await new Promise((r) => setTimeout(r, 60));
  const saves = host.sent.filter((c) => c.cmd === "save");
  assert.equal(saves.length, 1, "one save for the burst");
  assert.deepEqual((saves[0] as { volumes: unknown }).volumes, {
    master: 0.5,
    music: 0.5,
    sfx: 0.5,
    ambience: 0.5,
    ui: 0.5,
  });
  p.onEvents(host.savedEvents());
  await settle();
  assert.equal(p.state, "saved");
  const [meta] = await store.list();
  assert.equal(meta?.name, "World 1");
  assert.equal(meta?.tiles, 3);
  assert.equal(meta?.mix.Forest, 6);
  assert.equal(meta?.map.length, 3);
  assert.deepEqual([...(store.data.get(meta!.id) ?? [])].slice(0, 3), [72, 69, 88]);
  assert.equal(p.current.stored, true);
});

test("resume: the most recent slot loads on start, and autosave waits for it", async () => {
  const store = new MemoryBackend();
  const old = { id: "w-old", name: "Old", tiles: 2, updatedAt: 10, bytes: 1, mix: {}, map: [] };
  const recent = { ...old, id: "w-new", name: "Riverside", updatedAt: 20 };
  await store.write(old, new Uint8Array([1]));
  await store.write(recent, new Uint8Array([2]));
  const { host, p } = setup(store);
  await p.start();
  assert.deepEqual([...(host.loads[0] ?? [])], [2], "the newest slot's bytes");
  assert.equal(p.ready, false, "waits for the engine's loaded event");
  p.onEvents([placed]);
  await new Promise((r) => setTimeout(r, 60));
  assert.equal(host.sent.length, 0, "no autosave before the resume settles");
  p.onEvents([{ type: "loaded", tiles: 2, version: 1, settings: null }]);
  assert.equal(p.current.id, "w-new");
  assert.equal(p.current.name, "Riverside");
  assert.equal(p.ready, true);
});

test("switching worlds flushes the pending autosave ahead of the load, then saves to the right slot", async () => {
  const store = new MemoryBackend();
  await store.write(
    { id: "w-b", name: "Beta", tiles: 2, updatedAt: 5, bytes: 1, mix: {}, map: [] },
    new Uint8Array([9]),
  );
  const { host, p } = setup(store);
  host.result = { ok: false, code: "no_file", reason: "x", version: null };
  await p.start(); // the stored slot fails to open, so the start world stays
  host.result = { ok: true, tiles: 2, version: 1 };
  host.loads.length = 0;
  const first = p.current.id;
  p.observe([tile(0, 0, "meadow")]);
  p.onEvents([placed]);
  assert.equal(await p.loadSlot("w-b"), true);
  assert.equal(host.sent.at(-1)?.cmd, "save", "the flushed autosave is queued before the load");
  assert.equal(host.loads.length, 1);
  // The saved event arrives after the switch was requested: it still writes the old world's slot.
  p.onEvents(host.savedEvents());
  p.onEvents([{ type: "loaded", tiles: 2, version: 1, settings: null }]);
  await settle();
  assert.equal(p.current.id, "w-b");
  assert.ok(store.meta.has(first), "the autosave landed in the world it was taken from");
  assert.equal(p.slots[0]?.id, "w-b", "the opened world becomes the most recent");
});

test("import: a refused file is a toast saying why; a good one opens as a new, saved world", async () => {
  const store = new MemoryBackend();
  const { host, notes, p } = setup(store);
  await p.start();
  host.result = { ok: false, code: "bad_magic", reason: "not a .hexworld file", version: null };
  assert.equal(p.importFile("holiday.zip", new Uint8Array([80, 75])), false);
  assert.equal(notes.at(-1)?.text, "This isn't a Hex World file");
  assert.equal(notes.at(-1)?.tone, "bad");
  host.result = { ok: true, tiles: 3, version: 1 };
  assert.equal(p.importFile("harbour_town.hexworld", new Uint8Array([1])), true);
  p.onEvents([{ type: "loaded", tiles: 3, version: 1, settings: null }]);
  assert.equal(p.current.name, "harbour town");
  assert.equal(p.state, "saving", "an import is written once the HUD has seen its tiles");
  await new Promise((r) => setTimeout(r, 60));
  p.onEvents(host.savedEvents());
  await settle();
  assert.equal((await store.list()).find((m) => m.id === p.current.id)?.name, "harbour town");
});

test("new world, rename and delete", async () => {
  const store = new MemoryBackend();
  await store.write(
    { id: "w-x", name: "World 1", tiles: 4, updatedAt: 5, bytes: 1, mix: {}, map: [] },
    new Uint8Array([1]),
  );
  const { host, p } = setup(store);
  host.result = { ok: false, code: "no_file", reason: "x", version: null };
  await p.start();
  assert.equal(p.current.name, "World 2", "the start world takes the next free name");
  p.newWorld();
  assert.equal(host.sent.at(-1)?.cmd, "new_world");
  p.onEvents([{ type: "loaded", tiles: 1, version: 1, settings: null }]);
  assert.equal(p.current.name, "World 3");
  await p.rename(p.current.id, "  Riverside ");
  assert.equal(p.current.name, "Riverside");
  await p.rename("w-x", "Meadowlands");
  assert.equal(store.meta.get("w-x")?.name, "Meadowlands");
  assert.equal(await p.remove(p.current.id), false, "the current world is not deleted");
  assert.equal(await p.remove("w-x"), true);
  assert.equal(store.meta.has("w-x"), false);
  assert.equal(store.data.has("w-x"), false);
});

test("shown: only the player's New world is fresh (the title cue); resume, Load, import and Start over are not", async () => {
  const store = new MemoryBackend();
  await store.write(
    { id: "w-t", name: "Town", tiles: 9, updatedAt: 5, bytes: 1, mix: {}, map: [] },
    new Uint8Array([7]),
  );
  const host = new FakeHost();
  const shown: boolean[] = [];
  const p = new Persistence(host, store, { debounceMs: 30, notify: () => {}, shown: (fresh) => shown.push(fresh) });
  host.result = { ok: true, tiles: 9, version: 1 };
  await p.start();
  p.onEvents([{ type: "loaded", tiles: 9, version: 1, settings: null }]);
  assert.deepEqual(shown, [false], "the returning player's autosave resume never starts the title");
  p.newWorld();
  p.onEvents([{ type: "loaded", tiles: 1, version: 1, settings: null }]);
  assert.deepEqual(shown, [false, true], "New world is fresh once its world is on screen");
  assert.equal(await p.loadSlot("w-t"), true);
  p.onEvents([{ type: "loaded", tiles: 9, version: 1, settings: null }]);
  assert.deepEqual(shown, [false, true, false], "loading a saved world while the title plays ends it");
  assert.equal(p.importFile("harbour.hexworld", new Uint8Array([1])), true);
  p.onEvents([{ type: "loaded", tiles: 9, version: 1, settings: null }]);
  assert.equal(p.startOver(), true);
  p.onEvents([{ type: "loaded", tiles: 1, version: 1, settings: null }]);
  assert.deepEqual(shown, [false, true, false, false, false], "an import and Start over are not fresh");
  // A new_world no HUD action asked for (a script's own command) is not the player's New world either.
  p.onEvents([{ type: "loaded", tiles: 1, version: 1, settings: null }]);
  assert.equal(shown.at(-1), false);
});

test("shown at start-up: a first visit (nothing stored, or no storage) is fresh; any saved world is not (owner A43 O1)", async () => {
  const run = async (store: MemoryBackend | null, setupHost?: (h: FakeHost) => void, tiles = 1) => {
    const host = new FakeHost();
    setupHost?.(host);
    const shown: boolean[] = [];
    const p = new Persistence(host, store, { debounceMs: 30, notify: () => {}, shown: (fresh) => shown.push(fresh) });
    p.observe(Array.from({ length: tiles }, (_, i) => tile(i, 0, "meadow")));
    await p.start();
    return { p, shown };
  };
  const first = await run(new MemoryBackend());
  assert.deepEqual(first.shown, [true], "an empty worlds list: the start world is fresh");
  assert.equal(first.p.ready, true);
  const blocked = await run(null);
  assert.deepEqual(blocked.shown, [true], "IndexedDB blocked: nothing can be resumed, so it is a first visit");
  const placedFirst = await run(new MemoryBackend(), undefined, 2);
  assert.deepEqual(placedFirst.shown, [], "a tile placed before storage answered: the start world is no longer fresh");

  const saved = new MemoryBackend();
  await saved.write(
    { id: "w-a", name: "Autosaved", tiles: 3, updatedAt: 5, bytes: 1, mix: {}, map: [] },
    new Uint8Array([3]),
  );
  const back = await run(saved);
  assert.deepEqual(back.shown, [], "a returning player's start-up is never fresh");
  back.p.onEvents([{ type: "loaded", tiles: 3, version: 1, settings: null }]);
  assert.deepEqual(back.shown, [false], "the resumed autosave ends any title");
  // Saved worlds exist but the newest fails to open: the start world stays, and it is still a returning player.
  const refused = await run(saved, (h) => {
    h.result = { ok: false, code: "no_file", reason: "x", version: null };
  });
  assert.deepEqual(refused.shown, [], "a saved world that fails to open is not a first visit");
  assert.equal(refused.p.ready, true);
});

test("start over clears the world in place: same name and slot, the cleared world autosaved there", async () => {
  const store = new MemoryBackend();
  await store.write(
    { id: "w-r", name: "Riverside", tiles: 4, updatedAt: 5, bytes: 1, mix: {}, map: [] },
    new Uint8Array([1]),
  );
  await store.write(
    { id: "w-o", name: "Other", tiles: 2, updatedAt: 3, bytes: 1, mix: {}, map: [] },
    new Uint8Array([2]),
  );
  const { host, notes, p } = setup(store);
  assert.equal(p.startOver(), false, "nothing to clear before the resume settles");
  await p.start();
  assert.equal(p.startOver(), false, "still waiting for the resumed world's loaded event");
  p.onEvents([{ type: "loaded", tiles: 4, version: 1, settings: null }]);
  assert.equal(p.current.id, "w-r");
  p.observe([tile(0, 0, "meadow"), tile(1, 0, "meadow"), tile(2, 0, "forest"), tile(0, 1, "meadow")]);
  p.onEvents([placed]);
  host.sent.length = 0;
  assert.equal(p.startOver(), true);
  assert.deepEqual(
    host.sent.map((c) => c.cmd),
    ["save", "new_world"],
    "the pending autosave is flushed ahead of the reset; the engine's new_world does the reset",
  );
  assert.equal(notes.at(-1)?.text, "Started Riverside over");
  p.onEvents(host.savedEvents());
  p.onEvents([{ type: "loaded", tiles: 1, version: 1, settings: null }]);
  assert.deepEqual(p.current, { id: "w-r", name: "Riverside", stored: true }, "name and slot kept");
  assert.equal(p.state, "saving", "the cleared world is autosaved once the HUD has seen it");
  p.observe([tile(0, 0, "meadow")]);
  await new Promise((r) => setTimeout(r, 60));
  p.onEvents(host.savedEvents().slice(1));
  await settle();
  assert.equal(p.state, "saved");
  const slots = await store.list();
  assert.deepEqual(slots.map((m) => m.id).sort(), ["w-o", "w-r"], "no new slot");
  assert.equal(store.meta.get("w-r")?.name, "Riverside");
  assert.equal(store.meta.get("w-r")?.tiles, 1, "the slot now holds the start tile only");
  assert.equal(p.slots[0]?.id, "w-r", "it stays the world the next start resumes");
});

test("start over with storage off resets the world and writes nothing", async () => {
  const { host, p } = setup(null);
  await p.start();
  const name = p.current.name;
  assert.equal(p.startOver(), true);
  assert.equal(host.sent.at(-1)?.cmd, "new_world");
  p.onEvents([{ type: "loaded", tiles: 1, version: 1, settings: null }]);
  assert.equal(p.current.name, name);
  assert.equal(p.state, "off");
  await new Promise((r) => setTimeout(r, 60));
  assert.equal(host.sent.filter((c) => c.cmd === "save").length, 0);
});

test("duplicate copies a world to a new slot that sorts under its source", async () => {
  const store = new MemoryBackend();
  await store.write(
    { id: "w-a", name: "Alpha", tiles: 2, updatedAt: 50, bytes: 1, mix: {}, map: [] },
    new Uint8Array([7]),
  );
  const { host, p } = setup(store);
  host.result = { ok: false, code: "no_file", reason: "x", version: null };
  await p.start();
  await p.duplicate("w-a");
  const copy = (await store.list()).find((m) => m.name === "Alpha copy");
  assert.ok(copy, "a stored world is copied without loading it");
  assert.deepEqual([...(store.data.get(copy.id) ?? [])], [7]);
  assert.ok(copy.updatedAt < 50, "the copy never becomes the world the next start resumes");
  p.observe([tile(0, 0, "meadow")]);
  await p.duplicate(p.current.id);
  assert.equal(host.sent.at(-1)?.cmd, "save", "the current world is copied through a save");
  p.onEvents(host.savedEvents().slice(-1));
  await settle();
  assert.ok((await store.list()).some((m) => m.name === "World 1 copy"));
});

test("page hide writes a pending autosave at once, from the engine's last frame", async () => {
  const store = new MemoryBackend();
  const { host, p } = setup(store);
  await p.start();
  assert.equal(p.flushNow(), false, "nothing pending: nothing written");
  assert.equal(host.nowCalls, 0);
  p.observe([tile(0, 0, "meadow"), tile(1, 0, "meadow")]);
  p.onEvents([placed]);
  assert.equal(p.flushNow(), true, "the debounced autosave is written now");
  assert.equal(host.nowCalls, 1);
  await settle();
  const [meta] = await store.list();
  assert.equal(meta?.id, p.current.id);
  assert.equal(meta?.tiles, 2);
  assert.deepEqual([...(store.data.get(p.current.id) ?? [])], [72, 69, 88, 0]);
  assert.equal(p.state, "saved");
  await new Promise((r) => setTimeout(r, 60));
  assert.equal(host.sent.filter((c) => c.cmd === "save").length, 0, "the debounce was cancelled, not run twice");
  assert.equal(p.flushNow(), false, "a second hide has nothing left to write");
});

test("settle before a reload: a pending autosave is written and awaited, an unanswered save times out", async () => {
  const store = new MemoryBackend();
  const { host, p } = setup(store);
  await p.start();
  assert.equal(await p.settle(), true, "nothing pending settles at once");
  p.observe([tile(0, 0, "meadow"), tile(1, 0, "meadow")]);
  p.onEvents([placed]);
  assert.equal(await p.settle(), true, "the debounced autosave is written before settle resolves");
  assert.equal((await store.list())[0]?.tiles, 2);
  assert.equal(p.state, "saved");

  // The engine cannot encode at once: settle waits for the queued save's `saved` event.
  host.saveNow = () => null;
  p.onEvents([placed]);
  setTimeout(() => p.onEvents(host.savedEvents().slice(-1)), 120);
  assert.equal(await p.settle(), true, "the queued save landed while settle waited");
  assert.equal(p.state, "saved");
  p.onEvents([placed]);
  assert.equal(await p.settle(150), false, "a save that never answers runs the wait out");

  const off = setup(null).p;
  await off.start();
  assert.equal(await off.settle(), false, "storage off has nothing to settle into");
});

test("storage off: autosave never runs, the bar reads Autosave off, a caution toast offers Export", async () => {
  const { host, notes, p } = setup(null);
  await p.start();
  assert.equal(p.state, "off");
  assert.equal(stateWords(p.state), "Autosave off");
  assert.equal(notes[0]?.tone, "caution");
  assert.equal(notes[0]?.action, "Export");
  p.onEvents([placed]);
  await new Promise((r) => setTimeout(r, 60));
  assert.equal(host.sent.length, 0);
  notes[0]?.onAction?.();
  assert.equal(host.sent[0]?.cmd, "save", "Export still works");
});

test("a store that starts failing turns autosave off instead of breaking play", async () => {
  const store = new MemoryBackend();
  store.write = async () => {
    throw new DOMException("quota", "QuotaExceededError");
  };
  const { host, notes, p } = setup(store);
  await p.start();
  p.onEvents([placed]);
  await new Promise((r) => setTimeout(r, 60));
  p.onEvents(host.savedEvents());
  await settle();
  assert.equal(p.state, "off");
  assert.equal(notes.at(-1)?.text, "Autosave stopped");
});

test("settings: a quality or tilt-shift change autosaves like an edit, and waits for the resume", async () => {
  const store = new MemoryBackend();
  const { host, p } = setup(store);
  p.settingsChanged();
  assert.equal(p.state, "saved", "before start has settled nothing is queued");
  await p.start();
  p.observe([tile(0, 0, "meadow")]);
  p.settingsChanged();
  p.settingsChanged();
  assert.equal(p.state, "saving");
  await new Promise((r) => setTimeout(r, 60));
  assert.equal(host.sent.filter((c) => c.cmd === "save").length, 1, "one debounced save");
  p.onEvents(host.savedEvents());
  await settle();
  assert.equal(p.state, "saved");
  assert.equal((await store.list()).length, 1);
  assert.ok(p.flushNow() === false, "nothing left pending for a page hide");
});

test("load confirm (S10.3): unsaved changes are held while it asks, then saved first or dropped", async () => {
  const store = new MemoryBackend();
  await store.write(
    { id: "w-b", name: "Beta", tiles: 2, updatedAt: 5, bytes: 1, mix: {}, map: [] },
    new Uint8Array([9]),
  );
  const { host, p, tick } = setup(store);
  host.result = { ok: false, code: "no_file", reason: "x", version: null };
  await p.start();
  host.result = { ok: true, tiles: 2, version: 1 };
  assert.equal(p.unsaved, false, "nothing changed yet");
  p.observe([tile(0, 0, "meadow")]);
  p.onEvents([placed]);
  assert.equal(p.unsaved, true);
  assert.equal(p.unsavedSince, 1_000);
  tick(90_000);
  p.holdAutosave();
  await new Promise((r) => setTimeout(r, 60));
  assert.equal(host.sent.filter((c) => c.cmd === "save").length, 0, "the held autosave does not run");
  assert.equal(p.unsaved, true, "still unsaved while held");
  assert.equal(p.unsavedSince, 1_000, "since the first change");
  // Cancel: the autosave runs again.
  p.releaseAutosave();
  await new Promise((r) => setTimeout(r, 60));
  assert.equal(host.sent.filter((c) => c.cmd === "save").length, 1, "released, it saves");
  p.onEvents(host.savedEvents());
  await settle();
  assert.equal(p.unsaved, false);
  assert.equal(p.unsavedSince, null);
  // Load without saving: the held changes are dropped, nothing is saved, and the load goes ahead.
  host.sent.length = 0;
  host.loads.length = 0;
  p.onEvents([placed]);
  p.holdAutosave();
  assert.equal(await p.loadSlot("w-b", "discard"), true);
  assert.equal(host.sent.filter((c) => c.cmd === "save").length, 0, "discard sends no save");
  assert.equal(host.loads.length, 1);
  await new Promise((r) => setTimeout(r, 60));
  assert.equal(host.sent.filter((c) => c.cmd === "save").length, 0, "and none later");
  p.onEvents([{ type: "loaded", tiles: 2, version: 1, settings: null }]);
  await settle();
  assert.equal(p.current.id, "w-b");
  assert.equal(p.unsaved, false);
});

test("load confirm (S10.3): Save, then load writes the held changes ahead of the load", async () => {
  const store = new MemoryBackend();
  await store.write(
    { id: "w-b", name: "Beta", tiles: 2, updatedAt: 5, bytes: 1, mix: {}, map: [] },
    new Uint8Array([9]),
  );
  const { host, p } = setup(store);
  host.result = { ok: false, code: "no_file", reason: "x", version: null };
  await p.start();
  host.result = { ok: true, tiles: 2, version: 1 };
  const first = p.current.id;
  p.observe([tile(0, 0, "meadow")]);
  p.onEvents([placed]);
  p.holdAutosave();
  assert.equal(await p.loadSlot("w-b", "save"), true);
  assert.equal(host.sent.at(-1)?.cmd, "save", "the held autosave is queued before the load");
  p.onEvents(host.savedEvents());
  p.onEvents([{ type: "loaded", tiles: 2, version: 1, settings: null }]);
  await settle();
  assert.ok(store.meta.has(first), "the changes landed in the world they were made in");
  assert.equal(p.current.id, "w-b");
});

test("import replace (S10.6): the file opens over the world on screen, keeping its name and slot", async () => {
  const store = new MemoryBackend();
  const removed: string[] = [];
  const host = new FakeHost();
  const notes: Note[] = [];
  const p = new Persistence(host, store, {
    debounceMs: 30,
    notify: (n) => notes.push(n),
    removed: (id) => removed.push(id),
  });
  await p.start();
  p.observe([tile(0, 0, "meadow")]);
  p.onEvents([placed]);
  await new Promise((r) => setTimeout(r, 60));
  p.onEvents(host.savedEvents());
  await settle();
  const { id, name } = p.current;
  assert.equal(p.current.stored, true);
  host.result = { ok: true, tiles: 5, version: 2 };
  assert.equal(p.importFile("harbour.hexworld", new Uint8Array([1]), "replace"), true);
  assert.deepEqual(removed, [id], "the replaced world's camera pose is forgotten");
  assert.equal(notes.at(-1)?.text, `Replaced ${name}`);
  p.onEvents([{ type: "loaded", tiles: 5, version: 2, settings: null }]);
  assert.equal(p.current.id, id);
  assert.equal(p.current.name, name);
  assert.equal(p.state, "saving", "written to its slot once the HUD has seen the tiles");
  assert.equal(p.importName("harbour.hexworld"), "harbour", "the preview's name for a new world");
  assert.deepEqual(p.inspect(new Uint8Array([1])), host.inspected, "inspect passes through to the bridge");
});

/**
 * A Persistence on a stored slot "Beta" with one unsaved change, under mocked timers (the hold
 * limit is tested by ticking, not by sleeping, so host load cannot make it flaky).
 */
async function heldSetup(holdMs?: number) {
  const store = new MemoryBackend();
  await store.write(
    { id: "w-b", name: "Beta", tiles: 2, updatedAt: 5, bytes: 1, mix: {}, map: [] },
    new Uint8Array([9]),
  );
  const host = new FakeHost();
  host.result = { ok: false, code: "no_file", reason: "x", version: null };
  const p = new Persistence(host, store, holdMs === undefined ? { debounceMs: 30 } : { debounceMs: 30, holdMs });
  await p.start();
  host.result = { ok: true, tiles: 2, version: 1 };
  mock.timers.enable({ apis: ["setTimeout"] });
  p.observe([tile(0, 0, "meadow")]);
  p.onEvents([placed]);
  return { host, p, store, saves: () => host.sent.filter((c) => c.cmd === "save").length };
}

/** Lets the in-memory store's writes finish while setTimeout is mocked. */
const microtasks = async () => {
  for (let i = 0; i < 20; i++) await Promise.resolve();
};

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));

test("load confirm (S10.3): the hold runs out after LOAD_HOLD_MS and the held autosave is written", async () => {
  assert.ok(LOAD_HOLD_MS >= 20_000 && LOAD_HOLD_MS <= 30_000, `between 20 and 30 s (${LOAD_HOLD_MS})`);
  const { host, p, saves } = await heldSetup();
  try {
    p.holdAutosave();
    mock.timers.tick(LOAD_HOLD_MS - 1);
    assert.equal(saves(), 0, "still held inside the limit");
    assert.equal(p.unsaved, true);
    mock.timers.tick(1);
    assert.equal(saves(), 1, "the hold expired: the held autosave was sent at once");
    assert.equal(p.unsaved, false, "nothing is held any more");
    assert.equal(p.state, "saving", "until the save lands");
    p.onEvents(host.savedEvents());
    await microtasks();
    assert.equal(p.state, "saved");
    assert.equal(p.unsavedSince, null);
    p.releaseAutosave();
    mock.timers.tick(1000);
    assert.equal(saves(), 1, "a later Cancel has nothing left to release");
  } finally {
    mock.timers.reset();
  }
});

test("load confirm (S10.3): ending the hold early (cancel or discard) stops its timer", async () => {
  const a = await heldSetup(500);
  try {
    a.p.holdAutosave();
    a.p.releaseAutosave();
    mock.timers.tick(1000);
    assert.equal(a.saves(), 1, "released: one debounced save, and the expired timer adds none");
  } finally {
    mock.timers.reset();
  }
  const b = await heldSetup(500);
  try {
    b.p.holdAutosave();
    assert.equal(await b.p.loadSlot("w-b", "discard"), true);
    mock.timers.tick(1000);
    assert.equal(b.saves(), 0, "Load without saving: no stale timer saves the newly loaded world into the old slot");
  } finally {
    mock.timers.reset();
  }
});

/** A stored world with tiles, saved once, ready to be replaced by an import. */
async function replaceSetup() {
  const store = new MemoryBackend();
  const host = new FakeHost();
  const notes: Note[] = [];
  const p = new Persistence(host, store, { debounceMs: 30, notify: (n) => notes.push(n) });
  await p.start();
  p.observe([tile(0, 0, "meadow"), tile(1, 0, "forest")]);
  p.onEvents([placed]);
  await wait(60);
  p.onEvents(host.savedEvents());
  await settle();
  const { id, name } = p.current;
  const before = store.data.get(id)?.slice();
  assert.ok(before, "the world is in its slot");
  host.sent.length = 0;
  host.loads.length = 0;
  return { store, host, notes, p, id, name, before };
}

const five = [0, 2, 3, 4, 5].map((q) => tile(q, 0, "meadow"));

test("import replace (S10.6): the toast's Undo puts the replaced world back byte for byte", async () => {
  const { store, host, notes, p, id, name, before } = await replaceSetup();
  // The import's autosave encodes to different bytes than the world it replaces.
  const take = host.takeSave.bind(host);
  host.takeSave = (tag) => {
    const b = take(tag);
    return b && new Uint8Array([...b, 7]);
  };
  host.result = { ok: true, tiles: 5, version: 2 };
  assert.equal(p.importFile("harbour.hexworld", new Uint8Array([1, 2, 3]), "replace"), true);
  const toast = notes.at(-1);
  assert.equal(toast?.text, `Replaced ${name}`);
  assert.equal(toast?.action, "Undo", "the Replace toast offers Undo");
  // The import comes up and its autosave writes it over the slot.
  p.onEvents([{ type: "loaded", tiles: 5, version: 2, settings: null }]);
  p.observe(five);
  await wait(60);
  p.onEvents(host.savedEvents());
  await settle();
  assert.notDeepEqual(store.data.get(id), before, "the import is in the slot");
  assert.equal(store.meta.get(id)?.tiles, 5);
  const sent = host.sent.filter((c) => c.cmd === "save").length;
  toast?.onAction?.();
  await settle();
  assert.deepEqual(store.data.get(id), before, "the slot holds the replaced world's exact bytes");
  assert.deepEqual(host.loads.at(-1), before, "the engine loads those bytes");
  p.onEvents([{ type: "loaded", tiles: 2, version: 1, settings: null }]);
  p.observe([tile(0, 0, "meadow"), tile(1, 0, "forest")]);
  await wait(60);
  await settle();
  assert.equal(host.sent.filter((c) => c.cmd === "save").length, sent, "no autosave encodes it again");
  assert.deepEqual(store.data.get(id), before, "still byte-identical after the load");
  assert.equal(p.current.id, id, "same id");
  assert.equal(p.current.name, name, "same name");
  assert.equal(store.meta.get(id)?.tiles, 2, "its summary is the replaced world's again");
  assert.equal(store.meta.get(id)?.name, name);
  assert.equal(p.state, "saved");
  assert.equal(notes.at(-1)?.text, `Restored ${name}`);
  assert.equal(await p.undoReplace(), false, "one Undo per Replace");
});

test("import replace (S10.6): Undo before the import is up drops its pending autosave", async () => {
  const { store, host, p, id, before } = await replaceSetup();
  host.result = { ok: true, tiles: 5, version: 2 };
  p.importFile("harbour.hexworld", new Uint8Array([1]), "replace");
  assert.equal(await p.undoReplace(), true);
  // Both loads come up in order: the import's, then the restored world's.
  p.onEvents([{ type: "loaded", tiles: 5, version: 2, settings: null }]);
  p.onEvents([{ type: "loaded", tiles: 2, version: 1, settings: null }]);
  await wait(60);
  await settle();
  assert.equal(host.sent.filter((c) => c.cmd === "save").length, 0, "the import's write never runs");
  assert.deepEqual(store.data.get(id), before);
  assert.equal(p.current.id, id);
});

test("import replace (S10.6): unsaved changes are written first and are what Undo restores", async () => {
  const { store, host, notes, p, id } = await replaceSetup();
  p.observe([tile(0, 0, "meadow"), tile(1, 0, "forest"), tile(2, 0, "meadow")]);
  p.onEvents([placed]);
  assert.equal(p.unsaved, true);
  host.result = { ok: true, tiles: 5, version: 2 };
  p.importFile("harbour.hexworld", new Uint8Array([1]), "replace");
  assert.equal(host.nowCalls, 1, "the unsaved world is encoded at once, ahead of the load");
  await settle();
  const written = store.data.get(id)?.slice();
  assert.deepEqual(written, new Uint8Array([72, 69, 88, 0]));
  p.onEvents([{ type: "loaded", tiles: 5, version: 2, settings: null }]);
  p.observe(five);
  await wait(60);
  p.onEvents(host.savedEvents());
  await settle();
  notes.at(-1)?.onAction?.();
  await settle();
  assert.deepEqual(store.data.get(id), written, "the slot holds the world as it was on screen");
  assert.equal(store.meta.get(id)?.tiles, 3);
});

test("import replace (S10.6): Undo is gone once another world is on screen", async () => {
  const { host, notes, p } = await replaceSetup();
  host.result = { ok: true, tiles: 5, version: 2 };
  p.importFile("harbour.hexworld", new Uint8Array([1]), "replace");
  p.onEvents([{ type: "loaded", tiles: 5, version: 2, settings: null }]);
  const undo = notes.at(-1)?.onAction;
  p.newWorld();
  p.onEvents([{ type: "loaded", tiles: 1, version: 1, settings: null }]);
  const loads = host.loads.length;
  undo?.();
  await settle();
  assert.equal(host.loads.length, loads, "nothing loads");
  assert.equal(await p.undoReplace(), false);
});

test("import replace (S10.6): Undo is byte-exact after a Duplicate wrote another slot", async () => {
  const { store, host, notes, p, id, before } = await replaceSetup();
  await p.duplicate(id);
  p.onEvents(host.savedEvents());
  await settle();
  assert.equal(store.meta.size, 2, "the copy has its own slot");
  host.result = { ok: true, tiles: 5, version: 2 };
  p.importFile("harbour.hexworld", new Uint8Array([1]), "replace");
  assert.equal(host.nowCalls, 0, "the stored bytes are kept, not encoded again");
  p.onEvents([{ type: "loaded", tiles: 5, version: 2, settings: null }]);
  notes.at(-1)?.onAction?.();
  await settle();
  assert.deepEqual(store.data.get(id), before);
});

test("import replace (S10.6): Undo is byte-exact on a world opened by Save, then load", async () => {
  const { store, host, notes, p } = await replaceSetup();
  const beta = new Uint8Array([66, 69, 84, 65]);
  await store.write({ id: "w-b", name: "Beta", tiles: 2, updatedAt: 5, bytes: 4, mix: {}, map: [] }, beta);
  await p.refresh();
  p.onEvents([placed]);
  p.holdAutosave();
  host.result = { ok: true, tiles: 2, version: 1 };
  assert.equal(await p.loadSlot("w-b", "save"), true);
  // The flushed autosave of the world left behind lands after Beta's bytes were read.
  p.onEvents(host.savedEvents());
  p.onEvents([{ type: "loaded", tiles: 2, version: 1, settings: null }]);
  await settle();
  assert.equal(p.current.id, "w-b");
  host.result = { ok: true, tiles: 5, version: 2 };
  p.importFile("harbour.hexworld", new Uint8Array([1]), "replace");
  assert.equal(host.nowCalls, 0, "Beta's bytes as read are kept");
  p.onEvents([{ type: "loaded", tiles: 5, version: 2, settings: null }]);
  notes.at(-1)?.onAction?.();
  await settle();
  assert.deepEqual(store.data.get("w-b"), beta);
});
