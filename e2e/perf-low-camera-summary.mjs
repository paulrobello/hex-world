// Merges the low-camera rounds (scripts/perf/low-camera-minimac.sh: web-<n>.json from
// e2e/frame-pacing.mjs lc_* arms, native-<n>.json from e2e/perf-low-camera-native.mjs) into one
// results file with per-arm medians and paired deltas against the base arm of the same round.
// usage: node e2e/perf-low-camera-summary.mjs <results-dir> <out.json>
import { readdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

const [dir, OUT] = process.argv.slice(2);
if (!OUT) throw new Error("usage: node e2e/perf-low-camera-summary.mjs <results-dir> <out.json>");
const r2 = (x) => (x == null ? null : Math.round(x * 100) / 100);
const median = (v) => { const s = v.filter((x) => x != null).sort((a, b) => a - b); return s.length ? r2(s[Math.floor((s.length - 1) / 2)] / 2 + s[Math.ceil((s.length - 1) / 2)] / 2) : null; };
const spread = (v) => { const s = v.filter((x) => x != null); return s.length ? { median: median(s), min: r2(Math.min(...s)), max: r2(Math.max(...s)), n: s.length } : null; };
const mean = (v) => v.reduce((a, b) => a + b, 0) / v.length;

const files = (await readdir(dir)).filter((f) => /^(web|native)-\d+\.json$/.test(f)).sort();
const web = [], native = [];
for (const f of files) {
  const round = Number(f.match(/-(\d+)\.json$/)[1]);
  const j = JSON.parse(await readFile(join(dir, f), "utf8"));
  for (const row of j.rows) (f.startsWith("web") ? web : native).push({ round, ...row });
}

// Web: one row per arm and round; its two closed windows are averaged.
const webRow = (row) => {
  if (row.error) return { arm: row.arm, round: row.round, error: row.error, load_before: row.load_before, load_after: row.load_after };
  const w = row.phases;
  const avg = (f) => r2(mean(w.map(f)));
  return { arm: row.arm, round: row.round, canvas: row.canvas, submit_mean: avg((p) => p.submit.mean), submit_p50: avg((p) => p.submit.p50),
    submit_p95: avg((p) => p.submit.p95), cpu_mean: avg((p) => p.cpu.mean), cpu_p95: avg((p) => p.cpu.p95),
    engine_frames_5s: avg((p) => p.engine_frames), latency_p50: avg((p) => p.latency?.p50 ?? 0),
    gpu_total: row.census?.gpu?.total?.mean ?? null, draws: row.census?.draws_total, kit_draws: row.census?.kit_draws?.total,
    grass_blades: row.census?.grass_blades, load_before: r2(row.load_before), load_after: r2(row.load_after) };
};
const nativeRow = (row) => row.error ? { arm: row.arm, round: row.round, error: row.error, load_before: row.load_before, load_after: row.load_after }
  : { arm: row.arm, round: row.round, p50: row.p50, p95: row.p95, mean: row.mean, vsync_like: row.vsync_like, draws: row.draws_total,
    kit_draws: row.kit_draws?.total, gpu_total: row.gpu?.total?.mean ?? null, gpu_total_p95: row.gpu?.total?.p95 ?? null,
    passes: row.gpu?.passes ? Object.fromEntries(Object.entries(row.gpu.passes).map(([k, v]) => [k, v.mean])) : null,
    rejected: row.gpu?.rejected_non_monotonic ?? null, load_before: r2(row.load_before), load_after: r2(row.load_after) };

function summarize(rows, base, fields) {
  const arms = [...new Set(rows.map((r) => r.arm))];
  const out = {};
  for (const arm of arms) {
    const mine = rows.filter((r) => r.arm === arm && !r.error);
    const s = { runs: mine.length, errors: rows.filter((r) => r.arm === arm && r.error).length };
    for (const f of fields) {
      s[f] = spread(mine.map((r) => r[f]));
      const deltas = mine.map((r) => { const b = rows.find((x) => x.arm === base && x.round === r.round && !x.error); return b && r[f] != null && b[f] != null ? r[f] - b[f] : null; });
      if (arm !== base) s[`${f}_delta`] = spread(deltas);
    }
    if (mine[0]?.passes) {
      s.passes = {};
      for (const k of Object.keys(mine[0].passes)) {
        s.passes[k] = spread(mine.map((r) => r.passes?.[k]));
        if (arm !== base) s.passes[`${k}_delta`] = spread(mine.map((r) => { const b = rows.find((x) => x.arm === base && x.round === r.round && !x.error); return b?.passes ? r.passes[k] - b.passes[k] : null; }));
      }
    }
    out[arm] = s;
  }
  return out;
}

const W = web.map(webRow), N = native.map(nativeRow);
const loads = [...W, ...N].flatMap((r) => [r.load_before, r.load_after]).filter((x) => x != null);
const result = { generated: new Date().toISOString(), sources: files, load: { min: r2(Math.min(...loads)), max: r2(Math.max(...loads)) },
  web: { summary: summarize(W, "lc_base", ["submit_mean", "submit_p50", "submit_p95", "cpu_mean", "cpu_p95", "engine_frames_5s", "gpu_total", "draws"]), rows: W },
  native: { summary: summarize(N, "base", ["gpu_total", "gpu_total_p95", "p50", "p95", "mean", "draws"]), rows: N } };
await writeFile(OUT, `${JSON.stringify(result, null, 2)}\n`);
console.log(`wrote ${OUT}: ${W.length} web rows, ${N.length} native rows, load ${result.load.min}-${result.load.max}`);
