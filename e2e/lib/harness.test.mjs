// Unit tests for the shared e2e static server: loopback-only bind and path containment (SEC-001).
// Runs under make ui-test; needs no browser and no e2e/node_modules.
import test from "node:test";
import assert from "node:assert/strict";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { serveDist } from "./harness.mjs";

/** A served root plus a secret file one level above it, both removed on cleanup. */
function fixture() {
  const parent = mkdtempSync(join(tmpdir(), "hex-harness-"));
  const root = join(parent, "root");
  mkdirSync(root);
  writeFileSync(join(root, "index.html"), "<p>hi</p>");
  writeFileSync(join(root, "data.json"), "{}");
  const secret = join(parent, "outside-secret.txt");
  writeFileSync(secret, "secret");
  return { parent, root, secret };
}

test("serveDist binds 127.0.0.1 on an ephemeral port and serves files under the root", async () => {
  const { parent, root } = fixture();
  const { server, base } = await serveDist(root);
  try {
    assert.equal(server.address().address, "127.0.0.1");
    const page = await fetch(new URL("index.html", base));
    assert.equal(page.status, 200);
    assert.equal(page.headers.get("content-type"), "text/html");
    assert.equal(await page.text(), "<p>hi</p>");
    const rootDoc = await fetch(base);
    assert.equal(rootDoc.status, 200);
    assert.equal(await rootDoc.text(), "<p>hi</p>");
    const json = await fetch(new URL("data.json", base));
    assert.equal(json.status, 200);
    assert.equal(json.headers.get("content-type"), "application/json");
    assert.equal((await fetch(new URL("missing.html", base))).status, 404);
  } finally {
    server.close();
    rmSync(parent, { recursive: true });
  }
});

test("encoded traversal outside the root is refused", async () => {
  const { parent, root, secret } = fixture();
  const { server, base } = await serveDist(root);
  try {
    for (const url of [
      new URL("/..%2f..%2foutside-secret.txt", base),
      new URL("/%2e%2e/%2e%2e/outside-secret.txt", base),
      new URL("/..%2foutside-secret.txt", base),
    ]) {
      const res = await fetch(url);
      assert.ok(res.status === 403 || res.status === 404, `GET ${url.pathname} -> ${res.status}`);
      assert.ok(!res.ok, `traversal must not serve ${secret}`);
    }
    // A traversal that lands back inside the root is served as any other root-relative path.
    const inside = await fetch(new URL("/%2e%2e/%2e%2e/root/index.html", base));
    assert.ok(inside.status === 200 || inside.status === 404, `inside traversal -> ${inside.status}`);
  } finally {
    server.close();
    rmSync(parent, { recursive: true });
  }
});

test("malformed percent-encoding answers 400, not a crash", async () => {
  const { parent, root } = fixture();
  const { server, base } = await serveDist(root);
  try {
    const res = await fetch(new URL("/%zz", base));
    assert.equal(res.status, 400);
  } finally {
    server.close();
    rmSync(parent, { recursive: true });
  }
});

test("respond can intercept a path before the file serve", async () => {
  const { parent, root } = fixture();
  const { server, base } = await serveDist(root, {
    respond: (req, res, file) => {
      if (!file.endsWith("/data.json")) return false;
      res.writeHead(503);
      res.end("down");
      return true;
    },
  });
  try {
    const intercepted = await fetch(new URL("data.json", base));
    assert.equal(intercepted.status, 503);
    assert.equal(await intercepted.text(), "down");
    assert.equal((await fetch(new URL("index.html", base))).status, 200);
  } finally {
    server.close();
    rmSync(parent, { recursive: true });
  }
});
