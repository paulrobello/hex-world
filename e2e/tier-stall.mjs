// Tier-change stall (owner report 2026-10-14: "changing detail levels stalls until the new resources
// are swapped in"). GPU Chromium at 60 Hz on the play scenario: optionally builds the 208-tile water
// world (e2e/water-world.mjs), then for each step sends `set_tier` (or, with TIER_VIA=ui, clicks the
// Settings Quality row) and records every animation frame for TIER_WINDOW_MS. Reports per step the
// stall (the send to the end of the last frame over 25 ms), the longest frame, the slow frames, when
// `state().tier` followed and reported the rebuild drawn, when the quality-change layer showed and
// cleared, the asset requests the change made, and the engine's stall tail. Each step runs inside the
// host-load guard unless TIER_LOAD_GUARD=0. With TIER_TRACE=<file> the first step also records a
// Chrome trace (summarize it with e2e/stall-trace-summary.mjs). Each round also reports READY (the
// navigation to the first frame `ready()` holds) and, on a build with the pipeline warm-up
// (`state().tier.warm`), the warm-up's own stall after the world settles: the frames from its start
// to TIER_WINDOW_MS after it ends, analysed like a step. Every round launches a fresh browser profile; a cold
// Metal shader cache also needs the test browser's system cache moved aside between rounds
// (docs/checkpoints/tier-change/README.md). Local only.
// usage: node e2e/tier-stall.mjs <dist> <out.json>
//   TIER_STEPS=high>medium,medium>high,high>medium (default), TIER_WORLD=<radius> (default 8; 0 = the
//   start tile), TIER_ROUNDS (default 2), TIER_WINDOW_MS (default 6000), TIER_VIA=cmd|ui (default cmd),
//   TIER_QUERY=<extra query keys, e.g. tier_warm=off>
import { chromium } from "playwright";
import { createServer } from "node:http";
import { open, readFile, writeFile } from "node:fs/promises";
import { loadavg } from "node:os";
import { extname, join } from "node:path";
import { LOAD_LIMIT, guarded, waitForLoad } from "./perf-load.mjs";
import { waterWorldScript } from "./water-world.mjs";

const dist = process.argv[2] ?? "dist";
const OUT = process.argv[3] ?? "target/tier-stall.json";
const STEPS = (process.env.TIER_STEPS ?? "high>medium,medium>high,high>medium").split(",").map((s) => s.split(">"));
const RADIUS = Number(process.env.TIER_WORLD ?? 8);
const ROUNDS = Number(process.env.TIER_ROUNDS ?? 2);
const WINDOW_MS = Number(process.env.TIER_WINDOW_MS ?? 6000);
const GUARD = process.env.TIER_LOAD_GUARD !== "0";
const VIA = process.env.TIER_VIA ?? "cmd";
const QUERY = process.env.TIER_QUERY ? `&${process.env.TIER_QUERY}` : "";
const SLOW_MS = 25;

const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css",
  ".ktx2": "image/ktx2", ".glb": "model/gltf-binary", ".png": "image/png", ".webp": "image/webp",
  ".jpg": "image/jpeg", ".woff2": "font/woff2", ".webm": "audio/webm", ".m4a": "audio/mp4", ".json": "application/json" };
const server = createServer(async (req, res) => {
  let path = decodeURIComponent(new URL(req.url, "http://x").pathname);
  if (path.endsWith("/")) path += "index.html";
  try {
    const body = await readFile(join(dist, path));
    res.writeHead(200, { "content-type": types[extname(path)] ?? "application/octet-stream" });
    res.end(body);
  } catch { res.writeHead(404); res.end(); }
}).listen(0);
const base = `http://127.0.0.1:${server.address().port}/`;

const TRACE_CATS = ["devtools.timeline", "v8", "v8.execute", "v8.wasm", "viz", "gpu", "gpu.dawn", "gpu.service", "webgpu",
  "disabled-by-default-webgpu", "toplevel", "blink"];

