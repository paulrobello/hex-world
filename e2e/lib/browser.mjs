// Shared Playwright helpers for the e2e and perf scripts (QA-002): the GPU launch arguments, one
// launch path, the condition wait and the hardware-adapter check that the scripts used to paste.
// Playwright is imported inside `launch`, so this module loads without e2e/node_modules and its unit test (browser.test.mjs) runs under make ui-test.
import { loadavg } from "node:os";

/**
 * Chromium arguments for a WebGPU run. Metal hardware by default; `swiftshader` (default: the
 * PW_GPU=swiftshader environment) selects the CPU rasteriser. `extra` is appended. Keep
 * --enable-unsafe-webgpu out of a deliberate no-GPU run: with it, --disable-gpu still yields a
 * SwiftShader adapter, so such a run passes its own arguments to `launch` instead (smoke.mjs).
 */
export function gpuArgs({ swiftshader = process.env.PW_GPU === "swiftshader", extra = [] } = {}) {
  const base = swiftshader
    ? ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--enable-unsafe-webgpu"]
    : ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu"];
  return [...base, ...extra];
}

/** Arguments of an uncapped timing run: no vsync, no frame-rate limit, timestamp quantization lifted. */
export const UNCAPPED_ARGS = ["--disable-gpu-vsync", "--disable-frame-rate-limit"];
export const DEVELOPER_FEATURES_ARG = "--enable-webgpu-developer-features";

let loadLogged = false;

/**
 * Launches Chromium (`channel: "chromium"`, headless unless told otherwise) and logs the host's
 * 1-minute load average once per process, so a flaky run records the load it ran under.
 * `assertBackend: true` opens a scratch page and fails the launch when the WebGL renderer is not
 * the requested one: SwiftShader when `swiftshader`, anything else otherwise.
 */
export async function launch({ args = gpuArgs(), headless = true, assertBackend = false,
  swiftshader = process.env.PW_GPU === "swiftshader" } = {}) {
  const { chromium } = await import("playwright");
  if (!loadLogged) {
    loadLogged = true;
    console.log(`host load (1 min): ${loadavg()[0].toFixed(2)}`);
  }
  const browser = await chromium.launch({ channel: "chromium", headless, args });
  if (assertBackend) {
    const page = await browser.newPage();
    try {
      const renderer = await webglRenderer(page);
      if (/swiftshader/i.test(renderer ?? "") !== swiftshader)
        throw new Error(`wanted ${swiftshader ? "SwiftShader" : "a hardware"} renderer, got ${renderer}`);
    } catch (e) {
      await browser.close();
      throw e;
    } finally {
      await page.close().catch(() => {});
    }
  }
  return browser;
}

/** The page's `UNMASKED_RENDERER_WEBGL`, or null when the context exposes none. */
export function webglRenderer(page) {
  return page.evaluate(() => {
    const gl = document.createElement("canvas").getContext("webgl");
    const ext = gl?.getExtension("WEBGL_debug_renderer_info");
    return ext ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) : null;
  });
}

/** The WebGPU adapter's "vendor | architecture | description", throwing unless it is hardware. */
export async function requireHardwareAdapter(page) {
  const adapter = await page.evaluate(async () => {
    const i = (await navigator.gpu.requestAdapter())?.info;
    return i ? [i.vendor, i.architecture, i.description].join(" | ") : null;
  });
  if (!adapter || /swiftshader/i.test(adapter)) throw new Error("no hardware WebGPU adapter: " + adapter);
  return adapter;
}

/**
 * Polls `fn` (an async node-side function returning a truthy value when its condition holds) until
 * it does, and returns that value. After `ms` it throws an Error naming `what`, or, with an
 * `onTimeout(message)` callback (a script's `fail`), reports through it and returns the last value
 * so the run goes on collecting failures. `interval` is the poll period in ms; `log` prints
 * "ok: <what>" when the condition holds.
 */
export async function until(page, fn, what, { ms = 10_000, interval = 50, onTimeout, log = false } = {}) {
  const deadline = Date.now() + ms;
  for (;;) {
    const v = await fn();
    if (v) {
      if (log) console.log("ok:", what);
      return v;
    }
    if (Date.now() > deadline) {
      const message = `timed out: ${what}`;
      if (!onTimeout) throw new Error(message);
      onTimeout(message);
      return v;
    }
    await page.waitForTimeout(interval);
  }
}

/**
 * A condition wait bound to a page and a failure sink, for scripts that keep collecting failures:
 * `const waitFor = waiter(() => page, { fail, ms: 15_000 })`, then `await waitFor(fn, "what", ms?)`.
 * `getPage` is a function so a script that replaces its page (a fresh context) waits on the new one.
 */
export function waiter(getPage, { fail, ms: defaultMs = 10_000, interval = 50, log = false } = {}) {
  return (fn, what, ms = defaultMs) =>
    until(getPage(), fn, what, { ms, interval, log, ...(fail ? { onTimeout: fail } : {}) });
}

/** `until` for a condition evaluated in the page: `fn` runs through `page.evaluate`. */
export function untilInPage(page, fn, what, opts) {
  return until(page, () => page.evaluate(fn), what, opts);
}

/**
 * Resolves once every finite CSS animation and transition on the page has finished and two
 * animation frames have run, so a capture follows the page's own motion instead of a fixed delay.
 * Looping animations (a breathing outline) never finish and are skipped.
 */
export function animationsSettled(page) {
  return page.evaluate(async () => {
    const finite = () => document.getAnimations().filter((a) => a.effect?.getComputedTiming().iterations !== Infinity);
    await Promise.all(finite().map((a) => a.finished.catch(() => {})));
    await new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done)));
  });
}
