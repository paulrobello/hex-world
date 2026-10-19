// Grass distance stops without screenshots: the water world of script.mjs, the 80 m view, then
// Medium, Very far and back to Near set live, a state dump after each (groups, blades, mesh_bytes,
// last_build_ms), so growing builds the newly reached tiles and shrinking drops them.
// usage: node docs/checkpoints/grass-distance/stops.mjs <out.json>; run natively with the
// run.sh keys and copy target/shots/grass-distance-state-stop-*.json here.
import { writeFile } from "node:fs/promises";
import { waterWorldScript } from "../../../e2e/water-world.mjs";

const LOOK = [-10, 0, 30];
const a = (30 * Math.PI) / 180;
const flat = 80 * Math.cos(a);
const cam = { cmd: "camera", pos: [LOOK[0] - flat * 0.5, 80 * Math.sin(a), LOOK[2] + flat * 0.866], look_at: LOOK };
const out = [...waterWorldScript(8).script, cam, { cmd: "wait", frames: 600 }];
for (const [name, scale] of [
  ["medium", 1.5],
  ["very-far", 3],
  ["near-after-very-far", 1],
]) {
  out.push({ cmd: "set_grass_distance", scale }, { cmd: "wait", frames: 400 });
  out.push({ cmd: "dump_state", name: `grass-distance-state-stop-${name}-80m` });
}
out.push({ cmd: "exit" });
await writeFile(process.argv[2], `${JSON.stringify(out)}\n`);
