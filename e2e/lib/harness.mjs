// Shared static server for the e2e and perf scripts (SEC-001): binds 127.0.0.1 only and contains
// every served path inside the root, so a run cannot expose the host's files to the local network.
// Imports only node: modules: the unit test (harness.test.mjs) and make ui-test run without a
// browser and without e2e/node_modules.
import { createServer } from "node:http";
import { readFileSync } from "node:fs";
import { extname, resolve, sep } from "node:path";

/** The union of the per-script content-type maps, merged when the scripts started sharing this. */
export const TYPES = {
  ".css": "text/css",
  ".glb": "model/gltf-binary",
  ".html": "text/html",
  ".jpg": "image/jpeg",
  ".js": "text/javascript",
  ".json": "application/json",
  ".ktx2": "image/ktx2",
  ".m4a": "audio/mp4",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".wasm": "application/wasm",
  ".webm": "audio/webm",
  ".webp": "image/webp",
  ".wesl": "text/plain",
  ".wgsl": "text/plain",
  ".woff2": "font/woff2",
};

/**
 * Serves `root` on an ephemeral 127.0.0.1 port; resolves to `{ server, base }`, `base` the
 * trailing-slash origin to join page URLs against. Decoding happens after URL parsing, so a
 * `%2f` becomes a separator only here, where the containment check sees it; `..` segments
 * therefore cannot resolve outside `root` (403) and malformed encodings get 400.
 * Optional hooks: `respond(req, res, file)` returning true handles the request itself (used to
 * intercept paths, e.g. a swapped version.json); `onRequest(req, path)` observes without
 * changing the response.
 */
export function serveDist(root, { types = TYPES, onRequest, respond } = {}) {
  const base = resolve(root);
  const server = createServer((req, res) => {
    let path;
    try {
      path = decodeURIComponent(new URL(req.url, "http://x").pathname);
    } catch {
      res.writeHead(400);
      res.end();
      return;
    }
    if (path.endsWith("/")) path += "index.html";
    const file = resolve(base, "." + path);
    if (file !== base && !file.startsWith(base + sep)) {
      res.writeHead(403);
      res.end();
      return;
    }
    onRequest?.(req, path);
    if (respond?.(req, res, file) === true) return;
    try {
      const body = readFileSync(file);
      res.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream" });
      res.end(body);
    } catch {
      res.writeHead(404);
      res.end();
    }
  });
  return new Promise((ok) =>
    server.listen(0, "127.0.0.1", () =>
      ok({ server, base: `http://127.0.0.1:${server.address().port}/` })));
}
