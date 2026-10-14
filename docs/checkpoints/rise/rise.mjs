// Writes the staged-rise proof script (PRD §4.4, design/ui/legend/in-world.md): one rise per case,
// captured on every frame from the frame after the placement, then the fast-forward, undo-mid-rise
// and reduced-motion cases and the in-world affordance set. Run the app with `--clock fixed` (and
// `--water_clock fixed`) so each frame advances the rise by exactly 1/60 s: frame fNNN of a case
// shows the rise at t = (NNN + 2) / 60 s. usage: node docs/checkpoints/rise/rise.mjs <out.json>
import { writeFileSync } from "node:fs";

// Frames captured per rise: the default 2.0 s timeline plus a settled tail.
export const FRAMES = 132;
const R = 16;
const centre = (q, r) => [R * Math.sqrt(3) * (q + r / 2), R * 1.5 * r];
const closeup = (q, r) => {
  const [x, z] = centre(q, r);
  return { cmd: "camera", pos: [x + 7, 24, z + 40], look_at: [x, -1.5, z] };
};
const frames = (name, n) =>
  Array.from({ length: n }, (_, i) => ({ cmd: "screenshot", name: `${name}-f${String(i).padStart(3, "0")}` }));

// (case, tile, q, r): placement order keeps every placement legal with auto-rotate.
export const CASES = [
  ["meadow", "meadow", 0, -1],
  ["house", "house_cluster", 0, 1],
  ["river", "river_straight", -1, 0],
  ["ocean", "ocean", 2, 0],
];

const out = [
  { cmd: "camera", pos: [14, 95, 125], look_at: [14, 0, 16] },
  { cmd: "wait", frames: 40 },
];
for (const [name, tile, q, r] of CASES) {
  if (name === "ocean") {
    // The ocean needs the beach between it and the meadows.
    out.push({ cmd: "select_tile", id: "beach" }, { cmd: "place", q: 1, r: 0 }, { cmd: "wait", frames: 150 });
  }
  out.push(closeup(q, r), { cmd: "select_tile", id: tile }, { cmd: "wait", frames: 20 });
  out.push({ cmd: "place", q, r }, ...frames(`rise-${name}`, FRAMES));
  out.push({ cmd: "dump_state", name: `rise-state-${name}` }, { cmd: "wait", frames: 10 });
}

// Fast-forward: a forest mid-rise jumps to its end state when the next tile lands.
out.push(closeup(0.5, -2), { cmd: "select_tile", id: "forest" }, { cmd: "wait", frames: 20 });
out.push({ cmd: "place", q: 0, r: -2 }, ...frames("rise-ff-a", 34));
out.push({ cmd: "select_tile", id: "meadow" }, { cmd: "place", q: 1, r: -2 }, ...frames("rise-ff-b", 40));
out.push({ cmd: "dump_state", name: "rise-state-fast-forward" }, { cmd: "wait", frames: 150 });

// Undo mid-rise: the road sinks from where it is (400 ms ease-in) and nothing is left behind.
out.push(closeup(2, -2), { cmd: "select_tile", id: "road_straight" }, { cmd: "wait", frames: 20 });
out.push({ cmd: "place", q: 2, r: -2 }, ...frames("rise-undo-a", 40));
out.push({ cmd: "undo" }, ...frames("rise-undo-b", 40));
out.push({ cmd: "dump_state", name: "rise-state-undo" }, { cmd: "wait", frames: 10 });

// Reduced motion: an instant cut, no dust or shake.
out.push(closeup(0, 2), { cmd: "set_reduced_motion", on: true }, { cmd: "select_tile", id: "meadow" });
out.push({ cmd: "place", q: 0, r: 2 }, { cmd: "screenshot", name: "rise-reduced-motion" });
out.push({ cmd: "dump_state", name: "rise-state-reduced-motion" }, { cmd: "set_reduced_motion", on: false });

// The affordance set: slot ring, hovered slot, a valid ghost with compatible edges and socket key
// marks, an invalid ghost with an incompatible edge and its refused pulse, then bulldoze.
const [ax, az] = centre(1, 0);
out.push({ cmd: "camera", pos: [ax + 2, 62, az + 72], look_at: [ax + 2, 0, az + 6] }, { cmd: "wait", frames: 30 });
out.push({ cmd: "select_tile", id: "meadow" }, { cmd: "hover", q: 1, r: -1 }, { cmd: "wait", frames: 40 });
out.push({ cmd: "screenshot", name: "aff-01-valid-ghost" }, { cmd: "dump_state", name: "aff-state-valid-ghost" });
out.push({ cmd: "select_tile", id: "ocean" }, { cmd: "hover", q: 1, r: 1 }, { cmd: "wait", frames: 40 });
out.push({ cmd: "screenshot", name: "aff-02-incompatible-edge" }, { cmd: "dump_state", name: "aff-state-incompatible" });
out.push({ cmd: "place", q: 1, r: 1 }, { cmd: "wait", frames: 5 }, { cmd: "screenshot", name: "aff-03-refused-pulse" });
out.push({ cmd: "set_tool", mode: "bulldoze" }, { cmd: "hover", q: 2, r: 0 }, { cmd: "wait", frames: 30 });
out.push({ cmd: "screenshot", name: "aff-04-bulldoze-removable" });
out.push({ cmd: "hover", q: 1, r: 0 }, { cmd: "wait", frames: 30 }, { cmd: "screenshot", name: "aff-05-bulldoze-split" });
out.push({ cmd: "bulldoze", q: 1, r: 0 }, { cmd: "wait", frames: 5 }, { cmd: "screenshot", name: "aff-06-bulldoze-refused-pulse" });
out.push({ cmd: "dump_state", name: "aff-state-bulldoze-refused" }, { cmd: "set_tool", mode: "place" }, { cmd: "exit" });

writeFileSync(process.argv[2], JSON.stringify(out) + "\n");
console.log(`wrote ${process.argv[2]}: ${out.length} commands`);
