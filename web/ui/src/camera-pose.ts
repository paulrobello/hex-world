// Where this viewer last looked at each saved world (owner request 2026-10-14): the orbit camera, and
// the walker when walking, kept per slot in localStorage outside the world save, so flying around
// costs no save and adds no undo entry. A reload or a Load puts the camera back; with no pose (a
// world from another browser) the world save's own camera stays. Storage is optional: every call is
// wrapped, and a store that throws or is missing leaves the game running without it.
import type { Cell, Command, OrbitPose, PlayState, State, Vec3, WalkState } from "./bridge.ts";

export const POSE_KEY = "hexWorld.camera";
/** How often a moved camera is written. */
export const POSE_INTERVAL_MS = 2000;
/** The most poses kept (the newest), whatever the slots say. */
export const POSE_CAP = 64;
/** A restore gives up waiting for its world, the walk ground or the walker after this long. */
export const RESTORE_TIMEOUT_MS = 20_000;
/** Frames the walk and photo modes must stay off before the camera is set (their exit writes the camera). */
const SETTLE_FRAMES = 3;
/** Time the engine gets to act on `walk_to` (it runs a frame or two after sending) before it is judged. */
const WALK_JUDGE_MS = 400;
/** The orbit read back within these counts as restored. */
const EPS_M = 0.01;
const EPS_RAD = 1e-4;
const EPS_DEG = 0.01;

export interface WalkPose {
  feet: Vec3;
  /** Degrees clockwise viewed from above, 0 toward -Z (as `walk_to` takes it). */
  heading: number;
  /** Degrees, up positive. */
  pitch: number;
  /** The tile under the feet when it was recorded. */
  cell: Cell;
}

export interface CameraPose {
  orbit: OrbitPose;
  /** Set while walking: the walker, over the orbit camera that leaving returns to. */
  walk: WalkPose | null;
  /** Unix ms of the write. */
  at: number;
}

export type PoseMap = Record<string, CameraPose>;

/** The part of `localStorage` used here. */
export interface PoseStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

const num = (v: unknown): v is number => typeof v === "number" && Number.isFinite(v);
const vec3 = (v: unknown): v is Vec3 => Array.isArray(v) && v.length === 3 && v.every(num);

function isOrbit(v: unknown): v is OrbitPose {
  const o = v as OrbitPose;
  return !!o && vec3(o.focus) && num(o.yaw) && num(o.pitch) && num(o.distance) && o.distance > 0;
}

function isWalk(v: unknown): v is WalkPose {
  const w = v as WalkPose;
  return (
    !!w &&
    vec3(w.feet) &&
    num(w.heading) &&
    num(w.pitch) &&
    !!w.cell &&
    Number.isInteger(w.cell.q) &&
    Number.isInteger(w.cell.r)
  );
}

export function isPose(v: unknown): v is CameraPose {
  const p = v as CameraPose;
  return !!p && isOrbit(p.orbit) && (p.walk === null || isWalk(p.walk)) && num(p.at);
}

/** The camera pose on screen, or null without an orbit camera (other scenarios, before the camera exists). */
export function poseOf(
  play: PlayState | null | undefined,
  walk: WalkState | null | undefined,
  at: number,
): CameraPose | null {
  const orbit = play?.orbit;
  if (!play || !isOrbit(orbit)) return null;
  let w: WalkPose | null = null;
  if (play.walk_mode && walk?.on && walk.feet && walk.cell && num(walk.heading) && num(walk.pitch))
    w = { feet: walk.feet, heading: walk.heading, pitch: walk.pitch, cell: { q: walk.cell.q, r: walk.cell.r } };
  return {
    orbit: { focus: [...orbit.focus], yaw: orbit.yaw, pitch: orbit.pitch, distance: orbit.distance },
    walk: w,
    at,
  };
}

/** The smallest difference between two angles of the given period (2π radians, or 360 degrees). */
export function angleDiff(a: number, b: number, period: number): number {
  const d = (((a - b) % period) + period) % period;
  return Math.min(d, period - d);
}

const near3 = (a: Vec3, b: Vec3, eps: number) => a.every((v, i) => Math.abs(v - (b[i] ?? Number.NaN)) <= eps);

