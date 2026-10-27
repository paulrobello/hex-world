// Close-camera draw-call work A/B natively (docs/perf/close-camera-draws/README.md): two release
// binaries, the 20-tile scatter world (docs/checkpoints/lod-band-instanced/draws.json placements) at
// its overview, forest-cluster, meadow and 152 m band cameras, and the exact 2,066-tile save
// (docs/perf/post-arc001-2026-10-16/local-world.hexworld) at its close camera and an overview.
// play, High, grass off, tilt off, seed 1, fps off, present immediate, 2560x1440. Each camera window
// is dumped 700 frames after the move (the 600-frame diag ring holds only it). Builds alternate order
// each round; every run sits inside e2e/perf-load.mjs's guard (1-minute load <= 15).
// Each build runs from its own checkout (<repo>/target/release/hex-world with <repo> as the working
// directory), because a native build loads ./assets/shaders at run time and the two commits' kit
// shaders differ.
// The 31-tile mixed world (docs/checkpoints/scatter/scatter-mixed.json) runs at its every-tile overview and
// the 20-tile overview pose. WORLDS (comma list) runs only the named worlds; every world by default.
// usage: node e2e/perf-close-camera-native.mjs <repo-before> <repo-after> <out.json>   ROUNDS (default 5), WORLDS
import { spawn } from "node:child_process";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { cpus, hostname } from "node:os";
import { join, resolve } from "node:path";
import { guarded, LOAD_LIMIT } from "./perf-load.mjs";

const [repoBefore, repoAfter, OUT] = process.argv.slice(2);
if (!OUT) throw new Error("usage: node e2e/perf-close-camera-native.mjs <repo-before> <repo-after> <out.json>");
const ROUNDS = Number(process.env.ROUNDS ?? 5);
const SAVE = resolve(process.env.SAVE ?? "docs/perf/post-arc001-2026-10-16/local-world.hexworld");
const REPOS = { before: resolve(repoBefore), after: resolve(repoAfter) };
const binOf = (build) => join(REPOS[build], "target/release/hex-world");
const WINDOW_FRAMES = 700;
const shotsOf = (build) => join(REPOS[build], "target/shots");
const ARGS = ["--scenario", "play", "--tier", "high", "--grass", "off", "--tilt", "off", "--seed", "1", "--fps", "off",
  "--present", "immediate", "--size", "2560x1440", "--diag"];

const place = (list) => list.flatMap(([id, cells]) => [{ cmd: "select_tile", id }, ...cells.map(([q, r]) => ({ cmd: "place", q, r }))]);
const world20 = place([["forest", [[1, 0]]], ["oak", [[2, 0]]], ["forest", [[1, -1], [2, -1]]], ["oak", [[3, -1]]],
  ["forest", [[3, 0], [0, -1]]], ["meadow", [[-1, 0]]], ["river_straight", [[-1, -1]]], ["road_straight", [[1, -2], [2, -2]]],
  ["house_cluster", [[3, -2]]], ["meadow", [[-1, 1], [0, 1], [1, 1], [0, 2], [-1, 2]]], ["forest", [[2, 1], [1, 2]]]]);
const cam = ([x, y, z, lx, ly, lz]) => ({ cmd: "camera", pos: [x, y, z], look_at: [lx, ly, lz] });
// As the draws census (draws.json): the pointer off the world, photo mode on with DOF off, every
// pack landed (780 frames) before the first window.
const settle = [{ cmd: "hover", q: 9, r: -9 }, { cmd: "toggle", what: "photo" },
  { cmd: "photo", dof: "off", pause_agents: false }, { cmd: "wait", frames: 780 }];
// The 31-tile mixed river, beach and ocean world of make shots-scatter (owner A24 Q5): its placements,
// read from the checkpoint script, at its every-tile overview and at the 20-tile overview pose.
const mixedScript = JSON.parse(await readFile("docs/checkpoints/scatter/scatter-mixed.json", "utf8"))
  .filter((c) => ["select_tile", "hover", "place"].includes(c.cmd));
const WORLDS = {
  world20: { tiles: 20, args: [], setup: [cam([20, 160, 150, 20, 0, -5]), { cmd: "wait", frames: 30 }, ...world20, ...settle],
    cameras: {
      overview: [20, 160, 150, 20, 0, -5],
      forest_cluster: [46, 58, 48, 46, 0, -12],
      meadow: [6, 13, 44, 12, 0, 22],
      band152: [20, 56, 136, 20, 0, -5],
    } },
  world2066: { tiles: 2066, args: ["--load", SAVE], setup: [...settle, { cmd: "wait", frames: 600 }], cameras: {
    close: [10, 12, 40, 0, 0, 0],
    overview: [20, 160, 150, 20, 0, -5],
  } },
  mixed: { tiles: 31, args: [], setup: [cam([12, 172, 165, 12, 0, -2]), { cmd: "wait", frames: 30 }, ...mixedScript, ...settle],
    cameras: {
      overview: [12, 172, 165, 12, 0, -2],
      pose20: [20, 160, 150, 20, 0, -5],
    } },
};
const RUN_WORLDS = (process.env.WORLDS ?? Object.keys(WORLDS).join(",")).split(",").filter((w) => {
  if (!WORLDS[w]) throw new Error(`unknown world ${w}; known: ${Object.keys(WORLDS).join(", ")}`);
  return true;
});

