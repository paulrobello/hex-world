// Chrome trace event totals are inclusive and overlap. CPU samples are counted separately.
// Usage: node e2e/perf-trace-summary.mjs <trace.json> <summary.json>
import { readFile, writeFile } from "node:fs/promises";
const [input, output, symbolsPath] = process.argv.slice(2);
const symbolMap = symbolsPath ? JSON.parse(await readFile(symbolsPath, "utf8")) : null;
const symbolize = (name) => {
  const match = /wasm-function\[(\d+)\]/.exec(name ?? "");
  return match && symbolMap?.matched ? symbolMap.names[match[1]] ?? name : name;
};
const raw = JSON.parse(await readFile(input, "utf8"));
const events = raw.traceEvents ?? raw;
const threads = new Map();
for (const e of events) {
  if (e.ph === "M" && e.name === "thread_name") threads.set(`${e.pid}:${e.tid}`, e.args.name);
}
const spans = [];
const stacks = new Map();
for (const e of events) {
  const key = `${e.pid}:${e.tid}`;
  if (e.ph === "X") spans.push(e);
  if (e.ph === "B") {
    const stack = stacks.get(key) ?? [];
    stack.push(e);
    stacks.set(key, stack);
  }
  if (e.ph === "E") {
    const start = stacks.get(key)?.pop();
    if (start) spans.push({ ...start, ph: "X", dur: e.ts - start.ts });
  }
}
const totals = new Map();
for (const e of spans) {
  if (e.ph !== "X" || !e.dur) continue;
  const thread = threads.get(`${e.pid}:${e.tid}`) ?? `${e.pid}:${e.tid}`;
  const key = `${thread}\t${e.name}`;
  const row = totals.get(key) ?? { thread, event: e.name, count: 0, inclusive_ms: 0, max_ms: 0 };
  row.count++;
  row.inclusive_ms += e.dur / 1000;
  row.max_ms = Math.max(row.max_ms, e.dur / 1000);
  totals.set(key, row);
}
const rows = [...totals.values()].sort((a, b) => b.inclusive_ms - a.inclusive_ms);
const nodes = new Map();
const hits = new Map();
for (const e of events) {
  const profile = e.args?.data?.cpuProfile;
  if (!profile) continue;
  const key = `${e.pid}:${e.tid}:${e.id ?? ""}`;
  const map = nodes.get(key) ?? new Map();
  for (const n of profile.nodes ?? []) map.set(n.id, n.callFrame);
  nodes.set(key, map);
  for (const id of profile.samples ?? []) {
    const frame = map.get(id);
    const name = frame?.functionName ?? `unknown:${id}`;
    const label = `${name} (${frame?.url ?? ""})`;
    hits.set(label, (hits.get(label) ?? 0) + 1);
  }
}
const profile = JSON.parse(await readFile(input + ".cpuprofile", "utf8"));
const profileNodes = new Map(profile.nodes.map((n) => [n.id, n]));
const sampled = new Map();
const parents = new Map();
for (const node of profile.nodes) for (const child of node.children ?? []) parents.set(child, node.id);
const inclusive = new Map();
const categories = ["check_dir_light_mesh_visibility", "calculate_bounds", "check_visibility_cpu_culling",
  "queue_material_meshes", "scatter::sync", "extract_meshes_for_gpu_building", "publish_state", "serde_json"];
const categoryMs = new Map(categories.map((name) => [name, 0]));
for (let i = 0; i < (profile.samples ?? []).length; i++) {
  const node = profileNodes.get(profile.samples[i]);
  const name = symbolize(node?.callFrame?.functionName ?? "unknown");
  const row = sampled.get(name) ?? { name, samples: 0, sampled_ms: 0 };
  row.samples++;
  row.sampled_ms += (profile.timeDeltas?.[i] ?? 0) / 1000;
  sampled.set(name, row);
  let ancestor = node;
  const seen = new Set();
  const matchedCategories = new Set();
  while (ancestor && !seen.has(ancestor.id)) {
    seen.add(ancestor.id);
    const label = symbolize(ancestor.callFrame?.functionName ?? "unknown");
    inclusive.set(label, (inclusive.get(label) ?? 0) + (profile.timeDeltas?.[i] ?? 0) / 1000);
    for (const category of categories) if (label.includes(category)) matchedCategories.add(category);
    ancestor = profileNodes.get(parents.get(ancestor.id));
  }
  for (const category of matchedCategories)
    categoryMs.set(category, categoryMs.get(category) + (profile.timeDeltas?.[i] ?? 0) / 1000);
}
const callbacks = spans.filter((e) => e.name === "FireAnimationFrame").map((e) => e.dur / 1000).sort((a, b) => a - b);
const quantile = (p) => callbacks[Math.min(callbacks.length - 1, Math.floor(p * callbacks.length))] ?? null;
const summary = { input, events: events.length, callback_ms: { count: callbacks.length, p50: quantile(0.5), p95: quantile(0.95), max: quantile(1) },
  cpu_profile_duration_ms: (profile.endTime - profile.startTime) / 1000,
  category_ancestry: [...categoryMs].map(([name, sampled_ms]) => ({ name, sampled_ms,
    percent: 100_000 * sampled_ms / (profile.endTime - profile.startTime) })),
  cpu_profile_leaf: [...sampled.values()].sort((a, b) => b.sampled_ms - a.sampled_ms).slice(0, 60),
  cpu_profile_inclusive: [...inclusive].sort((a, b) => b[1] - a[1]).slice(0, 120).map(([name, sampled_ms]) => ({ name, sampled_ms })), notes: [
  "Event totals are inclusive, nested, and overlap. Never sum them as frame time.",
  "GPU-process host events are command encoding/service time, not hardware execution time.",
  "CPU profiler leaf samples name WASM functions only when symbols survive release build.",
], main_thread: rows.filter((r) => /CrRendererMain/.test(r.thread)).slice(0, 35),
  gpu_threads: rows.filter((r) => /gpu/i.test(r.thread + r.event)).slice(0, 35),
  cpu_leaf_samples: [...hits].sort((a, b) => b[1] - a[1]).slice(0, 50).map(([name, count]) => ({ name, count })),
  all_threads: [...threads].map(([id, name]) => ({ id, name })), top_events: rows.slice(0, 35) };
await writeFile(output, JSON.stringify(summary, null, 2) + "\n");
console.log(JSON.stringify(summary, null, 2));
