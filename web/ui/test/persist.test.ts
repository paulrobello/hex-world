// Saved worlds: slot summaries, the autosave debounce, the storage-unavailable fallback and the
// autosave/resume/import flow, against an in-memory slot backend and a fake bridge.
import assert from "node:assert/strict";
import { mock, test } from "node:test";
import type { Command, GameEvent, LoadResult, PlayTile } from "../src/bridge.ts";
import { type Note, Persistence, refusalText, stateWords, whenText } from "../src/persist.ts";
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