async function run(build, worldName) {
  const world = WORLDS[worldName];
  const name = (c) => `ccd-${build}-${worldName}-${c}`;
  const script = [...world.setup];
  for (const [c, pose] of Object.entries(world.cameras))
    script.push(cam(pose), { cmd: "wait", frames: WINDOW_FRAMES }, { cmd: "dump_state", name: name(c) });
  script.push({ cmd: "exit" });
  const SHOTS = shotsOf(build);
  await mkdir(SHOTS, { recursive: true });
  const scriptPath = join(SHOTS, `ccd-${build}-${worldName}-script.json`);
  await writeFile(scriptPath, JSON.stringify(script));
  for (const c of Object.keys(world.cameras)) await rm(join(SHOTS, `${name(c)}.json`), { force: true });
  const args = [...ARGS, ...world.args, "--script", scriptPath];
  const log = [];
  const code = await new Promise((accept, reject) => {
    const child = spawn(binOf(build), args, { cwd: REPOS[build] });
    child.stdout.on("data", (b) => log.push(b.toString()));
    child.stderr.on("data", (b) => log.push(b.toString()));
    const deadline = setTimeout(() => child.kill("SIGKILL"), 900_000);
    child.once("error", reject);
    child.once("close", (c) => { clearTimeout(deadline); accept(c); });
  });
  await writeFile(join(SHOTS, `ccd-${build}-${worldName}.log`), log.join(""));
  if (code !== 0) throw new Error(`native exited ${code}`);
  const cameras = {};
  for (const c of Object.keys(world.cameras)) {
    const st = JSON.parse(await readFile(join(SHOTS, `${name(c)}.json`), "utf8"));
    const d = st.diag ?? {};
    if (st.play?.tiles?.length !== world.tiles) throw new Error(`world has ${st.play?.tiles?.length} tiles`);
    if (st.tier?.tier !== "high" || (d.samples ?? 0) < 600) throw new Error(`tier ${st.tier?.tier}, ${d.samples} samples`);
    const count = (p) => p.multidraw_sets + p.batchable_bins + p.unbatchable + p.non_mesh;
    cameras[c] = { p50: d.p50, p95: d.p95, p99: d.p99, mean: d.mean, max: d.max, dropped_over_25ms: d.dropped_over_25ms,
      samples: d.samples, draws: (d.passes ?? []).reduce((n, p) => n + count(p), 0),
      kit_draws: d.kit_draws ? { slots: d.kit_draws.slots, total: d.kit_draws.total } : null,
      gpu_timing: d.gpu ?? null, camera_pos: st.camera?.pos };
  }
  return { build, world: worldName, args, cameras };
}

const result = { date: new Date().toISOString(), host: hostname(), cpu: cpus()[0]?.model, repos: REPOS, args: ARGS,
  save: SAVE, window_frames: WINDOW_FRAMES, load_limit: LOAD_LIMIT, rounds: ROUNDS,
  pacing_note: "Play may impose 60 Hz despite immediate presentation; compare mean and p95, and the GPU timing when present.",
  rows: [] };
for (let r = 0; r < ROUNDS; r++)
  for (const worldName of RUN_WORLDS)
    for (const build of r % 2 ? ["after", "before"] : ["before", "after"]) {
      const label = `native ${build} ${worldName} round ${r}`;
      const g = await guarded(() => run(build, worldName).catch((e) => ({ build, world: worldName, error: String(e.message ?? e) })), label);
      result.rows.push({ round: r, ...g.result, load_before: g.load_before, load_after: g.load_after, load_discarded: g.load_discarded });
      const x = result.rows.at(-1);
      console.log(`${label}: ${x.error ?? Object.entries(x.cameras).map(([n, c]) => `${n} p50 ${c.p50} p95 ${c.p95} mean ${c.mean} draws ${c.draws}`).join(" | ")}` +
        ` (load ${g.load_before.toFixed(2)} -> ${g.load_after.toFixed(2)})`);
      await writeFile(OUT, `${JSON.stringify(result, null, 2)}\n`);
    }
console.log(`wrote ${OUT}`);
