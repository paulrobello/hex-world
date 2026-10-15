// Cache busting: scripts/web/version-dist.mjs inlines a content hash for every file in dist/ into
// index.html as window.__HEX_HASHES (dist-relative path to hash), so a fetched URL changes exactly
// when its file does. The wasm's fetches use the same map (crates/hex-assets/src/bevy.rs).

type Hashes = Readonly<Record<string, string>>;

const warned = new Set<string>();

function pageHashes(): Hashes | undefined {
  return (globalThis as { __HEX_HASHES?: Hashes }).__HEX_HASHES;
}

/** The URL to fetch for the page-relative `path`: `path?v=<hash>`, or `path` itself when the map
 *  lacks it, warned once per path (once in all without a map); web-smoke fails on such a request. */
export function versioned(path: string, hashes: Hashes | undefined = pageHashes()): string {
  const key = path.replace(/^\.?\//, "");
  if (!hashes) {
    if (!warned.has("")) {
      warned.add("");
      console.warn("assets: no window.__HEX_HASHES in the page; fetching every asset unversioned");
    }
    return path;
  }
  const hash = Object.hasOwn(hashes, key) ? hashes[key] : undefined;
  if (hash) return `${path}?v=${hash}`;
  if (!warned.has(key)) {
    warned.add(key);
    console.warn(`assets: ${key} has no content hash in window.__HEX_HASHES; fetched unversioned`);
  }
  return path;
}
