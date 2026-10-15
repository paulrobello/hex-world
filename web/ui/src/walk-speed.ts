// The walk speed setting (owner, 2026-10-14): the Settings sheet's World tab slider and the speed the
// overlay sends at startup. It is kept per viewer in localStorage, outside the world save, like the
// colour grade. Free of the DOM so Node tests read it directly.

/** The viewer's walk speed (m/s) in localStorage. */
export const WALK_SPEED_KEY = "hexWorld.walkSpeed";
/** The engine's range (m/s, `hex-app` `walk::WALK_SPEED` and `WALK_SPEED_MAX`); the default is the lowest. */
export const WALK_SPEED_MIN = 2.8;
export const WALK_SPEED_MAX = 8;
export const WALK_SPEED_STEP = 0.1;
/** Shift sprints at this many times the walk speed, whatever the setting. */
export const SPRINT_FACTOR = 2;

/** `v` on the slider's 0.1 m/s step, within the range. */
export function clampWalkSpeed(v: number): number {
  const stepped = Math.round(v * 10) / 10;
  return Math.min(WALK_SPEED_MAX, Math.max(WALK_SPEED_MIN, stepped));
}

/** A stored speed, or null when nothing or nothing numeric is stored. */
export function storedWalkSpeed(raw: string | null): number | null {
  if (raw == null || raw.trim() === "") return null;
  const v = Number(raw);
  return Number.isFinite(v) ? clampWalkSpeed(v) : null;
}

export const walkSpeedCommand = (speed: number): { cmd: "set_walk_speed"; speed: number } => ({
  cmd: "set_walk_speed",
  speed: clampWalkSpeed(speed),
});

/** "2.8 m/s". */
export const speedLabel = (v: number): string => `${v.toFixed(1)} m/s`;

/** The slider's filled share (0 to 100) at `v`. */
export const walkSpeedPct = (v: number): number =>
  ((clampWalkSpeed(v) - WALK_SPEED_MIN) / (WALK_SPEED_MAX - WALK_SPEED_MIN)) * 100;
