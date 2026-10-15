// The new-version check: the poll interval key, the version.json body, and VersionWatch's silent
// retries, focus throttle and once-per-id report, against a fake fetch.
import assert from "node:assert/strict";
import { test } from "node:test";
import { MIN_GAP_MS, POLL_MS, VersionWatch, liveId, pollMs, updateWords } from "../src/version-check-model.ts";

test("poll interval: 3 minutes by default, version_poll seconds from the address, at least 1 s", () => {
  assert.equal(pollMs(""), POLL_MS);
  assert.equal(POLL_MS, 180_000);
  assert.equal(pollMs("?version_poll=2"), 2000);
  assert.equal(pollMs("?scenario=play&version_poll=0.2"), 1000);
  for (const bad of ["?version_poll=", "?version_poll=0", "?version_poll=-5", "?version_poll=soon"])
    assert.equal(pollMs(bad), POLL_MS, bad);
});

test("version.json bodies: only a hex id counts", () => {
  assert.equal(liveId({ id: "9180bb81e5" }), "9180bb81e5");
  for (const bad of [null, {}, { id: 7 }, { id: "<script>" }, { id: "" }, "9180bb81e5"])
    assert.equal(liveId(bad), null);
});

function harness(page: string | null = "aaaaaaaaaa") {
  let now = 1_000_000;
  let online = true;
  const urls: string[] = [];
  const found: string[] = [];
  let reply: () => Promise<Response> = async () => Response.json({ id: page });
  const watch = new VersionWatch({
    page,
    fetch: async (url, init) => {
      assert.equal(init.cache, "no-store");
      urls.push(url);
      return reply();
    },
    online: () => online,
    now: () => now,
    onNewer: (id) => found.push(id),
  });
  return {
    watch,
    urls,
    found,
    tick: (ms: number) => (now += ms),
    setOnline: (v: boolean) => (online = v),
    setReply: (r: () => Promise<Response>) => (reply = r),
  };
}

test("a newer id is reported once; the same id as the page never is", async () => {
  const t = harness();
  await t.watch.check(true);
  assert.deepEqual(t.found, []);
  t.setReply(async () => Response.json({ id: "bbbbbbbbbb" }));
  t.tick(1000);
  await t.watch.check(true);
  t.tick(1000);
  await t.watch.check(true);
  assert.deepEqual(t.found, ["bbbbbbbbbb"], "reported once");
  t.setReply(async () => Response.json({ id: "cccccccccc" }));
  t.tick(1000);
  await t.watch.check(true);
  assert.deepEqual(t.found, ["bbbbbbbbbb", "cccccccccc"], "a later deploy is reported too");
  assert.ok(
    t.urls.every((u) => /^version\.json\?t=\d+$/.test(u)),
    t.urls.join(" "),
  );
  assert.equal(new Set(t.urls).size, t.urls.length, "every request carries its own cache-busting query");
});

test("errors, bad statuses, bad bodies and offline are silent, and the next check retries", async () => {
  const t = harness();
  t.setReply(async () => {
    throw new TypeError("Failed to fetch");
  });
  await t.watch.check(true);
  t.setReply(async () => new Response("down", { status: 503 }));
  t.tick(1000);
  await t.watch.check(true);
  t.setReply(async () => new Response("<html>", { status: 200 }));
  t.tick(1000);
  await t.watch.check(true);
  t.setOnline(false);
  t.setReply(async () => Response.json({ id: "bbbbbbbbbb" }));
  t.tick(1000);
  await t.watch.check(true);
  assert.equal(t.urls.length, 3, "no request while offline");
  assert.deepEqual(t.found, []);
  t.setOnline(true);
  t.tick(1000);
  await t.watch.check(true);
  assert.deepEqual(t.found, ["bbbbbbbbbb"]);
});

test("focus and visibility checks are throttled; timed checks are not; no page id, no checks", async () => {
  const t = harness();
  await t.watch.check();
  t.tick(MIN_GAP_MS - 1);
  await t.watch.check();
  assert.equal(t.urls.length, 1, "a focus check right after another is skipped");
  await t.watch.check(true);
  assert.equal(t.urls.length, 2, "a timed check always runs");
  t.tick(MIN_GAP_MS);
  await t.watch.check();
  assert.equal(t.urls.length, 3);

  const none = harness(null);
  await none.watch.check(true);
  assert.equal(none.urls.length, 0, "a page without a build id (a dev page) never polls");
});

test("update note words: saved first, or plainly unsaved", () => {
  assert.match(updateWords("on").sub, /saved before/);
  assert.match(updateWords("off").sub, /Autosave is off, so reloading loses this world/);
  assert.equal(updateWords("failed").action, "Reload anyway");
});
