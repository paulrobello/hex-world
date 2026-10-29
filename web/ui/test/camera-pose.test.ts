// The per-viewer camera pose: reading it from the state, "moved" with wrapped angles, the stored
// map (malformed, capped, pruned), the camera command that puts the orbit back, and the keeper's
// record and restore flow against a fake bridge and storage, including a store that throws.
import assert from "node:assert/strict";
import { test } from "node:test";
import type { Command, OrbitPose, PlayState, State, WalkState } from "../src/bridge.ts";
import {
  CameraKeeper,
  type CameraPose,
  POSE_CAP,
  POSE_KEY,
  type PoseMap,
  type PoseStorage,
  angleDiff,
  groundPending,
  onlySlots,
  orbitCommand,
  parsePoses,
  poseOf,
  samePose,
  walkAllowed,
  withPose,
} from "../src/camera-pose.ts";

const orbit = (o: Partial<OrbitPose> = {}): OrbitPose => ({
  focus: [10, 0, -4],
  yaw: 0.7,
  pitch: 0.9,
  distance: 140,
  ...o,
});

const play = (p: Partial<PlayState> = {}): PlayState =>
  ({
    walk_mode: false,
    photo_mode: false,
    tiles: [
      { q: 0, r: 0, tile: "meadow", rotation: 0 },
      { q: 1, r: 0, tile: "meadow", rotation: 0 },
    ],
    orbit: orbit(),
    ...p,
  }) as PlayState;

const walking = (w: Partial<WalkState> = {}): WalkState =>
  ({
    on: true,
    feet: [27.7, 0.1, 0.5],
    heading: 120,
    pitch: -6,
    cell: { q: 1, r: 0 },
    blocked: null,
    ...w,
  }) as WalkState;

const pose = (p: Partial<CameraPose> = {}): CameraPose => ({ orbit: orbit(), walk: null, at: 1, ...p });

class MemStore implements PoseStorage {
  data = new Map<string, string>();
  getItem(k: string): string | null {
    return this.data.get(k) ?? null;
  }
  setItem(k: string, v: string): void {
    this.data.set(k, v);
  }
}

test("poseOf: the orbit always, the walker only while walk mode is on", () => {
  assert.equal(poseOf(null, null, 5), null);
  assert.equal(poseOf(play({ orbit: null }), null, 5), null);
  const p = poseOf(play(), walking(), 5);
  assert.deepEqual(p, { orbit: orbit(), walk: null, at: 5 }, "walk state is ignored with walk mode off");
  const w = poseOf(play({ walk_mode: true }), walking(), 6);
  assert.deepEqual(w?.walk, { feet: [27.7, 0.1, 0.5], heading: 120, pitch: -6, cell: { q: 1, r: 0 } });
  assert.equal(poseOf(play({ walk_mode: true }), walking({ feet: null }), 6)?.walk, null);
});

test("angles compare around the circle, so a wrap is not a move", () => {
  assert.ok(Math.abs(angleDiff(Math.PI - 1e-6, -Math.PI + 1e-6, 2 * Math.PI)) < 1e-5);
  assert.equal(angleDiff(359.995, 0.004, 360).toFixed(3), "0.009");
  assert.equal(angleDiff(10, 350, 360), 20);
  assert.ok(
    samePose(pose({ orbit: orbit({ yaw: Math.PI - 1e-6 }) }), pose({ orbit: orbit({ yaw: -Math.PI + 1e-6 }) })),
  );
  assert.ok(!samePose(pose(), pose({ orbit: orbit({ distance: 141 }) })));
  assert.ok(!samePose(pose(), pose({ orbit: orbit({ focus: [10, 0, -3.9] }) })));
  const wp = { feet: [1, 0, 2] as [number, number, number], heading: 359.999, pitch: 3, cell: { q: 0, r: 0 } };
  assert.ok(samePose(pose({ walk: wp }), pose({ walk: { ...wp, heading: 0.001 } })));
  assert.ok(!samePose(pose({ walk: wp }), pose()), "walking and orbiting differ");
});

