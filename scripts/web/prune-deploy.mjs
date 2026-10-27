// Removes files from the built dist/ that the deployed site never fetches. Trunk's copy-dir copies all
// of assets/, so test fixtures and unplayed cues land in dist/ too. The local dist/ keeps them
// (make web-smoke serves kit_src=fixture_tiled from it); CI runs this only on the Pages artifact, after
// initial-download. index.html is left alone: its hash map's entries for pruned files are never read,
// and rewriting it would change the inline script's CSP hash.
// Usage: node scripts/web/prune-deploy.mjs [dir]   (default: dist)
import { existsSync, readdirSync, rmSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

/** Paths under dist/ the deployed site never fetches. Every entry must exist, so a rename fails here. */
export const NOT_DEPLOYED = [
  // The P0 hex texture probe (scenario=kit_test) and the kit gate fixtures (kit_src=fixture,
  // kit_src=fixture_tiled): test scenes only.
  "assets/kit/p0/hex_test.glb",
  "assets/kit/p0/hex_test.png",
  "assets/kit/p0/hex_test.bc7.ktx2",
  "assets/kit/p0/hex_test.astc.ktx2",
  "assets/kit/p1/fixture",
  "assets/kit/p1/fixture_tile",
  // title-theme is mastered but no event plays it (audio-mix.ts keeps it off the biome playlist).
  // Drop these two lines when something plays it.
  "assets/audio/music/title-theme.webm",
  "assets/audio/music/title-theme.m4a",
];

/** Deletes every NOT_DEPLOYED path under `dir`; returns the bytes removed. Throws if one is missing. */
export function pruneDeploy(dir, paths = NOT_DEPLOYED) {
  const missing = paths.filter((p) => !existsSync(join(dir, p)));
  if (missing.length) throw new Error(`prune-deploy: not in ${dir}: ${missing.join(", ")}`);
  let bytes = 0;
  for (const p of paths) {
    bytes += size(join(dir, p));
    rmSync(join(dir, p), { recursive: true });
  }
  return bytes;
}

function size(path) {
  const st = statSync(path);
  if (!st.isDirectory()) return st.size;
  let total = 0;
  for (const entry of readdirSync(path)) total += size(join(path, entry));
  return total;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const dir = process.argv[2] ?? "dist";
  const bytes = pruneDeploy(dir);
  console.log(`prune-deploy: removed ${NOT_DEPLOYED.length} paths, ${(bytes / 1048576).toFixed(2)} MiB from ${dir}`);
}
