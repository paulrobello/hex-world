// Foreign automated GPU work is forbidden. Owner permits human desktop Chrome only while idle.
import { execFileSync } from "node:child_process";
const browsers = () => execFileSync("ps", ["-Ao", "pid=,ppid=,pcpu=,comm="], { encoding: "utf8" }).split("\n")
  .map((line) => /^\s*(\d+)\s+(\d+)\s+([\d.]+)\s+(.+)$/.exec(line)).filter(Boolean)
  .map((m) => ({ pid: Number(m[1]), parent: Number(m[2]), cpu: Number(m[3]), command: m[4] }))
  .filter((p) => /Chrome|Chromium/.test(p.command) && !/crashpad|ChatGPT for Chrome/.test(p.command));
export function assertBrowserExclusivity() {
  const active = browsers();
  const owned = new Set([process.pid]);
  for (let n = 0; n < active.length; n++) for (const p of active) if (owned.has(p.parent)) owned.add(p.pid);
  const foreign = active.filter((p) => !owned.has(p.pid));
  const automated = foreign.filter((p) => !p.command.startsWith("/Applications/Google Chrome.app/"));
  if (automated.length) throw new Error(`GPU contention: foreign automated Chrome ${JSON.stringify(automated)}`);
  const human_cpu_percent = foreign.reduce((sum, p) => sum + p.cpu, 0);
  if (human_cpu_percent >= 20) throw new Error(`human desktop Chrome active: summed CPU ${human_cpu_percent}% >=20%`);
  return { after: active, human_cpu_percent, human_desktop_chrome_present: foreign.length > 0,
    condition: "human desktop Chrome present but idle; no automated GPU contention", checked_at: new Date().toISOString() };
}
export async function waitForBrowserExclusivity() {
  const deadline = Date.now() + 60 * 60_000;
  for (;;) {
    try { return assertBrowserExclusivity(); } catch (error) {
      if (Date.now() > deadline) throw new Error(`GPU exclusivity deadline: ${String(error)}`);
      console.log(`GPU exclusivity: waiting (${String(error).slice(0, 180)})`);
      await new Promise((resolve) => setTimeout(resolve, 15_000));
    }
  }
}
export function monitorBrowserExclusivity() {
  const violations = [];
  let peakHumanCpu = 0, samples = 0;
  const check = () => {
    samples++;
    try { peakHumanCpu = Math.max(peakHumanCpu, assertBrowserExclusivity().human_cpu_percent); }
    catch (error) { violations.push(String(error)); }
  };
  check();
  const timer = setInterval(check, 1000);
  timer.unref();
  return () => {
    clearInterval(timer);
    check();
    if (violations.length) throw new Error(`GPU contention during run: ${JSON.stringify(violations.slice(0, 5))}`);
    return { interval_ms: 1000, samples, peak_human_cpu_percent: peakHumanCpu, violations: 0, ...assertBrowserExclusivity() };
  };
}
