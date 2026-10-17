// Scratch probe: builds the radius-26 (2,066-tile) play world in GPU Chromium and reports
// whether it completes, plus diag + a screenshot. Not a committed e2e.
import { chromium } from 'playwright';
import { waterWorldScript } from './water-world.mjs';
import { buildWorld } from './build-world.mjs';

const BASE = process.env.PROBE_BASE ?? 'http://127.0.0.1:8099/';
const RADIUS = Number(process.env.PROBE_RADIUS ?? 26);
const OUT = process.env.PROBE_OUT ?? '/private/tmp/claude-501/-Users-probello-Repos-hex-world/c9e84eab-d8d8-48a7-bf71-ab6d274abeaa/scratchpad';

const browser = await chromium.launch({
  channel: 'chromium',
  headless: true,
  args: ['--use-angle=metal', '--enable-gpu', '--ignore-gpu-blocklist'],
});
const page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
page.on('console', (m) => {
  const t = m.text();
  if (/error|panic|oom|abort/i.test(t)) console.log(`CONSOLE ${t.slice(0, 300)}`);
});
page.on('pageerror', (e) => console.log(`PAGEERROR ${String(e).slice(0, 300)}`));

await page.goto(BASE, { waitUntil: 'load', timeout: 60_000 });
await page.waitForFunction(() => window.hexWorld?.state?.()?.frames > 0, null, {
  timeout: 300_000,
  polling: 500,
});

const world = waterWorldScript(RADIUS);
await page.evaluate((s) => window.hexWorld.cmd(s), world.script);
const t0 = Date.now();
let last = -1;
for (;;) {
  await page.waitForTimeout(15_000);
  const s = await page.evaluate(() => {
    const st = window.hexWorld.state();
    return {
      frames: st.frames,
      queued: st.queued,
      tiles: st.play?.tiles?.length ?? 0,
      rising: st.play?.rising?.length ?? -1,
      fps: st.fps,
    };
  });
  console.log(
    `T${Math.round((Date.now() - t0) / 1000)}s ${JSON.stringify(s)}`,
  );
  if (s.queued === 0 && s.rising === 0 && s.tiles === world.tiles.length) {
    console.log('BUILT', JSON.stringify(s));
    break;
  }
  if (Date.now() - t0 > 900_000) throw new Error(`stalled: ${JSON.stringify(s)}`);
}
const diag = await page.evaluate(() => {
  const d = window.hexWorld.diag?.() ?? {};
  const s = window.hexWorld.state();
  return {
    wasm: d.wasm_memory_bytes ?? null,
    tiles: s.play?.tiles?.length,
    queued: s.queued,
  };
});
console.log(`BUILT ${JSON.stringify(diag)}`);
await page.screenshot({ path: `${OUT}/instanced-web.png` });
await browser.close();
console.log('PROBE OK');
