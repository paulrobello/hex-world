// Low-camera play frame, natively (docs/perf/low-camera/README.md): the radius-10 water world of
// e2e/frame-pacing.mjs (317 tiles) at its low camera pos [10, 12, 40] looking at the origin, play,
// High, grass 1, tilt off, seed 1, fps off, present immediate, 2560x1440. The web split of
// `gpu_timing` is not usable (docs/agent-operability.md, --gpu_timing), so the per-pass GPU table
// comes from here: every arm but `base_off` runs with `--gpu_timing on`, which serializes the passes,
// so `base_off` gives the frame time without it. One process per arm; the arm order rotates each
// round, and every run sits inside e2e/perf-load.mjs's guard (1-minute load <= 15).
// The binary runs from <repo> (it loads ./assets at run time): <repo>/target/release/hex-world.
// usage: node e2e/perf-low-camera-native.mjs <repo> <out.json>   ROUNDS (default 3), ARMS (comma list),
//   LOWCAM_SIZE (default 2560x1440), LOWCAM_WAIT (frames after the camera move, default 700)
import { spawn } from "node:child_process";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { cpus, hostname } from "node:os";
import { join, resolve } from "node:path";
import { guarded, LOAD_LIMIT } from "./perf-load.mjs";
import { waterWorldScript } from "./water-world.mjs";

const [repoArg, OUT] = process.argv.slice(2);
if (!OUT) throw new Error("usage: node e2e/perf-low-camera-native.mjs <repo> <out.json>");
const REPO = resolve(repoArg);
const BIN = join(REPO, "target/release/hex-world");
const ROUNDS = Number(process.env.ROUNDS ?? 3);
const SIZE = process.env.LOWCAM_SIZE ?? "2560x1440";
const WAIT = Number(process.env.LOWCAM_WAIT ?? 700);
const world = waterWorldScript(10);
const CAMERA = { cmd: "camera", pos: [10, 12, 40], look_at: [0, 0, 0] };
const BASE = { scenario: "play", tier: "high", grass: "1", tilt: "off", seed: "1", fps: "off", present: "immediate", diag: "1" };
const T = { gpu_timing: "on" };
// `expect` is checked against the dumped diag and state, so a flag that does not apply in play fails
// the row instead of reading as a zero saving.
const ARMS = {
  base_off: { flags: {}, expect: { tier: "high", shadows: true, ssao: true } },
  base: { flags: T, expect: { tier: "high", shadows: true, ssao: true } },
  shadows_off: { flags: { ...T, shadows: "off" }, expect: { shadows: false } },
  cascades_2: { flags: { ...T, cascades: "2" }, expect: { cascades: 2 } },
  post_none: { flags: { ...T, post: "none" }, expect: { ssao: false } },
  grass_off: { flags: { ...T, grass: "off" }, expect: { grass_blades: 0 } },
  medium: { flags: { ...T, tier: "medium" }, expect: { tier: "medium", ssao: false } },
  low: { flags: { ...T, tier: "low" }, expect: { tier: "low" } },
  s1080: { flags: { ...T }, size: "1920x1080", expect: {} },
};
const pick = (process.env.ARMS ?? Object.keys(ARMS).join(",")).split(",").filter((a) => {
  if (!ARMS[a]) throw new Error(`unknown arm ${a}; known: ${Object.keys(ARMS).join(", ")}`);
  return true;
});

const vsyncLike = (p50) => [16.67, 8.33].some((t) => Math.abs(p50 - t) <= 0.3);
const count = (p) => p.multidraw_sets + p.batchable_bins + p.unbatchable + p.non_mesh;

