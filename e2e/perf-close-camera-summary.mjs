// Paired deltas of an e2e/perf-close-camera.mjs result against one arm (docs/perf/large-world-o4o5/):
// per arm, world and camera, the median [min, max] over the rounds of each statistic, and of each
// round's delta against the reference arm in the same round. `real` follows the close-camera-draws
// README's rule: every paired delta has the same sign and the median is larger than their spread.
// usage: node e2e/perf-close-camera-summary.mjs <result.json> [<reference arm>] [<out.json>]
import { readFile, writeFile } from "node:fs/promises";

const [IN, REF = "base", OUT] = process.argv.slice(2);
const res = JSON.parse(await readFile(IN, "utf8"));
const rows = res.rows.filter((r) => !r.error && r.cameras);
const STATS = { mean: (c) => c.mean, p95: (c) => c.p95, p50: (c) => c.p50, cpu_mean: (c) => c.cpu?.mean, cpu_p95: (c) => c.cpu?.p95,
  over_25ms: (c) => c.dropped_over_25ms, draws: (c) => c.draws, kit_draws: (c) => c.kit_draws?.total };
const r2 = (x) => Math.round(x * 100) / 100;
const med = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length % 2 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
};
const spread = (xs) => xs.length ? { median: r2(med(xs)), min: r2(Math.min(...xs)), max: r2(Math.max(...xs)), n: xs.length } : null;
const at = (arm, world, round) => rows.find((r) => r.build === arm && r.world === world && r.round === round);
const arms = [...new Set(rows.map((r) => r.build))];
const worlds = [...new Set(rows.map((r) => r.world))];
const out = { source: IN, reference: REF, loads: { min: r2(Math.min(...rows.flatMap((r) => [r.load_before, r.load_after]))),
  max: r2(Math.max(...rows.flatMap((r) => [r.load_before, r.load_after]))) },
  discarded: rows.reduce((n, r) => n + (r.load_discarded ?? 0), 0), failed: res.rows.filter((r) => r.error).length, cells: [] };
for (const world of worlds)
  for (const camera of Object.keys(rows.find((r) => r.world === world).cameras))
    for (const arm of arms) {
      const mine = rows.filter((r) => r.build === arm && r.world === world);
      const cell = { world, camera, arm, rounds: mine.length, stats: {}, delta: {} };
      for (const [k, f] of Object.entries(STATS)) {
        cell.stats[k] = spread(mine.map((r) => f(r.cameras[camera])).filter((x) => x != null));
        if (arm === REF) continue;
        const d = mine.map((r) => {
          const ref = at(REF, world, r.round);
          const a = f(r.cameras[camera]);
          const b = ref && f(ref.cameras[camera]);
          return a != null && b != null ? a - b : null;
        }).filter((x) => x != null);
        const s = spread(d);
        if (s) s.real = d.length > 1 && (d.every((x) => x > 0) || d.every((x) => x < 0)) && Math.abs(s.median) > s.max - s.min;
        cell.delta[k] = s;
      }
      out.cells.push(cell);
    }
for (const c of out.cells) {
  const f = (k) => {
    const s = c.stats[k];
    const d = c.delta[k];
    return `${k} ${s?.median}${d ? ` (${d.median >= 0 ? "+" : ""}${d.median} [${d.min}, ${d.max}]${d.real ? " REAL" : ""})` : ""}`;
  };
  console.log(`${c.world} ${c.camera} ${c.arm}: ${["mean", "p95", "cpu_mean", "cpu_p95", "over_25ms", "draws", "kit_draws"].map(f).join(" | ")}`);
}
console.log(`loads ${out.loads.min}-${out.loads.max}, discarded ${out.discarded}, failed ${out.failed}`);
if (OUT) await writeFile(OUT, `${JSON.stringify(out, null, 2)}\n`);
