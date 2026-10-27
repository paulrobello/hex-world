// Summarizes the sessions of scripts/perf/tier-change-rounds.sh: per arm, cache mode and step, the
// median [min, max] over rounds of the stall, the longest frame and the pipelines the change created,
// plus READY and the warm-up's own stall, with every load read.
// usage: node scripts/perf/tier-change-summary.mjs <results dir> <out.json>
import { readdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

const dir = process.argv[2] ?? "results";
const OUT = process.argv[3] ?? "summary.json";
const sessions = [];
for (const f of (await readdir(dir)).filter((f) => f.endsWith(".json")).sort()) {
  const m = f.match(/^(.+)-(cold|warm)-r(\d+)\.json$/);
  if (!m) continue;
  sessions.push({ arm: m[1], mode: m[2], round: Number(m[3]), data: JSON.parse(await readFile(join(dir, f), "utf8")) });
}
const stat = (v) => {
  const s = v.filter((x) => x != null).sort((a, b) => a - b);
  return s.length ? { median: s[Math.floor(s.length / 2)], min: s[0], max: s.at(-1), n: s.length } : null;
};
const groups = {};
for (const s of sessions) {
  const g = (groups[`${s.arm} ${s.mode}`] ??= { arm: s.arm, mode: s.mode, rounds: [], steps: {}, ready_ms: [], warmup: [], loads: [] });
  g.rounds.push(s.round);
  for (const w of s.data.warmups ?? []) {
    g.ready_ms.push(w.ready_ms);
    if (w.warmup) g.warmup.push(w.warmup);
  }
  for (const r of s.data.rows) {
    const k = `#${r.index + 1} ${r.step}`;
    const st = (g.steps[k] ??= { stall_ms: [], longest_frame_ms: [], slow_frames: [], pipelines_added: [], rebuild_settled_ms: [] });
    for (const f of Object.keys(st)) st[f].push(r[f]);
    g.loads.push(r.load_before, r.load_after);
  }
}
const out = Object.values(groups).map((g) => ({
  arm: g.arm,
  mode: g.mode,
  rounds: g.rounds.sort(),
  ready_ms: stat(g.ready_ms),
  steps: Object.fromEntries(Object.entries(g.steps).map(([k, st]) => [k, Object.fromEntries(Object.entries(st).map(([f, v]) => [f, stat(v)]))])),
  warmup: g.warmup.length ? {
    longest_frame_ms: stat(g.warmup.map((w) => w.longest_frame_ms)),
    settled_longest_frame_ms: stat(g.warmup.map((w) => w.settled_longest_frame_ms)),
    ready_longest_frame_ms: stat(g.warmup.map((w) => w.ready_longest_frame_ms)),
    runs: stat(g.warmup.map((w) => w.runs)),
    added: stat(g.warmup.map((w) => w.added)),
    frames: stat(g.warmup.map((w) => w.frames)),
    loads: g.warmup.flatMap((w) => [w.load_before, w.load_after]),
    all_load_ok: g.warmup.every((w) => w.load_ok),
  } : null,
  load_range: [Math.min(...g.loads), Math.max(...g.loads)],
}));
await writeFile(OUT, JSON.stringify(out, null, 2) + "\n");
for (const g of out) {
  console.log(`${g.arm} ${g.mode} (rounds ${g.rounds.join(",")}, loads ${g.load_range.map((x) => x.toFixed(2)).join("-")}): READY ${g.ready_ms?.median} ms`);
  for (const [k, st] of Object.entries(g.steps))
    console.log(`  ${k}: stall ${st.stall_ms?.median} [${st.stall_ms?.min}, ${st.stall_ms?.max}] ms, longest ${st.longest_frame_ms?.median} ms, pipelines +${st.pipelines_added?.median ?? "-"}`);
  if (g.warmup) console.log(`  warm-up: longest frame ${g.warmup.longest_frame_ms?.median} [${g.warmup.longest_frame_ms?.min}, ${g.warmup.longest_frame_ms?.max}] ms (right after READY ${g.warmup.ready_longest_frame_ms?.median} ms, after the world settled ${g.warmup.settled_longest_frame_ms?.median} ms), ${g.warmup.runs?.median} runs`);
}
