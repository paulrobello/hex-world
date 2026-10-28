// Native grass arms on the exact 2,066-tile save (docs/perf/grass-cost-2026-10-28/README.md): play, High,
// tilt off, seed 1, fps off, present immediate, --offscreen on (MiniMac's console is locked and the
// binary runs over SSH, so without it the camera draws nothing), 2560x1440, the perf-grass low camera
// [10, 12, 40] looking at the origin, read 1,500 frames after the load (the 600-frame diag ring then
// opens about 900 frames in, after every pack has landed; perf-close-camera-native.mjs gives packs 780). One process per arm, arm order rotated each round, every run inside
// e2e/perf-load.mjs's guard (1-minute load <= 15 before and after).
// Run from the tree root: node docs/perf/grass-cost-2026-10-28/grass-native.mjs <out.json>
//   ROUNDS (default 3), NATIVE_ARMS (default off,sparse,normal,lush), HEXW_BIN (default target/release/hex-world),
//   SAVE (default docs/perf/post-arc001-2026-10-16/local-world.hexworld)
import { spawn } from "node:child_process";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { cpus, hostname } from "node:os";
import { join, resolve } from "node:path";
import { guarded, LOAD_LIMIT } from "../../../e2e/perf-load.mjs";

const OUT = process.argv[2];
if (!OUT) throw new Error("usage: node docs/perf/grass-cost-2026-10-28/grass-native.mjs <out.json>");
const ROOT = process.cwd();
const BIN = resolve(process.env.HEXW_BIN ?? "target/release/hex-world");
const SAVE = resolve(process.env.SAVE ?? "docs/perf/post-arc001-2026-10-16/local-world.hexworld");
const ROUNDS = Number(process.env.ROUNDS ?? 3);
const ARMS = (process.env.NATIVE_ARMS ?? "off,sparse,normal,lush").split(",");
const SHARE = { off: 0, sparse: 0.5, normal: 1, lush: 1.5 };
const WAIT = 1500;
const CAMERA = { cmd: "camera", pos: [10, 12, 40], look_at: [0, 0, 0] };
const BASE = { scenario: "play", tier: "high", tilt: "off", seed: "1", fps: "off", present: "immediate",
  offscreen: "on", size: "2560x1440", diag: "1" };
const count = (p) => p.multidraw_sets + p.batchable_bins + p.unbatchable + p.non_mesh;

async function run(arm) {
  const shots = join(ROOT, "target/shots");
  await mkdir(shots, { recursive: true });
  const scriptPath = join(shots, "grass-native-script.json");
  const dump = join(shots, `grass-native-${arm}.json`);
  await writeFile(scriptPath, JSON.stringify([CAMERA, { cmd: "wait", frames: WAIT }, { cmd: "exit" }]));
  await rm(dump, { force: true });
  const args = ["--load", SAVE, "--dump-state", dump, "--script", scriptPath, "--grass", arm];
  for (const [k, v] of Object.entries(BASE)) args.push(`--${k}`, v);
  const log = [];
  const code = await new Promise((accept, reject) => {
    const child = spawn(BIN, args, { cwd: ROOT });
    child.stdout.on("data", (b) => log.push(b.toString()));
    child.stderr.on("data", (b) => log.push(b.toString()));
    const deadline = setTimeout(() => child.kill("SIGKILL"), 420_000);
    child.once("error", reject);
    child.once("close", (c) => { clearTimeout(deadline); accept(c); });
  });
  await writeFile(join(shots, `grass-native-${arm}.log`), log.join(""));
  const st = JSON.parse(await readFile(dump, "utf8").catch(() => { throw new Error(`no state dump (exit ${code})`); }));
  const d = st.diag ?? {};
  if (st.play?.tiles?.length !== 2066) throw new Error(`world has ${st.play?.tiles?.length} tiles`);
  if (st.play?.rising?.length) throw new Error(`${st.play.rising.length} tiles still rising at the read`);
  if (st.tier?.tier !== "high") throw new Error(`tier ${st.tier?.tier}`);
  if (st.grass?.density !== SHARE[arm]) throw new Error(`grass density ${st.grass?.density}, expected ${SHARE[arm]}`);
  if ((d.samples ?? 0) < 600) throw new Error(`diag window not full: ${d.samples} samples`);
  if (d.window?.not_acquired) throw new Error(`window occluded: ${d.window.not_acquired} of ${d.window.frames} frames`);
  const packs = Object.entries(st.packs ?? {}).filter(([, v]) => v !== "ready").map(([k]) => k);
  return { platform: "native", arm, args, exit_code: code, p50: d.p50, p95: d.p95, p99: d.p99, mean: d.mean, max: d.max,
    dropped_over_25ms: d.dropped_over_25ms, samples: d.samples, window: d.window ?? null, adapter: d.adapter,
    packs_not_ready: packs, draws: (d.passes ?? []).reduce((n, p) => n + count(p), 0),
    kit_draws: d.kit_draws ? { slots: d.kit_draws.slots, total: d.kit_draws.total } : null,
    grass: st.grass && { status: st.grass.status, groups: st.grass.groups, blades: st.grass.blades,
      instances: st.grass.instances, mesh_bytes: st.grass.mesh_bytes },
    frame_cap: st.frame_cap ?? null, camera_pos: st.camera?.pos ?? null };
}

const result = { date: new Date().toISOString(), host: hostname(), cpu: cpus()[0]?.model, bin: BIN, save: SAVE, base: BASE,
  camera: CAMERA, wait_frames: WAIT, load_limit: LOAD_LIMIT, rounds: ROUNDS, arms: ARMS, rows: [] };
for (let r = 0; r < ROUNDS; r++)
  for (const arm of [...ARMS.slice(r % ARMS.length), ...ARMS.slice(0, r % ARMS.length)]) {
    const label = `native grass=${arm} round ${r}`;
    const g = await guarded(() => run(arm).catch((e) => ({ platform: "native", arm, error: String(e.message ?? e) })), label);
    result.rows.push({ round: r, ...g.result, load_before: g.load_before, load_after: g.load_after, load_discarded: g.load_discarded });
    const x = result.rows.at(-1);
    console.log(`${label}: ${x.error ?? `p50 ${x.p50} p95 ${x.p95} mean ${x.mean} max ${x.max} draws ${x.draws} ` +
      `blades ${x.grass?.blades} packs_not_ready ${x.packs_not_ready.length}`} (load ${g.load_before.toFixed(2)} -> ${g.load_after.toFixed(2)})`);
    await writeFile(OUT, `${JSON.stringify(result, null, 2)}\n`);
  }
console.log(`wrote ${OUT}`);