/** Records every animation frame's timestamp, the tier the state reports, and the overlay's busy flag. */
const RECORDER = () => {
  const rec = { t: [], tier: [], busy: [], warm: [], pipes: [], on: true };
  window.__tierRec = rec;
  const step = (ts) => {
    if (!rec.on) return;
    rec.t.push(performance.now());
    const t = window.hexWorld?.state()?.tier;
    rec.tier.push(t ? `${t.tier}|${t.rebuilding ?? "-"}` : "");
    rec.warm.push(!!t?.warm?.active);
    rec.pipes.push(t?.warm?.pipelines ?? null);
    rec.busy.push(!!document.querySelector(".rebuild:not([hidden])"));
    requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
};

function analyse(rec, t0, target) {
  const frames = rec.t.map((t, i) => ({ t, tier: rec.tier[i], busy: rec.busy[i], pipes: rec.pipes?.[i] ?? null }))
    .filter((f) => f.t >= t0 - 100);
  const after = frames.filter((f) => f.t >= t0);
  const intervals = after.slice(1).map((f, i) => ({ end: f.t, ms: f.t - after[i].t }));
  const first = after[0];
  const before = frames.filter((f) => f.t < t0).at(-1);
  const pre = first ? first.t - t0 : 0; // the send to the first frame after it
  const slow = intervals.filter((x) => x.ms > SLOW_MS);
  const lastSlow = slow.at(-1);
  const stallEnd = lastSlow ? lastSlow.end : (after[0]?.t ?? t0);
  const followed = after.find((f) => f.tier.startsWith(`${target}|`));
  const settled = after.find((f) => f.tier === `${target}|false`);
  const busyOn = after.find((f) => f.busy);
  const busyOff = busyOn ? after.find((f) => f.t > busyOn.t && !f.busy) : null;
  const r = (v) => (v == null ? null : Math.round(v * 10) / 10);
  return {
    stall_ms: r(stallEnd - t0),
    first_frame_ms: r(pre),
    longest_frame_ms: r(Math.max(pre, ...intervals.map((x) => x.ms))),
    slow_frames: slow.length,
    over_50ms: slow.filter((x) => x.ms > 50).length,
    slow_frame_ms: slow.map((x) => r(x.ms)),
    slow_sum_ms: r(slow.reduce((a, x) => a + x.ms, 0)),
    frames_in_window: after.length,
    tier_followed_ms: r(followed && followed.t - t0),
    rebuild_settled_ms: r(settled && settled.t - t0),
    // Positive: the engine reported the rebuild drawn (and the layer cleared) after the last slow frame.
    settled_after_stall_ms: r(settled && settled.t - stallEnd),
    indicator_shown_ms: r(busyOn && busyOn.t - t0),
    indicator_hidden_ms: r(busyOff && busyOff.t - t0),
    indicator_hidden_after_stall_ms: r(busyOff && busyOff.t - stallEnd),
    // Render pipelines the change created by the time it was reported drawn (null without the warm-up's
    // `tier.warm`): later ones belong to the warm-up that follows once play is idle again.
    pipelines_added: settled && before && settled.pipes != null && before.pipes != null ? settled.pipes - before.pipes : null,
  };
}

/** Waits for the world (at least `tiles` tiles, so a script still queued counts as unsettled) to settle. */
async function waitSettled(page, what, tiles = 0, warm = true) {
  await page.waitForFunction(([n, warm]) => {
    const s = window.hexWorld.state();
    const p = s.play;
    return s.tiles >= n && s.queued === 0 && p && p.rising.length === 0 && (s.pack_waiting ?? []).length === 0
      && !s.tier?.rebuilding && (!warm || !s.tier?.warm?.active && (!s.tier?.warm?.enabled || s.tier.warm.current))
      && Object.values(s.packs ?? {}).every((v) => v !== "loading");
  }, [tiles, warm], { timeout: 180_000, polling: 250 }).catch(() => console.log(`warn: ${what} did not settle in 180 s`));
}

async function round(n, tracePath) {
  // The session's load guard comes before the page: the first warm-up starts right after READY.
  const loadBefore = GUARD ? await waitForLoad("the session") : loadavg()[0];
  const browser = await chromium.launch({ channel: "chromium", headless: true,
    args: ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu"] });
  const results = [];
  try {
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 }, deviceScaleFactor: 1 });
    const errors = [];
    page.on("pageerror", (e) => errors.push(String(e)));
    page.on("console", (m) => { if (m.type() === "error") errors.push(m.text()); });
    const requests = [];
    page.on("request", (r) => requests.push({ t: Date.now(), url: r.url().replace(base, "") }));
    await page.goto(`${base}?scenario=play&tilt=off&fps=off&tier=${STEPS[0][0]}${QUERY}`);
    await page.waitForFunction(() => window.hexWorld?.ready(), null, { timeout: 180_000, polling: "raf" });
    const ready_ms = Math.round(await page.evaluate(() => performance.now()));
    // Recording from READY catches the first warm-up whole.
    await page.evaluate(RECORDER);
    const renderer = await page.evaluate(async () => {
      const a = await navigator.gpu?.requestAdapter();
      return a?.info ? `${a.info.vendor} ${a.info.architecture} ${a.info.description}`.trim() : null;
    });
    let tiles = 1;
    const worldAt = await page.evaluate(() => performance.now());
    if (RADIUS > 0) {
      const { script, tiles: t } = waterWorldScript(RADIUS);
      tiles = t.length;
      // Without the script's closing `wait`: it would hold the first step's `set_tier` behind it.
      await page.evaluate((s) => window.hexWorld.cmd(s), script.filter((c) => c.cmd !== "wait"));
    }
    await waitSettled(page, "the world", tiles, false);
    const settledAt = await page.evaluate(() => performance.now());
    // The warm-up starts once the settled world has been idle for a second; wait until it covers it.
    const warmed = await page.waitForFunction(() => {
      const w = window.hexWorld.state().tier?.warm;
      return !w || !w.enabled || (w.current && !w.active);
    }, null, { timeout: 120_000, polling: 100 }).then(() => true, () => false);
    await page.waitForTimeout(WINDOW_MS);
    const warm = await page.evaluate(() => {
      const rec = window.__tierRec;
      rec.on = false;
      return { rec, w: window.hexWorld.state().tier?.warm ?? null };
    });
    let warmup = warm.w;
    if (warm.w?.enabled) {
      // Each warm-up (a run of frames whose state reported a warm camera), up to 2 s after it ended:
      // its longest frame and the time in frames over 25 ms. Some run while the world is still
      // being built, so their frames also carry tile placement.
      const { t, warm: on } = warm.rec;
      const episodes = [];
      for (let k = 0; k < t.length; k++) {
        if (!on[k] || (k > 0 && on[k - 1])) continue;
        let e = k;
        while (e + 1 < t.length && on[e + 1]) e++;
        let last = e;
        while (last + 1 < t.length && t[last + 1] <= t[e] + 2000) last++;
        const iv = t.slice(k + 1, last + 1).map((x, j) => x - t[k + j]);
        const r = (v) => Math.round(v * 10) / 10;
        episodes.push({ at_ms: r(t[k] - t[0]), before_world: t[k] < worldAt, after_settle: t[k] >= settledAt, frames: e - k + 1,
          longest_frame_ms: r(Math.max(0, ...iv)), slow_ms: r(iv.filter((x) => x > SLOW_MS).reduce((a, x) => a + x, 0)) });
      }
      const load_after = loadavg()[0];
      const worst = (es) => (es.length ? Math.max(...es.map((x) => x.longest_frame_ms)) : null);
      warmup = { ...warm.w, finished: warmed, episodes, longest_frame_ms: worst(episodes),
        settled_longest_frame_ms: worst(episodes.filter((x) => x.after_settle)),
        // The warm-up right after READY, on the start tile, before the world script.
        ready_longest_frame_ms: worst(episodes.filter((x) => x.before_world)),
        load_before: +loadBefore.toFixed(2), load_after: +load_after.toFixed(2),
        // Not re-run: a warm-up happens once per session. Over the limit, the sample is only flagged.
        load_ok: !GUARD || load_after <= LOAD_LIMIT };
      console.log(`warm-up: ${JSON.stringify(warmup)}`);
    }
    if (VIA === "ui") {
      await page.locator(".tool[data-action=settings]").click();
      await page.waitForSelector(".sheet--settings:not(.is-closed) .tier[data-tier=medium]");
    }
    await page.waitForTimeout(1000);
    for (const [i, [from, to]] of STEPS.entries()) {
      const cur = (await page.evaluate(() => window.hexWorld.state().tier.tier));
      if (cur !== from) {
        await page.evaluate((t) => window.hexWorld.cmd({ cmd: "set_tier", tier: t }), from);
        await page.waitForFunction((t) => window.hexWorld.state().tier.tier === t, from);
      }
      // Every step starts settled, a warm-up (and the compile it causes) included.
      await waitSettled(page, `before ${from}>${to}`);
      await page.waitForTimeout(1500);
      const measure = async () => {
        await page.evaluate(RECORDER);
        await page.waitForTimeout(500);
        const diag0 = await page.evaluate(() => window.hexWorld.diag().frames);
        let cdp = null, chunks = [], done = null;
        if (tracePath && i === 0) {
          cdp = await page.context().newCDPSession(page);
          cdp.on("Tracing.dataCollected", (e) => chunks.push(...e.value));
          done = new Promise((r) => cdp.once("Tracing.tracingComplete", r));
          await cdp.send("Tracing.start", { traceConfig: { includedCategories: TRACE_CATS, recordMode: "recordContinuously" },
            transferMode: "ReportEvents" });
          await page.waitForTimeout(300);
        }
        const req0 = requests.length;
        const t0 = await page.evaluate(([t, ui]) => {
          const t0 = performance.now();
          if (ui) document.querySelector(`.sheet--settings .tier[data-tier=${t}]`).click();
          else window.hexWorld.cmd({ cmd: "set_tier", tier: t });
          return t0;
        }, [to, VIA === "ui"]);
        await page.waitForTimeout(WINDOW_MS);
        // A cold compile can hold the page past the window: record on until the change is drawn and
        // frames flow again (at most 60 s more), so the stall is measured whole.
        await page.waitForFunction((t) => {
          const s = window.hexWorld.state().tier;
          return s.tier === t && !s.rebuilding && performance.now() - window.__tierRec.t.at(-1) < 200;
        }, to, { timeout: 60_000, polling: 100 }).catch(() => console.log(`warn: ${from}>${to} not drawn 60 s after the window`));
        const rec = await page.evaluate(() => { window.__tierRec.on = false; return window.__tierRec; });
        if (cdp) {
          await cdp.send("Tracing.end");
          await done;
          const fh = await open(tracePath, "w");
          await fh.write("[\n");
          for (let k = 0; k < chunks.length; k++) await fh.write(JSON.stringify(chunks[k]) + (k + 1 < chunks.length ? ",\n" : "\n"));
          await fh.write("]\n");
          await fh.close();
        }
        const d = await page.evaluate(() => window.hexWorld.diag());
        const tail = (d.stall?.tail ?? []).filter((s) => s.frame > diag0)
          .map((s) => ({ frame: s.frame - diag0, ms: s.ms, pipelines_waiting: s.pipelines_waiting }));
        return { ...analyse(rec, t0, to), engine_tail: tail,
          requests: requests.slice(req0).map((r) => r.url).filter((u) => !u.startsWith("data:")) };
      };
      const g = GUARD ? await guarded(measure, `${from}>${to}`) : { result: await measure(), load_before: loadavg()[0], load_after: loadavg()[0], load_discarded: 0 };
      const row = { round: n, step: `${from}>${to}`, index: i, tiles, renderer, ready_ms, ...g.result,
        load_before: +g.load_before.toFixed(2), load_after: +g.load_after.toFixed(2), load_discarded: g.load_discarded };
      console.log(JSON.stringify({ ...row, engine_tail: row.engine_tail.length, requests: row.requests.length }));
      results.push(row);
      await waitSettled(page, `${to}`);
      await page.waitForTimeout(1500);
    }
    if (errors.length) console.log(`console errors: ${errors.slice(0, 5).join(" | ")}`);
    return { results, errors, warmup, ready_ms };
  } finally {
    await browser.close();
  }
}

