// CPU load cannot detect another Chromium process consuming the GPU.
import { execFileSync } from "node:child_process";
const browsers = () => execFileSync("ps", ["-Ao", "pid=,ppid=,comm="], { encoding: "utf8" }).split("\n")
  .map((line) => /^\s*(\d+)\s+(\d+)\s+(.+)$/.exec(line)).filter(Boolean)
  .map((m) => ({ pid: Number(m[1]), parent: Number(m[2]), command: m[3] }))
  .filter((p) => /Chrome|Chromium/.test(p.command));
export async function waitForBrowserExclusivity() {
  const deadline = Date.now() + 60 * 60_000;
  for (;;) {
    const active = browsers();
    if (!active.length) return { before: [], checked_at: new Date().toISOString() };
    if (Date.now() > deadline) throw new Error(`GPU exclusivity deadline: ${JSON.stringify(active)}`);
    console.log(`GPU exclusivity: waiting for ${active.length} existing Chrome processes`);
    await new Promise((resolve) => setTimeout(resolve, 15_000));
  }
}
export function monitorBrowserExclusivity() {
  const violations = [];
  const check = () => { try { assertBrowserExclusivity(); } catch (error) { violations.push(String(error)); } };
  check();
  const timer = setInterval(check, 1000);
  timer.unref();
  return () => {
    clearInterval(timer);
    check();
    if (violations.length) throw new Error(`GPU contention during run: ${JSON.stringify(violations.slice(0, 5))}`);
    return { interval_ms: 1000, violations: 0, ...assertBrowserExclusivity() };
  };
}
export function assertBrowserExclusivity() {
  const active = browsers();
  const owned = new Set([process.pid]);
  for (let n = 0; n < active.length; n++) for (const p of active) if (owned.has(p.parent)) owned.add(p.pid);
  const foreign = active.filter((p) => !owned.has(p.pid));
  if (foreign.length) throw new Error(`GPU contention: discard run, foreign Chrome processes ${JSON.stringify(foreign)}`);
  return { after: active, checked_at: new Date().toISOString() };
}