export function sameOrbit(a: OrbitPose, b: OrbitPose): boolean {
  return (
    near3(a.focus, b.focus, EPS_M) &&
    angleDiff(a.yaw, b.yaw, 2 * Math.PI) <= EPS_RAD &&
    Math.abs(a.pitch - b.pitch) <= EPS_RAD &&
    Math.abs(a.distance - b.distance) <= EPS_M
  );
}

/** Whether two poses look the same (the camera has not moved between them). */
export function samePose(a: CameraPose, b: CameraPose): boolean {
  if (!sameOrbit(a.orbit, b.orbit)) return false;
  if (!a.walk || !b.walk) return !a.walk && !b.walk;
  return (
    near3(a.walk.feet, b.walk.feet, EPS_M) &&
    angleDiff(a.walk.heading, b.walk.heading, 360) <= EPS_DEG &&
    Math.abs(a.walk.pitch - b.walk.pitch) <= EPS_DEG
  );
}

/** The stored poses, keeping only well-formed entries; anything unreadable is an empty map. */
export function parsePoses(raw: string | null): PoseMap {
  let v: unknown;
  try {
    v = JSON.parse(raw ?? "null");
  } catch {
    return {};
  }
  if (!v || typeof v !== "object" || Array.isArray(v)) return {};
  const out: PoseMap = {};
  for (const [id, p] of Object.entries(v as Record<string, unknown>)) if (isPose(p)) out[id] = p;
  return out;
}

/** `map` with `id` set to `pose`, keeping the `cap` newest entries. */
export function withPose(map: PoseMap, id: string, pose: CameraPose, cap = POSE_CAP): PoseMap {
  const all = Object.entries({ ...map, [id]: pose }).sort(([, a], [, b]) => b.at - a.at);
  return Object.fromEntries(all.slice(0, cap));
}

/** `map` without the poses of worlds no longer stored. */
export function onlySlots(map: PoseMap, ids: Iterable<string>): PoseMap {
  const keep = new Set(ids);
  return Object.fromEntries(Object.entries(map).filter(([id]) => keep.has(id)));
}

/** The `camera` command that puts the orbit camera back: `pos` on the orbit, looking at its focus. */
export function orbitCommand(o: OrbitPose): Command {
  const c = Math.cos(o.pitch);
  const pos: Vec3 = [
    o.focus[0] + c * Math.sin(o.yaw) * o.distance,
    o.focus[1] + Math.sin(o.pitch) * o.distance,
    o.focus[2] + c * Math.cos(o.yaw) * o.distance,
  ];
  return { cmd: "camera", pos, look_at: [...o.focus] };
}

/** Whether a walk pose may be restored: its tile is still placed (not bulldozed, not off the map). */
export function walkAllowed(w: WalkPose, tiles: readonly { q: number; r: number }[]): boolean {
  return tiles.some((t) => t.q === w.cell.q && t.r === w.cell.r);
}

/**
 * Whether the walker's tile is not walkable ground yet: the core kit is still coming, or the tile is
 * hidden waiting for its pack. Other packs streaming in the background do not hold the walker.
 */
export function groundPending(state: State, w: WalkPose): boolean {
  const core = state.packs?.core;
  if (core && core !== "ready") return true;
  return (state.pack_waiting ?? []).some((t) => t.q === w.cell.q && t.r === w.cell.r);
}

type Phase =
  | { kind: "settle"; frames: number }
  | { kind: "orbit"; sentAt: number; resent: boolean }
  | { kind: "ground" }
  | { kind: "walk"; sentAt: number; retried: boolean };

interface Restore {
  id: string;
  pose: CameraPose;
  phase: Phase;
  deadline: number;
  /**
   * The orbit camera as the load left it: the first `play.orbit` seen once the world is up (the state
   * a `loaded` event arrives with already holds the save's camera, also while a walk or photo exit
   * is still writing it back). Any other camera before the restore's own reads back is someone
   * else's move, and the restore yields to it.
   */
  anchor: OrbitPose | null;
}

