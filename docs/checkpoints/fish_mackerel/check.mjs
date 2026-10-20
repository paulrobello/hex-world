// Asserts the fish_mackerel checkpoint's state dumps (mackerel.json, the boids checkpoint's world): the
// four boid pieces drew, and the fish species follow the spawn water, every river-region fish a trout and
// every coast-region fish a mackerel, with the fish budget and regions as in the boids checkpoint.
// Usage: node docs/checkpoints/fish_mackerel/check.mjs [dir]
import { readFileSync } from "node:fs";
import { join } from "node:path";

const dir = process.argv[2] ?? "docs/checkpoints/fish_mackerel";
let failed = 0;
const check = (ok, what, got) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}: ${JSON.stringify(got)}`);
  if (!ok) failed++;
};
for (const name of ["mk-01-placed", "mk-02-state"]) {
  const b = JSON.parse(readFileSync(join(dir, `${name}.json`), "utf8")).boids;
  const river = b.fish_regions.find((r) => r.river);
  const coast = b.fish_regions.find((r) => !r.river);
  check(b.pieces === "ready", `${name}: bird_starling, bird_goldfinch, fish_trout and fish_mackerel drew`, b.pieces);
  check(b.fish === 192 && river?.fish === 48 && coast?.fish === 144, `${name}: fish budget and regions unchanged`,
    b.fish_regions.map((r) => [r.tiles, r.fish]));
  check(JSON.stringify(b.fish_species) === JSON.stringify([river?.fish, coast?.fish]),
    `${name}: [trout, mackerel] = [river fish, coast fish]`, b.fish_species);
  check(river?.river_palette === river?.fish && coast?.river_palette === 0, `${name}: palettes unchanged`,
    [river?.river_palette, coast?.river_palette]);
}
process.exit(failed ? 1 : 0);
