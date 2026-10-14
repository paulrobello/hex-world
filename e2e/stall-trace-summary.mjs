// Summarizes a Chrome trace from e2e/stall-probe.mjs (PROBE_TRACE): finds the page's animation
// frames on the renderer main thread, marks frame intervals over 25 ms as stalls, and lists what
// every renderer and GPU-process thread was doing inside the stalls next to the normal frames.
// With a third path it also writes a small Perfetto-loadable excerpt: the metadata plus the page's
// main thread and the GPU process's CrGpuMain and VizCompositorThread spans of 100 µs or more, from
// 20 ms before the longest stall to 20 ms after it.
// usage: node e2e/stall-trace-summary.mjs <trace.json> <summary.json> [excerpt.json]
import { readFile, writeFile } from "node:fs/promises";

const [tracePath, outPath, excerptPath] = process.argv.slice(2);
const raw = JSON.parse(await readFile(tracePath, "utf8"));
const ev = Array.isArray(raw) ? raw : raw.traceEvents;

const threadName = new Map(), procName = new Map();
for (const e of ev) {
  if (e.ph !== "M") continue;
  if (e.name === "thread_name") threadName.set(`${e.pid}:${e.tid}`, e.args.name);
  if (e.name === "process_name") procName.set(e.pid, e.args.name);
}
const label = (e) => `${procName.get(e.pid) ?? e.pid}/${threadName.get(`${e.pid}:${e.tid}`) ?? e.tid}`;

// Complete events; B/E pairs folded into X so durations are comparable.
const spans = [];
const open = new Map();
for (const e of ev) {
  if (e.ph === "X" && typeof e.dur === "number") spans.push(e);
  else if (e.ph === "B") { const k = `${e.pid}:${e.tid}`; (open.get(k) ?? open.set(k, []).get(k)).push(e); }
  else if (e.ph === "E") {
    const st = open.get(`${e.pid}:${e.tid}`)?.pop();
    if (st) spans.push({ ...st, ph: "X", dur: e.ts - st.ts });
  }
}

// The page's main thread is the renderer thread with the most animation frames.
const fires = spans.filter((e) => e.name === "FireAnimationFrame");
const byThread = new Map();
for (const e of fires) byThread.set(`${e.pid}:${e.tid}`, (byThread.get(`${e.pid}:${e.tid}`) ?? 0) + 1);
const mainKey = [...byThread].sort((a, b) => b[1] - a[1])[0]?.[0];
if (!mainKey) throw new Error("no FireAnimationFrame events in the trace");
const frames = fires.filter((e) => `${e.pid}:${e.tid}` === mainKey).sort((a, b) => a.ts - b.ts);
const mainPid = Number(mainKey.split(":")[0]);
const gpuPids = new Set([...procName].filter(([, n]) => /gpu/i.test(n)).map(([p]) => p));

const intervals = frames.slice(1).map((f, i) => ({ start: frames[i].ts, end: f.ts, ms: (f.ts - frames[i].ts) / 1000,
  callback_ms: frames[i].dur / 1000 }));
const stalls = intervals.filter((x) => x.ms > 25);
const normal = intervals.filter((x) => x.ms <= 25);
const q = (v, p) => { const s = [...v].sort((a, b) => a - b); return s.length ? s[Math.min(s.length - 1, Math.floor(p * s.length))] : null; };

// Time each (thread, event name) spends inside a set of windows, clipped, per window.
const relevant = spans.filter((e) => e.pid === mainPid || gpuPids.has(e.pid));
function census(wins) {
  const acc = new Map();
  for (const w of wins)
    for (const e of relevant) {
      const a = Math.max(e.ts, w.start), b = Math.min(e.ts + e.dur, w.end);
      if (b <= a) continue;
      const k = `${label(e)} :: ${e.name}`;
      acc.set(k, (acc.get(k) ?? 0) + (b - a) / 1000);
    }
  return new Map([...acc].map(([k, v]) => [k, v / Math.max(1, wins.length)]));
}
const inStall = census(stalls), inNormal = census(normal);
const top = [...inStall].map(([k, v]) => ({ event: k, stall_ms_per_frame: +v.toFixed(2),
  normal_ms_per_frame: +(inNormal.get(k) ?? 0).toFixed(2) }))
  .filter((x) => x.stall_ms_per_frame >= 1).sort((a, b) => (b.stall_ms_per_frame - b.normal_ms_per_frame) - (a.stall_ms_per_frame - a.normal_ms_per_frame))
  .slice(0, 40);

const gc = spans.filter((e) => e.pid === mainPid && /GC|Scavenge|MarkCompact|MinorMS/.test(e.name));
const gcInStall = gc.filter((e) => stalls.some((s) => e.ts < s.end && e.ts + e.dur > s.start));
const grow = spans.filter((e) => /grow/i.test(e.name));

const out = {
  trace: tracePath, events: ev.length, main_thread: label(frames[0]), gpu_processes: [...gpuPids].map((p) => procName.get(p)),
  window_ms: (frames.at(-1).ts - frames[0].ts) / 1000, animation_frames: frames.length,
  interval_ms: { p50: q(intervals.map((x) => x.ms), 0.5), p95: q(intervals.map((x) => x.ms), 0.95), max: q(intervals.map((x) => x.ms), 1) },
  callback_ms: { p50: q(intervals.map((x) => x.callback_ms), 0.5), max: q(intervals.map((x) => x.callback_ms), 1) },
  stalls: stalls.length,
  stall_detail: stalls.map((s) => ({ at_ms: +((s.start - frames[0].ts) / 1000).toFixed(1), interval_ms: +s.ms.toFixed(1),
    callback_ms: +s.callback_ms.toFixed(1) })),
  stall_spacing_frames: stalls.slice(1).map((s, i) => intervals.indexOf(s) - intervals.indexOf(stalls[i])),
  gc: { events: gc.length, in_stalls: gcInStall.length, total_ms: +(gc.reduce((a, e) => a + e.dur, 0) / 1000).toFixed(2),
    distinct_names: new Set(gc.map((e) => e.name)).size },
  memory_grow_events: grow.map((e) => ({ name: e.name, thread: label(e), ms: e.dur / 1000 })),
  top_in_stalls_vs_normal: top,
};
if (excerptPath && stalls.length) {
  const worst = stalls.reduce((a, b) => (b.ms > a.ms ? b : a));
  const [a, b] = [worst.start - 20_000, worst.end + 20_000];
  const keep = (e) => {
    const t = label(e);
    return (`${e.pid}:${e.tid}` === mainKey || (gpuPids.has(e.pid) && /CrGpuMain|VizCompositorThread/.test(t)));
  };
  // Spans of 100 µs or more only: the per-command WebGPU events would put it at tens of MB.
  const picked = [...ev.filter((e) => e.ph === "M"),
    ...spans.filter((e) => keep(e) && e.dur >= 100 && e.ts + e.dur >= a && e.ts <= b)];
  await writeFile(excerptPath, "[\n" + picked.map((e) => JSON.stringify(e)).join(",\n") + "\n]\n");
  out.excerpt = { path: excerptPath, events: picked.length, stall_interval_ms: +worst.ms.toFixed(1),
    window_us: [a, b] };
}
await writeFile(outPath, JSON.stringify(out, null, 2) + "\n");
console.log(JSON.stringify({ ...out, top_in_stalls_vs_normal: top.slice(0, 15) }, null, 2));