test("the stored map: malformed entries dropped, capped to the newest, pruned to stored slots", () => {
  assert.deepEqual(parsePoses(null), {});
  assert.deepEqual(parsePoses("{not json"), {});
  assert.deepEqual(parsePoses("[1,2]"), {});
  const good = pose({ at: 7 });
  const map = parsePoses(
    JSON.stringify({ a: good, b: { orbit: { focus: [0, 0] }, walk: null, at: 1 }, c: { ...good, walk: { feet: 3 } } }),
  );
  assert.deepEqual(Object.keys(map), ["a"]);
  let m: PoseMap = {};
  for (let i = 0; i < POSE_CAP + 5; i++) m = withPose(m, `w${i}`, pose({ at: i }));
  assert.equal(Object.keys(m).length, POSE_CAP);
  assert.ok(!("w0" in m) && `w${POSE_CAP + 4}` in m, "the oldest go first");
  assert.deepEqual(Object.keys(onlySlots(m, ["w10", "gone"])), ["w10"]);
});

test("orbitCommand puts the camera on the orbit, looking at its focus", () => {
  const o = orbit({ focus: [-40, 0, 25], yaw: -2.1, pitch: 0.6, distance: 333 });
  const c = orbitCommand(o) as Extract<Command, { cmd: "camera" }>;
  assert.equal(c.cmd, "camera");
  assert.deepEqual(c.look_at, [-40, 0, 25]);
  const off = c.pos.map((v, i) => v - (o.focus[i] ?? 0));
  assert.ok(Math.abs(Math.hypot(...off) - 333) < 1e-9);
  assert.ok(Math.abs(Math.atan2(off[0] ?? 0, off[2] ?? 0) + 2.1) < 1e-9, "yaw from +Z toward +X");
  assert.ok(Math.abs(Math.asin((off[1] ?? 0) / 333) - 0.6) < 1e-9);
});

test("a walk pose restores only onto a placed tile whose ground is in", () => {
  const w = { feet: [27.7, 0, 0.5] as [number, number, number], heading: 0, pitch: 0, cell: { q: 1, r: 0 } };
  assert.ok(walkAllowed(w, play().tiles));
  assert.ok(!walkAllowed({ ...w, cell: { q: 2, r: 0 } }, play().tiles), "bulldozed or off the map");
  const s = (x: Partial<State>): State => ({ play: play(), ...x });
  assert.ok(!groundPending(s({ pack_waiting: [], packs: { core: "ready", water: "idle" } }), w));
  assert.ok(groundPending(s({ pack_waiting: [{ q: 1, r: 0, tile: "meadow", packs: ["core"] }] }), w));
  assert.ok(groundPending(s({ packs: { core: "loading" } }), w));
  assert.ok(
    !groundPending(s({ packs: { core: "ready", village: "loading" } }), w),
    "other packs streaming do not hold it",
  );
});

/** A keeper on a fake clock, bridge and store. */
function rig(storage: () => PoseStorage | null = () => store) {
  let now = 1000;
  const sent: Command[] = [];
  const keeper = new CameraKeeper({ cmd: (c) => sent.push(c) }, { storage, now: () => now, intervalMs: 2000 });
  return { keeper, sent, advance: (ms: number) => (now += ms) };
}
let store = new MemStore();

test("records a moved camera every interval and on flush, only for a stored world", () => {
  store = new MemStore();
  const { keeper, advance } = rig();
  const st = (o: OrbitPose, p: Partial<PlayState> = {}, walk?: WalkState): State => ({
    play: play({ orbit: o, ...p }),
    ...(walk ? { walk } : {}),
  });
  keeper.tick(null, st(orbit()));
  assert.equal(store.getItem(POSE_KEY), null, "no slot, nothing written");
  keeper.tick("w1", st(orbit()));
  assert.ok(keeper.read("w1"), "the first check writes");
  const first = store.getItem(POSE_KEY);
  advance(500);
  keeper.tick("w1", st(orbit({ yaw: 1.2 })));
  assert.equal(store.getItem(POSE_KEY), first, "inside the interval nothing is written");
  advance(2000);
  keeper.tick("w1", st(orbit({ yaw: 1.2 })));
  assert.equal(keeper.read("w1")?.orbit.yaw, 1.2);
  const second = store.getItem(POSE_KEY);
  advance(2000);
  keeper.tick("w1", st(orbit({ yaw: 1.2 })));
  assert.equal(store.getItem(POSE_KEY), second, "an unmoved camera writes nothing");
  advance(100);
  keeper.tick("w1", st(orbit({ yaw: 1.2 }), { walk_mode: true }, walking()));
  keeper.flush();
  assert.equal(keeper.read("w1")?.walk?.heading, 120, "flush writes at once");
  keeper.forget("w1");
  assert.equal(keeper.read("w1"), null);
});