export interface KeeperOptions {
  /** The store, or null when there is none; may throw. */
  storage: () => PoseStorage | null;
  now?: () => number;
  intervalMs?: number;
}

/**
 * What a restore ended with: the orbit or the walker put back, a fallback to the orbit (the walker's
 * tile is gone or `walk_to` was refused), the deadline, or `yielded` to another camera move.
 */
export type RestoreOutcome = "orbit" | "walk" | "fallback" | "timeout" | "yielded";

/** `hexWorld.cameraRestore()`: whether a restore is running, its phase, and how the latest one ended. */
export interface RestoreStatus {
  restoring: boolean;
  phase: Phase["kind"] | null;
  outcome: RestoreOutcome | null;
}

/**
 * Records the camera of the world on screen every couple of seconds when it moved, and on demand
 * (page hide, world switch), and puts a stored world's camera back when it is opened. Recording
 * pauses while a restore runs, so the loaded save's own camera never overwrites the pose being put
 * back.
 */
export class CameraKeeper {
  /** The latest finished restore, for tests and the e2e. */
  lastOutcome: RestoreOutcome | null = null;

  private readonly host: { cmd(c: Command): void };
  private readonly opts: KeeperOptions;
  private readonly now: () => number;
  private readonly interval: number;
  private latest: { id: string; pose: CameraPose } | null = null;
  /** The pose last written (or restored) for `baseId`, the baseline "moved" compares with. */
  private base: CameraPose | null = null;
  private baseId: string | null = null;
  private nextCheck = 0;
  private restore: Restore | null = null;

  constructor(host: { cmd(c: Command): void }, opts: KeeperOptions) {
    this.host = host;
    this.opts = opts;
    this.now = opts.now ?? Date.now;
    this.interval = opts.intervalMs ?? POSE_INTERVAL_MS;
  }

  get restoring(): boolean {
    return this.restore !== null;
  }

  status(): RestoreStatus {
    return { restoring: this.restore !== null, phase: this.restore?.phase.kind ?? null, outcome: this.lastOutcome };
  }

  /** The stored pose of a world, or null (none, or no readable store). */
  read(id: string): CameraPose | null {
    return this.load()[id] ?? null;
  }

  /**
   * Every frame: `id` is the stored world on screen, or null while there is none to record for (no
   * slot yet, the start-up resume pending, storage off).
   */
  tick(id: string | null, state: State): void {
    const play = state.play ?? null;
    if (this.restore) {
      this.step(this.restore, state);
      return;
    }
    const pose = id ? poseOf(play, state.walk, this.now()) : null;
    if (!id || !pose) {
      this.latest = null;
      return;
    }
    this.latest = { id, pose };
    if (id !== this.baseId) {
      this.baseId = id;
      this.base = this.read(id);
    }
    const t = this.now();
    if (t < this.nextCheck) return;
    this.nextCheck = t + this.interval;
    this.record();
  }

  /** Writes the latest pose now if it moved (a page hide, or ahead of a world switch). Skipped while restoring. */
  flush(): void {
    if (!this.restore) this.record();
  }

  /** A stored world came up (the start-up resume or a Load): puts its viewer's camera back, if any. */
  begin(id: string): void {
    this.latest = null;
    this.baseId = id;
    const pose = this.read(id);
    this.base = pose;
    this.restore = pose
      ? { id, pose, phase: { kind: "settle", frames: 0 }, deadline: this.now() + RESTORE_TIMEOUT_MS, anchor: null }
      : null;
  }

  /** Drops a deleted world's pose. */
  forget(id: string): void {
    const map = this.load();
    if (!(id in map)) return;
    delete map[id];
    this.save(map);
  }

  /** Drops the poses of worlds that are no longer stored. */
  keepOnly(ids: Iterable<string>): void {
    const map = this.load();
    const kept = onlySlots(map, ids);
    if (Object.keys(kept).length !== Object.keys(map).length) this.save(kept);
  }

  private record(): void {
    const l = this.latest;
    if (!l || (this.baseId === l.id && this.base && samePose(this.base, l.pose))) return;
    const pose = { ...l.pose, at: this.now() };
    if (this.save(withPose(this.load(), l.id, pose))) {
      this.baseId = l.id;
      this.base = pose;
    }
  }

