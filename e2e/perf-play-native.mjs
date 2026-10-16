// Exact saved perf-grass world, native release, guarded separately from browser runs.
// Usage: node e2e/perf-play-native.mjs <binary> <save.hexworld> <output-directory>
import { spawn } from "node:child_process";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { resolve, join } from "node:path";
import { guarded } from "./perf-load.mjs";
import { waitForBrowserExclusivity, monitorBrowserExclusivity } from "./perf-exclusivity.mjs";

const [bin, save, output] = process.argv.slice(2).map((p) => resolve(p));
await mkdir(output, { recursive: true });
const script = join(output, "native-script.json");
const dump = join(output, "native-state.json");
await writeFile(script, JSON.stringify([
  { cmd: "camera", pos: [10, 12, 40], look_at: [0, 0, 0] },
  { cmd: "wait", frames: 1000 }, { cmd: "exit" },
]));
const args = ["--scenario", "play", "--tier", "high", "--grass", "off", "--tilt", "off", "--seed", "1",
  "--fps", "off", "--present", "immediate", "--size", "2560x1440", "--diag", "--load", save,
  "--script", script, "--dump-state", dump];
if (process.env.PERF_EXCLUSIVE === "1") await waitForBrowserExclusivity();
const result = await guarded(async () => {
  const finishExclusivity = process.env.PERF_EXCLUSIVE === "1" ? monitorBrowserExclusivity() : null;
  const log = [];
  const code = await new Promise((accept, reject) => {
    const child = spawn(bin, args);
    child.stdout.on("data", (b) => log.push(b.toString()));
    child.stderr.on("data", (b) => log.push(b.toString()));
    const deadline = setTimeout(() => child.kill("SIGKILL"), 300_000);
    child.once("error", reject);
    child.once("close", (c) => { clearTimeout(deadline); accept(c); });
  });
  await writeFile(join(output, "native.log"), log.join(""));
  if (code !== 0) throw new Error(`native exited ${code}`);
  const state = JSON.parse(await readFile(dump, "utf8"));
  if (state.play?.tiles?.length !== 2066 || state.diag?.samples < 600)
    throw new Error(`native world/window mismatch: ${state.play?.tiles?.length}/${state.diag?.samples}`);
  if (state.tier?.tier !== "high" || state.grass?.density !== 0 || state.tilt?.enabled || state.play?.rising?.length)
    throw new Error("native quality/grass/tilt/settled state disagrees with workload");
  const exclusivity = finishExclusivity?.() ?? null;
  return { platform: "native", args, exclusivity, viewport: "2560x1440", device_scale_factor: 1,
    tiles: state.play.tiles.length, tier: state.tier, camera: state.camera, grass: state.grass, tilt: state.tilt,
    pacing_note: "Play may impose 60 Hz despite immediate presentation. Pacing is not CPU/GPU render cost.",
    ...state.diag, play_tiles: state.play.tiles.length };
}, "native exact 2066-tile play world");
await writeFile(join(output, "native.json"), JSON.stringify(result, null, 2) + "\n");
console.log(JSON.stringify(result, null, 2));