async function run(name) {
  const arm = ARMS[name];
  const shots = join(REPO, "target/shots");
  await mkdir(shots, { recursive: true });
  const scriptPath = join(shots, "lc-native-script.json");
  const dump = join(shots, `lc-native-${name}.json`);
  await writeFile(scriptPath, JSON.stringify([...world.script, CAMERA, { cmd: "wait", frames: WAIT }, { cmd: "exit" }]));
  await rm(dump, { force: true });
  const args = ["--size", arm.size ?? SIZE, "--dump-state", dump, "--script", scriptPath];
  for (const [k, v] of Object.entries({ ...BASE, ...arm.flags })) args.push(`--${k}`, v);
  const log = [];
  const code = await new Promise((accept, reject) => {
    const child = spawn(BIN, args, { cwd: REPO });
    child.stdout.on("data", (b) => log.push(b.toString()));
    child.stderr.on("data", (b) => log.push(b.toString()));
    const deadline = setTimeout(() => child.kill("SIGKILL"), 420_000);
    child.once("error", reject);
    child.once("close", (c) => { clearTimeout(deadline); accept(c); });
  });
  await writeFile(join(shots, `lc-native-${name}.log`), log.join(""));
  const st = JSON.parse(await readFile(dump, "utf8").catch(() => { throw new Error(`no state dump (exit ${code})`); }));
  const d = st.diag ?? {};
  if (st.play?.tiles?.length !== world.tiles.length) throw new Error(`world has ${st.play?.tiles?.length} tiles`);
  if (st.play?.rising?.length) throw new Error(`${st.play.rising.length} tiles still rising at the read`);
  if ((d.samples ?? 0) < 600) throw new Error(`diag window not full: ${d.samples} samples`);
  const got = { tier: st.tier?.tier, shadows: d.shadows, cascades: d.cascades, ssao: d.ssao, grass_blades: st.grass?.blades ?? null };
  const bad = Object.entries(arm.expect).filter(([k, v]) => JSON.stringify(got[k]) !== JSON.stringify(v));
  if (bad.length) throw new Error(`toggle not applied: ${bad.map(([k, v]) => `${k} ${JSON.stringify(got[k])} != ${JSON.stringify(v)}`).join(", ")}`);
  if (arm.flags.gpu_timing === "on" && !d.gpu?.passes) throw new Error(`no GPU pass timing: ${JSON.stringify(d.gpu)}`);
  const passes = (d.passes ?? []).map((p) => ({ view: p.view, phase: p.phase, draws: count(p) }));
  return { arm: name, args, exit_code: code, p50: d.p50, p95: d.p95, p99: d.p99, mean: d.mean, max: d.max,
    vsync_like: vsyncLike(d.p50), samples: d.samples, ...got, shadow_dist: d.shadow_dist, post: d.post,
    cars: d.cars, boids: d.boids, adapter: d.adapter, visible_tris: d.visible_tris,
    draws_total: passes.reduce((n, p) => n + p.draws, 0), passes,
    kit_draws: d.kit_draws ? { slots: d.kit_draws.slots, total: d.kit_draws.total } : null,
    gpu: d.gpu ? { ...d.gpu, tail_over_25ms: d.gpu.tail_over_25ms?.length ?? null } : null, camera_pos: st.camera?.pos ?? null };
}

const result = { date: new Date().toISOString(), host: hostname(), cpu: cpus()[0]?.model, repo: REPO, size: SIZE, base: BASE,
  camera: CAMERA, world: { generator: "e2e/water-world.mjs waterWorldScript(10)", tiles: world.tiles.length },
  wait_frames: WAIT, load_limit: LOAD_LIMIT, rounds: ROUNDS, rows: [] };
for (let r = 0; r < ROUNDS; r++)
  for (const name of [...pick.slice(r % pick.length), ...pick.slice(0, r % pick.length)]) {
    const label = `native ${name} round ${r}`;
    const g = await guarded(() => run(name).catch((e) => ({ arm: name, error: String(e.message ?? e) })), label);
    result.rows.push({ round: r, ...g.result, load_before: g.load_before, load_after: g.load_after, load_discarded: g.load_discarded });
    const x = result.rows.at(-1);
    const top = x.gpu?.passes ? Object.entries(x.gpu.passes).sort((a, b) => b[1].mean - a[1].mean).slice(0, 4)
      .map(([k, v]) => `${k} ${v.mean}`).join(", ") : "";
    console.log(`${label}: ${x.error ?? `p50 ${x.p50} p95 ${x.p95} mean ${x.mean} draws ${x.draws_total} gpu total ${x.gpu?.total?.mean ?? "-"} [${top}]`}` +
      ` (load ${g.load_before.toFixed(2)} -> ${g.load_after.toFixed(2)})`);
    await writeFile(OUT, `${JSON.stringify(result, null, 2)}\n`);
  }
console.log(`wrote ${OUT}`);
