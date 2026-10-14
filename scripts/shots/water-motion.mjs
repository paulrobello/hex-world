// Writes the water motion-capture script: for each still in scripts/shots/water.json (except the
// overview), its camera followed by FRAMES consecutive screenshots. Run the app with
// `--water_clock fixed` so each frame advances water time by exactly 1/30 s (crates/hex-render/src/water.rs).
// usage: node scripts/shots/water-motion.mjs <out.json>
import { readFileSync, writeFileSync } from "node:fs";

export const FRAMES = 90;
const stills = JSON.parse(readFileSync(new URL("./water.json", import.meta.url), "utf8"));
const out = [{ cmd: "scenario", name: "water" }, { cmd: "wait", frames: 120 }];
let camera = null;
for (const c of stills) {
  if (c.cmd === "camera") camera = c;
  if (c.cmd !== "screenshot" || c.name === "water-overview") continue;
  const view = c.name.replace(/^water-/, "");
  out.push(camera, { cmd: "wait", frames: 30 });
  for (let f = 0; f < FRAMES; f++) out.push({ cmd: "screenshot", name: `water-motion-${view}-${String(f).padStart(3, "0")}` });
}
out.push({ cmd: "exit" });
writeFileSync(process.argv[2], JSON.stringify(out) + "\n");
console.log(`wrote ${process.argv[2]}: ${out.length} commands`);