test("restores the orbit after the load settles, without recording the save's camera meanwhile", () => {
  store = new MemStore();
  store.setItem(POSE_KEY, JSON.stringify({ w1: pose({ orbit: orbit({ yaw: -1.5 }) }) }));
  const { keeper, sent, advance } = rig();
  keeper.begin("w1");
  const loaded = { play: play({ orbit: orbit({ yaw: 0.2 }), walk_mode: true }) };
  keeper.tick("w1", loaded);
  keeper.tick("w1", { play: play({ orbit: orbit({ yaw: 0.2 }) }) });
  assert.equal(sent.length, 0, "waits while the load leaves walk mode and settles");
  keeper.tick("w1", { play: play({ orbit: orbit({ yaw: 0.2 }) }) });
  keeper.tick("w1", { play: play({ orbit: orbit({ yaw: 0.2 }) }) });
  keeper.tick("w1", { play: play({ orbit: orbit({ yaw: 0.2 }) }) });
  assert.equal(sent.length, 1);
  assert.equal(sent[0]?.cmd, "camera");
  advance(5000);
  keeper.flush();
  assert.equal(keeper.read("w1")?.orbit.yaw, -1.5, "a flush mid-restore leaves the stored pose alone");
  keeper.tick("w1", { play: play({ orbit: orbit({ yaw: -1.5 }) }) });
  assert.equal(keeper.lastOutcome, "orbit");
  assert.ok(!keeper.restoring);
});

test("restores the walker; a bulldozed tile or a refused walk_to falls back to the orbit", () => {
  const wp = { feet: [27.7, 0.1, 0.5] as [number, number, number], heading: 120, pitch: -6, cell: { q: 1, r: 0 } };
  const run = (tiles: PlayState["tiles"], blocked: WalkState["blocked"]) => {
    store = new MemStore();
    store.setItem(POSE_KEY, JSON.stringify({ w1: pose({ walk: wp }) }));
    const { keeper, sent, advance } = rig();
    keeper.begin("w1");
    for (let i = 0; i < 4; i++) keeper.tick("w1", { play: play({ tiles }) });
    keeper.tick("w1", { play: play({ tiles }), packs: { core: "loading" } });
    keeper.tick("w1", { play: play({ tiles }), packs: { core: "ready" } });
    advance(500);
    const on = { play: play({ tiles, walk_mode: true }), walk: walking({ blocked }) };
    keeper.tick("w1", on);
    advance(500);
    keeper.tick("w1", on);
    return { keeper, sent: sent.map((c) => c.cmd) };
  };
  const ok = run(play().tiles, null);
  assert.deepEqual(ok.sent, ["camera", "walk", "walk_to"]);
  assert.equal(ok.keeper.lastOutcome, "walk");
  const gone = run([{ q: 0, r: 0, tile: "meadow", rotation: 0, level: 0 }], null);
  assert.deepEqual(gone.sent, ["camera"], "a bulldozed tile never enters walk mode");
  assert.equal(gone.keeper.lastOutcome, "fallback");
  const refused = run(play().tiles, "water");
  assert.deepEqual(refused.sent, ["camera", "walk", "walk_to", "walk_to", "walk"], "one retry, then leave walk mode");
  assert.equal(refused.keeper.lastOutcome, "fallback");
});

