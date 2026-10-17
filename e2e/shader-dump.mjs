// Scratch: load the page, dump FULL console texts for shader errors.
import { chromium } from 'playwright';

const BASE = process.env.PROBE_BASE ?? 'http://127.0.0.1:8099/';
const browser = await chromium.launch({
  channel: 'chromium',
  headless: true,
  args: ['--use-angle=metal', '--enable-gpu', '--ignore-gpu-blocklist'],
});
const page = await browser.newPage({ viewport: { width: 640, height: 360 } });
const errors = [];
page.on('console', (m) => {
  const t = m.text();
  if (/Invalid ShaderModule|failed to process|module not found|entryPoint/i.test(t)) errors.push(t);
});
await page.goto(BASE, { waitUntil: 'load', timeout: 60_000 });
await page.waitForTimeout(25_000);
const seen = new Set();
for (const e of errors) {
  const key = e.slice(0, 80);
  if (seen.has(key)) continue;
  seen.add(key);
  console.log('=====\n' + e.slice(0, 4000));
}
await browser.close();
