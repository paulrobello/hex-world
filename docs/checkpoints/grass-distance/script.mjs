// Grass distance checkpoint (owner request 2026-10-19): the water world of e2e/water-world.mjs at
// radius 8 (land out past 150 m west of the coast), then the same three views at 40, 80 and 150 m
// from the look-at point at Near and at Far, set live with `set_grass_distance`, each after a wait
// long enough for the per-frame build budget to fill the larger radius, with a state dump per view.
// usage: node docs/checkpoints/grass-distance/script.mjs <out.json>
import { writeFile } from "node:fs/promises";
import { waterWorldScript } from "../../../e2e/water-world.mjs";

const LOOK = [-10, 0, 30];
// Pitched about 30 degrees down, from the south-west so the view runs over meadows and forests.
const view = (d) => {
  const a = (30 * Math.PI) / 180;
  const flat = d * Math.cos(a);
  return {
    cmd: "camera",
    pos: [LOOK[0] - flat * 0.5, d * Math.sin(a), LOOK[2] + flat * 0.866],
    look_at: LOOK,
  };
};

const { script } = waterWorldScript(8);
const out = [...script, { cmd: "wait", frames: 600 }];
for (const [name, scale] of [
  ["near", 1],
  ["far", 2],
]) {
  out.push({ cmd: "set_grass_distance", scale });
  for (const d of [40, 80, 150]) {
    out.push(view(d), { cmd: "wait", frames: 300 });
    out.push({ cmd: "screenshot", name: `grass-distance-${name}-${d}m` });
    out.push({ cmd: "dump_state", name: `grass-distance-state-${name}-${d}m` });
  }
}
out.push({ cmd: "exit" });
await writeFile(process.argv[2], `${JSON.stringify(out)}\n`);
console.log(`${out.length} commands`);
