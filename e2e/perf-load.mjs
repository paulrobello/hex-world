// Host-load guard for every timing run (owner rule): the 1-minute load average must be at or
// below PERF_LOAD_LIMIT (default 15) immediately before AND after a run. Before a run we wait
// (polling every 10 s, up to 10 min); a run whose post-run load is over the limit is discarded
// and re-run (up to 3 retries). Never records a contaminated sample silently: it throws instead.
import { loadavg } from "node:os";

export const LOAD_LIMIT = Number(process.env.PERF_LOAD_LIMIT ?? 15);
const POLL_MS = 10_000;
// PERF_LOAD_WAIT_MIN lengthens the bounded wait on a busy shared host (default 10 min).
const WAIT_MIN = Number(process.env.PERF_LOAD_WAIT_MIN ?? 10);
if (!(WAIT_MIN > 0)) throw new Error(`PERF_LOAD_WAIT_MIN must be a positive number, got ${process.env.PERF_LOAD_WAIT_MIN}`);
const WAIT_MS = WAIT_MIN * 60_000;
const MAX_RETRIES = 3;

const load = () => loadavg()[0];

export async function waitForLoad(label = "run") {
  const deadline = Date.now() + WAIT_MS;
  let l = load();
  while (l > LOAD_LIMIT) {
    if (Date.now() >= deadline)
      throw new Error(`host load guard: ${label} waited ${WAIT_MS / 60_000} min, load ${l.toFixed(2)} > ${LOAD_LIMIT}`);
    console.log(`load ${l.toFixed(2)} > ${LOAD_LIMIT}, waiting before ${label}`);
    await new Promise((r) => setTimeout(r, POLL_MS));
    l = load();
  }
  return l;
}

/** Runs `measure` under the guard; returns { result, load_before, load_after, load_discarded }. */
export async function guarded(measure, label = "run") {
  for (let discarded = 0; ; discarded++) {
    const load_before = await waitForLoad(label);
    const result = await measure();
    const load_after = load();
    if (load_after <= LOAD_LIMIT) return { result, load_before, load_after, load_discarded: discarded };
    console.log(`load after ${label} ${load_after.toFixed(2)} > ${LOAD_LIMIT}: sample discarded`);
    if (discarded + 1 > MAX_RETRIES)
      throw new Error(`host load guard: ${label} discarded ${discarded + 1} samples, post-run load ${load_after.toFixed(2)} > ${LOAD_LIMIT}`);
  }
}
