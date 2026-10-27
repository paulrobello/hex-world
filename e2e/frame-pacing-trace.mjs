// Splits a frame-pacing trace (PACING_TRACE in e2e/frame-pacing.mjs) into the intervals between
// engine frames (renderer FireAnimationFrame over 3 ms), short and long around the median, and says
// what each thread did after the frame's callback: renderer main busy, GPU main busy and, inside
// that, CALayerTreeCoordinator::ApplyBackpressure::Metal (the GPU process waiting on the GPU).
// usage: node e2e/frame-pacing-trace.mjs <trace.json>
import { readFile } from "node:fs/promises";

const ev = JSON.parse(await readFile(process.argv[2], "utf8"));
const names = new Map();
for (const e of ev) if (e.ph === "M" && e.name === "thread_name") names.set(`${e.pid}:${e.tid}`, e.args.name);
const on = (t) => ev.filter((e) => e.ph === "X" && names.get(`${e.pid}:${e.tid}`) === t);
const rm = on("CrRendererMain"), gm = on("CrGpuMain");
const byTs = (a, b) => a.ts - b.ts;
const frames = rm.filter((e) => e.name === "FireAnimationFrame" && e.dur > 3000).sort(byTs);
const top = (v) => v.filter((e) => e.name === "ThreadControllerImpl::RunTask");
const rTop = top(rm), gTop = top(gm);
const back = gm.filter((e) => e.name === "CALayerTreeCoordinator::ApplyBackpressure::Metal");
const busy = (v, a, b) => v.reduce((s, e) => s + Math.max(0, Math.min(b, e.ts + e.dur) - Math.max(a, e.ts)), 0) / 1000;
const gaps = frames.slice(0, -1).map((f, i) => {
  const a = f.ts + f.dur, b = frames[i + 1].ts;
  return { interval: (b - f.ts) / 1000, after: (b - a) / 1000, renderer_busy: busy(rTop, a, b),
    gpu_busy: busy(gTop, a, b), backpressure: busy(back, a, b), callback: f.dur / 1000 };
});
const median = [...gaps].map((g) => g.interval).sort((x, y) => x - y)[Math.floor(gaps.length / 2)];
const mean = (v, k) => Math.round((v.reduce((s, g) => s + g[k], 0) / v.length) * 100) / 100;
const out = {};
for (const [kind, v] of [["short", gaps.filter((g) => g.interval < median)], ["long", gaps.filter((g) => g.interval >= median)]])
  out[kind] = { n: v.length, ...Object.fromEntries(Object.keys(gaps[0]).map((k) => [k + "_ms", mean(v, k)])) };
console.log(JSON.stringify(out, null, 2));