  private finish(outcome: RestoreOutcome): void {
    this.restore = null;
    this.lastOutcome = outcome;
    // The next check writes the camera only if it differs from the stored pose (a fallback does).
    this.nextCheck = 0;
  }

  private step(r: Restore, state: State): void {
    const play = state.play ?? null;
    const t = this.now();
    if (t > r.deadline) {
      this.finish("timeout");
      return;
    }
    if (!play) return;
    const p = r.phase;
    const now = play.orbit;
    if (!r.anchor && now) r.anchor = now;
    switch (p.kind) {
      case "settle": {
        // The viewer (or a script) moved the camera before the restore sent its own: theirs wins.
        if (now && r.anchor && !sameOrbit(now, r.anchor)) {
          this.finish("yielded");
          return;
        }
        // A load that lands while walking or in photo mode leaves the mode and writes its camera back.
        p.frames = play.walk_mode || play.photo_mode ? 0 : p.frames + 1;
        if (p.frames < SETTLE_FRAMES) return;
        this.host.cmd(orbitCommand(r.pose.orbit));
        r.phase = { kind: "orbit", sentAt: t, resent: false };
        return;
      }
      case "orbit": {
        if (!now || !sameOrbit(now, r.pose.orbit)) {
          // Neither the restored pose nor the camera it was sent from: another move landed. No resend.
          if (now && r.anchor && !sameOrbit(now, r.anchor)) {
            this.finish("yielded");
            return;
          }
          if (!p.resent && t - p.sentAt > 1000) {
            this.host.cmd(orbitCommand(r.pose.orbit));
            p.resent = true;
          }
          return;
        }
        const w = r.pose.walk;
        if (!w) {
          this.finish("orbit");
          return;
        }
        if (!walkAllowed(w, play.tiles)) {
          this.finish("fallback");
          return;
        }
        r.phase = { kind: "ground" };
        return;
      }
      case "ground": {
        // The camera moved off the restored orbit while the walker waited for its ground: stay there.
        if (now && !sameOrbit(now, r.pose.orbit)) {
          this.finish("yielded");
          return;
        }
        const w = r.pose.walk as WalkPose;
        if (groundPending(state, w)) return;
        this.host.cmd({ cmd: "walk", on: true, pointer_lock: null });
        this.host.cmd(walkTo(w));
        r.phase = { kind: "walk", sentAt: t, retried: false };
        return;
      }
      case "walk": {
        const w = r.pose.walk as WalkPose;
        const walk = state.walk;
        if (t - p.sentAt < WALK_JUDGE_MS) return;
        // Walk mode was refused (photo mode): the orbit camera stays.
        if (!play.walk_mode || !walk?.on) {
          if (t - p.sentAt > 3000) this.finish("fallback");
          return;
        }
        // walk_to clears `blocked` when it stands the walker (on the spot, or snapped within 4 m), and
        // a refusal sets it; nothing else moves the walker during a restore.
        if (!walk.blocked) {
          this.finish("walk");
          return;
        }
        if (!p.retried) {
          // The ground may have been a frame behind its pack: one more try.
          this.host.cmd(walkTo(w));
          p.retried = true;
          p.sentAt = t;
          return;
        }
        this.host.cmd({ cmd: "walk", on: false, pointer_lock: null });
        this.finish("fallback");
        return;
      }
    }
  }

  private load(): PoseMap {
    try {
      return parsePoses(this.opts.storage()?.getItem(POSE_KEY) ?? null);
    } catch {
      return {};
    }
  }

  private save(map: PoseMap): boolean {
    try {
      const s = this.opts.storage();
      if (!s) return false;
      s.setItem(POSE_KEY, JSON.stringify(map));
      return true;
    } catch {
      return false;
    }
  }
}

function walkTo(w: WalkPose): Command {
  return { cmd: "walk_to", x: w.feet[0], z: w.feet[2], heading: w.heading, pitch: w.pitch };
}

/** `window.localStorage`, or null when even reading it throws (blocked site data). */
export function localPoseStorage(): PoseStorage | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}
