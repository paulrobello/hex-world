// Writes the play-water motion-capture script: the placements of water-play.json (everything before
// its first camera), then for each still in VIEWS its camera followed by FRAMES consecutive
// screenshots. Run the app with `--water_clock fixed` so each frame advances water time by exactly
// 1/30 s. usage: node docs/checkpoints/water-play/motion.mjs <out.json>
import { readFileSync, writeFileSync } from "node:fs";

export const FRAMES = 90;
export const VIEWS = ["coast", "seam-river", "seam-coast", "edge-ocean"];
const stills = JSON.parse(readFileSync(new URL("./water-play.json", import.meta.url), "utf8"));
const first = stills.findIndex((c) => c.cmd === "camera");
const out = stills.slice(0, first).filter((c) => c.cmd !== "dump_state");
let camera = null;
for (const c of stills.slice(first)) {
  if (c.cmd === "camera") camera = c;
  if (c.cmd !== "screenshot") continue;
  const view = c.name.replace(/^water-play-\d+-/, "");
  if (!VIEWS.includes(view)) continue;
  out.push(camera, { cmd: "wait", frames: 30 });
  for (let f = 0; f < FRAMES; f++) out.push({ cmd: "screenshot", name: `water-play-motion-${view}-${String(f).padStart(3, "0")}` });
}
out.push({ cmd: "exit" });
writeFileSync(process.argv[2], JSON.stringify(out) + "\n");
console.log(`wrote ${process.argv[2]}: ${out.length} commands`);
