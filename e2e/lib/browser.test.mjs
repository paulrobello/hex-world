// Unit tests for the shared browser helpers (QA-002) that need no browser: argument selection and the
// condition waits. Runs under make ui-test without e2e/node_modules.
import test from "node:test";
import assert from "node:assert/strict";
import { gpuArgs, until, untilInPage, waiter } from "./browser.mjs";

const idle = { waitForTimeout: (ms) => new Promise((r) => setTimeout(r, ms)) };

test("gpuArgs picks Metal by default and SwiftShader on request, appending extras", () => {
  assert.deepEqual(gpuArgs({ swiftshader: false }),
    ["--use-angle=metal", "--enable-gpu", "--ignore-gpu-blocklist", "--enable-unsafe-webgpu"]);
  assert.deepEqual(gpuArgs({ swiftshader: true, extra: ["--mute-audio"] }),
    ["--use-angle=swiftshader", "--enable-unsafe-swiftshader", "--enable-unsafe-webgpu", "--mute-audio"]);
});

test("until returns the first truthy value", async () => {
  let n = 0;
  const v = await until(idle, async () => (++n >= 3 ? "ready" : null), "three polls", { interval: 1 });
  assert.equal(v, "ready");
  assert.equal(n, 3);
});

test("until throws naming the condition when it never holds", async () => {
  await assert.rejects(until(idle, async () => false, "the pin clears", { ms: 20, interval: 1 }),
    /timed out: the pin clears/);
});

test("until reports through onTimeout and returns the last value", async () => {
  const seen = [];
  const v = await until(idle, async () => 0, "never", { ms: 20, interval: 1, onTimeout: (m) => seen.push(m) });
  assert.equal(v, 0);
  assert.deepEqual(seen, ["timed out: never"]);
});

test("waiter binds the page and the failure sink, and reads the page lazily", async () => {
  const seen = [];
  let current = idle;
  const waitFor = waiter(() => current, { fail: (m) => seen.push(m), ms: 20, interval: 1 });
  assert.equal(await waitFor(async () => "yes", "immediate"), "yes");
  assert.equal(await waitFor(async () => null, "never"), null);
  assert.deepEqual(seen, ["timed out: never"]);
  current = { waitForTimeout: () => { throw new Error("second page used"); } };
  await assert.rejects(waitFor(async () => false, "again"), /second page used/);
});

test("untilInPage evaluates the condition through the page", async () => {
  let n = 0;
  const page = { ...idle, evaluate: async (fn) => fn(++n) };
  assert.equal(await untilInPage(page, (k) => k >= 2 && "done", "second evaluate", { interval: 1 }), "done");
});