const rows = [];
const errors = [];
const warmups = [];
for (let n = 0; n < ROUNDS; n++) {
  const r = await round(n, n === 0 ? process.env.TIER_TRACE : null);
  rows.push(...r.results);
  errors.push(...r.errors);
  warmups.push({ round: n, ready_ms: r.ready_ms, warmup: r.warmup });
}
const med = (v) => { const s = v.filter((x) => x != null).sort((a, b) => a - b); return s.length ? s[Math.floor(s.length / 2)] : null; };
const summary = Object.fromEntries(STEPS.map(([a, b], i) => {
  const k = `#${i + 1} ${a}>${b}`;
  const rs = rows.filter((r) => r.index === i);
  return [k, { runs: rs.length, stall_ms: med(rs.map((r) => r.stall_ms)), longest_frame_ms: med(rs.map((r) => r.longest_frame_ms)),
    slow_frames: med(rs.map((r) => r.slow_frames)), rebuild_settled_ms: med(rs.map((r) => r.rebuild_settled_ms)),
    settled_after_stall_ms: med(rs.map((r) => r.settled_after_stall_ms)), indicator_shown_ms: med(rs.map((r) => r.indicator_shown_ms)),
    indicator_hidden_after_stall_ms: med(rs.map((r) => r.indicator_hidden_after_stall_ms)),
    pipelines_added: med(rs.map((r) => r.pipelines_added)), max_load: Math.max(...rs.map((r) => Math.max(r.load_before, r.load_after))) }];
}));
summary.ready_ms = med(warmups.map((w) => w.ready_ms));
summary.warmup_longest_frame_ms = med(warmups.map((w) => w.warmup?.longest_frame_ms));
summary.warmup_settled_longest_frame_ms = med(warmups.map((w) => w.warmup?.settled_longest_frame_ms));
summary.warmup_ready_longest_frame_ms = med(warmups.map((w) => w.warmup?.ready_longest_frame_ms));
await writeFile(OUT, JSON.stringify({ when: new Date().toISOString(), radius: RADIUS, window_ms: WINDOW_MS, guard: GUARD,
  query: QUERY, summary, warmups, rows, errors }, null, 2) + "\n");
console.log(JSON.stringify(summary, null, 2));
server.close();