test("a restore yields to another camera move, before it sends and before its own reads back", () => {
  const saved = orbit({ yaw: -1.5 });
  const loadedCam = orbit({ yaw: 0.2 });
  const moved = orbit({ focus: [18, 0, 26], yaw: 0.6 });
  const start = () => {
    store = new MemStore();
    store.setItem(POSE_KEY, JSON.stringify({ w1: pose({ orbit: saved }) }));
    const r = rig();
    r.keeper.begin("w1");
    assert.deepEqual(r.keeper.status(), { restoring: true, phase: "settle", outcome: null });
    return r;
  };
  const at = (o: OrbitPose) => ({ play: play({ orbit: o }) });

  // Before it sends: the save's camera is seen first, then a move during the settle frames.
  {
    const { keeper, sent, advance } = start();
    keeper.tick("w1", at(loadedCam));
    keeper.tick("w1", at(moved));
    for (let i = 0; i < 4; i++) keeper.tick("w1", at(moved));
    assert.deepEqual(sent, [], "nothing is sent over the move");
    assert.deepEqual(keeper.status(), { restoring: false, phase: null, outcome: "yielded" });
    advance(10);
    keeper.tick("w1", at(moved));
    assert.ok(
      samePose(keeper.read("w1") ?? pose(), pose({ orbit: moved })),
      "the moved camera becomes the stored pose",
    );
  }

  // After it sends, before its own camera reads back: a third camera wins, and is never resent over.
  {
    const { keeper, sent, advance } = start();
    for (let i = 0; i < 4; i++) keeper.tick("w1", at(loadedCam));
    assert.deepEqual(
      sent.map((c) => c.cmd),
      ["camera"],
    );
    assert.equal(keeper.status().phase, "orbit");
    keeper.tick("w1", at(moved));
    advance(1500);
    keeper.tick("w1", at(moved));
    assert.equal(sent.length, 1, "no resend after a yield");
    assert.equal(keeper.lastOutcome, "yielded");
  }

  // Still on the camera it was sent from after 1 s: resent once, then restored.
  {
    const { keeper, sent, advance } = start();
    for (let i = 0; i < 4; i++) keeper.tick("w1", at(loadedCam));
    advance(1100);
    keeper.tick("w1", at(loadedCam));
    advance(1100);
    keeper.tick("w1", at(loadedCam));
    assert.equal(sent.length, 2, "one resend while the camera has not moved");
    keeper.tick("w1", at(saved));
    assert.equal(keeper.lastOutcome, "orbit");
  }
});

test("a walker waiting for its ground yields when the camera leaves the restored orbit", () => {
  const wp = { feet: [27.7, 0.1, 0.5] as [number, number, number], heading: 120, pitch: -6, cell: { q: 1, r: 0 } };
  store = new MemStore();
  store.setItem(POSE_KEY, JSON.stringify({ w1: pose({ walk: wp }) }));
  const { keeper, sent } = rig();
  keeper.begin("w1");
  for (let i = 0; i < 5; i++) keeper.tick("w1", { play: play(), packs: { core: "loading" } });
  assert.equal(keeper.status().phase, "ground");
  keeper.tick("w1", { play: play({ orbit: orbit({ distance: 300 }) }), packs: { core: "ready" } });
  assert.deepEqual(
    sent.map((c) => c.cmd),
    ["camera"],
    "walk mode is never entered",
  );
  assert.equal(keeper.lastOutcome, "yielded");
});

test("a store that throws or is missing leaves the keeper working with nothing kept", () => {
  const throwing: PoseStorage = {
    getItem() {
      throw new DOMException("denied", "SecurityError");
    },
    setItem() {
      throw new DOMException("full", "QuotaExceededError");
    },
  };
  for (const storage of [
    () => throwing,
    () => null,
    () => {
      throw new DOMException("blocked", "SecurityError");
    },
  ]) {
    const { keeper, sent } = rig(storage);
    keeper.tick("w1", { play: play() });
    keeper.flush();
    keeper.forget("w1");
    keeper.keepOnly([]);
    keeper.begin("w1");
    assert.ok(!keeper.restoring, "nothing to restore");
    keeper.tick("w1", { play: play() });
    assert.equal(keeper.read("w1"), null);
    assert.equal(sent.length, 0);
  }
});
