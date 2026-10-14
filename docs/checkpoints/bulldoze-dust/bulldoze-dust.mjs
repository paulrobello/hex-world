// Writes the bulldoze dust-cloud proof script (owner request 2026-10-14, design/ui/legend/in-world.md
// Bulldoze): a house cluster bulldozed inside its dust cloud, a forest placement undone with the
// small puff for comparison, and a reduced-motion bulldoze with no cloud. Run the app with
// `--clock fixed` (and `--water_clock fixed`) so each frame advances 1/60 s: frame fNNN of a case is
// captured NNN frames after the bulldoze or undo frame.
// usage: node docs/checkpoints/bulldoze-dust/bulldoze-dust.mjs <out.json>
import { writeFileSync } from "node:fs";

// Frames captured after the bulldoze: the 0.7 s sink, the cloud's ~2 s linger and a clear tail.
export const FRAMES = 200;
const R = 16;
const centre = (q, r) => [R * Math.sqrt(3) * (q + r / 2), R * 1.5 * r];
const view = (q, r) => {
  const [x, z] = centre(q, r);
  return { cmd: "camera", pos: [x + 8, 34, z + 58], look_at: [x, 2, z] };
};
const frames = (name, n) =>
  Array.from({ length: n }, (_, i) => ({ cmd: "screenshot", name: `${name}-f${String(i).padStart(3, "0")}` }));

const out = [
  { cmd: "camera", pos: [14, 95, 125], look_at: [14, 0, 16] },
  { cmd: "wait", frames: 40 },
  // A leaf tile next to the start meadow, so bulldozing it neither empties nor splits the world.
  view(0, 1),
  { cmd: "select_tile", id: "house_cluster" },
  { cmd: "place", q: 0, r: 1 },
  { cmd: "wait", frames: 150 },
  { cmd: "set_tool", mode: "bulldoze" },
  { cmd: "hover", q: 0, r: 1 },
  { cmd: "wait", frames: 30 },
  { cmd: "screenshot", name: "dust-bulldoze-before" },
  { cmd: "dump_state", name: "dust-state-before" },
  { cmd: "bulldoze", q: 0, r: 1 },
  ...frames("dust-bulldoze", FRAMES),
  { cmd: "dump_state", name: "dust-state-bulldozed" },
  { cmd: "set_tool", mode: "place" },
  // Undo of a placement keeps the small rim puff and the 400 ms sink.
  view(-1, 0),
  { cmd: "select_tile", id: "forest" },
  { cmd: "place", q: -1, r: 0 },
  { cmd: "wait", frames: 150 },
  { cmd: "screenshot", name: "dust-undo-before" },
  { cmd: "undo" },
  ...frames("dust-undo", 90),
  { cmd: "dump_state", name: "dust-state-undone" },
  // Reduced motion: the bulldozed tile is gone at once, with no cloud.
  view(0, 1),
  { cmd: "select_tile", id: "house_cluster" },
  { cmd: "place", q: 0, r: 1 },
  { cmd: "wait", frames: 150 },
  { cmd: "set_reduced_motion", on: true },
  { cmd: "bulldoze", q: 0, r: 1 },
  { cmd: "wait", frames: 2 },
  { cmd: "screenshot", name: "dust-reduced-motion" },
  { cmd: "dump_state", name: "dust-state-reduced-motion" },
  { cmd: "set_reduced_motion", on: false },
  { cmd: "exit" },
];

writeFileSync(process.argv[2], JSON.stringify(out) + "\n");
console.log(`wrote ${process.argv[2]}: ${out.length} commands`);
