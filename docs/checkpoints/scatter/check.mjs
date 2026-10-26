// Asserts the scatter checkpoint's state dumps (make shots-scatter): every pack landed before the
// shots, each pine forest, oak tile, meadow and straight road scatters its planned count (the density per 100 m²
// of cap times 665 m²) and draws all of it, the river straight draws 0 to 4 bank props (owner 2026-10-26),
// nothing else scatters, only the oak tiles roost (12
// each), the rising forest scatters, bulldoze takes its scatter and undo brings back the same
// counts, and the 20-tile world's overview camera stays inside the 500-draw budget (owner A2 Q1,
// 2026-10-21: close cameras are tracked, not gated) with at most 60 scatter (mesh, material) pairs
// per LOD and 180 in all (A2 Q2).
import { readFileSync } from "node:fs";
import { join } from "node:path";

const dir = process.argv[2] ?? "docs/checkpoints/scatter";
const read = (name) => JSON.parse(readFileSync(join(dir, `${name}.json`), "utf8"));
let failed = 0;
const check = (ok, what, got) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}: ${JSON.stringify(got)}`);
  if (!ok) failed++;
};
const CAP = 1.5 * Math.sqrt(3) * 16 * 16;
const DENSITY = {
  forest: { tree_forest: 3.0, rock_small: 0.2 },
  oak: { rock_small: 0.5, wildflower_clump: 0.9 },
  meadow: { wildflower_clump: 4.0, rock_small: 0.3 },
  road_straight: { wildflower_single: 3.0 },
};
// Capped tiles: a seeded 0 to cap of their placed pieces (scatter.rs RIVER_MAX), each drawn.
const CAPPED = { river_straight: 4 };
const key = (t) => `${t.q},${t.r}`;

const placed = read("scatter-01-placed");
const s = placed.scatter;
check(placed.tiles === 19, "19 tiles placed (refused placements would be missing)", placed.tiles);
check(Object.values(placed.packs).every((v) => v === "ready"), "every pack ready", placed.packs);
check((placed.pack_waiting ?? []).length === 0 && s.waiting.length === 0, "no tile waits on a pack or its ground", { packs: placed.pack_waiting, scatter: s.waiting });
const scattering = placed.play.tiles.filter((t) => t.tile in DENSITY || (t.tile in CAPPED && s.tiles.some((x) => key(x) === key(t)))).map(key).sort();
check(JSON.stringify(s.tiles.map(key).sort()) === JSON.stringify(scattering), "exactly the pine forests, oak tiles, meadows, straight roads and river straights with bank props scatter", s.tiles.map((t) => `${t.tile}@${key(t)}`));
for (const t of s.tiles.filter((x) => x.tile in CAPPED)) {
  const n = Object.values(t.placed).reduce((a, b) => a + b, 0);
  const ok = n <= CAPPED[t.tile] && Object.keys(t.placed).every((p) => t.planned[p] === t.placed[p] && t.placed[p] === t.drawn[p]);
  check(ok, `${t.tile}@${key(t)} draws 0 to ${CAPPED[t.tile]} bank props, planned = placed = drawn`, [n, t.placed, t.drawn]);
}
for (const t of s.tiles.filter((x) => x.tile in DENSITY)) {
  for (const [piece, d] of Object.entries(DENSITY[t.tile])) {
    const want = (d * CAP) / 100;
    const ok = t.planned[piece] === t.placed[piece] && t.placed[piece] === t.drawn[piece] && Math.abs(t.placed[piece] - want) < 1;
    check(ok, `${t.tile}@${key(t)} ${piece} planned = placed = drawn ≈ ${want.toFixed(2)}`, [t.planned[piece], t.placed[piece], t.drawn[piece]]);
  }
}
const oaks = s.tiles.filter((t) => t.tile === "oak").length;
check(oaks === 2 && placed.boids.roosts === 12 * oaks, `12 roosts on each of ${oaks} oak tiles, none on the pine forests`, placed.boids.roosts);

const rising = read("scatter-06-rise-props");
const r = rising.play.rising.find((x) => x.q === 1 && x.r === 2);
check(r?.stage === "props", "the new forest is in its props stage at the rise shot", rising.play.rising);
const risingRow = rising.scatter.tiles.find((t) => t.q === 1 && t.r === 2);
check(risingRow?.drawn.tree_forest === 20, "the rising forest draws its spruces", risingRow);

const gone = read("scatter-08-bulldozed").scatter.tiles.find((t) => t.q === 1 && t.r === 2);
check(gone === undefined, "bulldoze takes the forest's scatter", gone);
const back = read("scatter-09-undo").scatter.tiles.find((t) => t.q === 1 && t.r === 2);
check(JSON.stringify(back) === JSON.stringify(risingRow), "undo brings back the same scatter", back);

const world = read("scatter-10-world");
check(world.tiles === 20, "the 20-tile world", world.tiles);
const census = (dump) => {
  const passes = dump.diag?.passes ?? [];
  const draws = passes.reduce((n, p) => n + p.multidraw_sets + p.batchable_bins + p.unbatchable + p.non_mesh, 0);
  return { passes, draws, by_pass: passes.map((p) => `${p.view}/${p.phase}:${p.multidraw_sets}+${p.batchable_bins}+${p.unbatchable}+${p.non_mesh}`) };
};
const overview = census(world);
check(overview.passes.length > 0 && overview.draws <= 500, "overview camera: draw calls over every pass and cascade within 500 (PRD §10)", { draws: overview.draws, passes: overview.by_pass });
// The close cameras are dumped after the overview, once the pass census is sampled (it is not
// during the first 600-frame window), at the scatter-02 and scatter-03 still cameras.
for (const name of ["scatter-10-forest-cluster", "scatter-10-meadow"]) {
  const c = census(read(name));
  console.log(`track ${name.replace("scatter-10-", "")} camera: ${c.draws} draws over every pass and cascade (tracked, not gated) ${JSON.stringify(c.by_pass)}`);
}
const pairs = world.scatter.mesh_material_pairs_by_lod ?? [];
check(pairs.length === 3 && pairs.every((n) => n <= 60), "scatter adds at most 60 (mesh, material) batches per LOD", pairs);
check(world.scatter.mesh_material_pairs <= 180, "scatter adds at most 180 (mesh, material) batches over all LODs", world.scatter.mesh_material_pairs);
const reduced = read("scatter-11-reduced");
const cut = reduced.scatter.tiles.find((t) => t.q === 1 && t.r === 2);
check(reduced.play.reduced_motion === true && reduced.play.rising.length === 0, "reduced motion: the undone forest is at rest at once", { reduced: reduced.play.reduced_motion, rising: reduced.play.rising });
check(JSON.stringify(cut) === JSON.stringify(risingRow) && cut.drawn.tree_forest === cut.planned.tree_forest, "reduced motion: its scatter is drawn in full straight away", cut);
// scatter-stream.json, run with HEXW_PACK_HOLD_FRAMES=120: the streamed packs are held, so the new
// forest waits on its ground (forest_base) and the start meadow's cap is bare until props lands.
const held = read("scatter-stream-1-held").scatter;
const heldMeadow = held.tiles.find((t) => t.q === 0 && t.r === 0);
check(held.waiting.some((t) => t.q === 1 && t.r === 0) && !held.tiles.some((t) => t.q === 1 && t.r === 0), "held: the forest shows its placeholder, its scatter waits on the forest pack", held.waiting);
check(heldMeadow && heldMeadow.drawn.wildflower_clump === 0 && heldMeadow.planned.wildflower_clump > 0, "held: the meadow's flowers wait on the props pack", heldMeadow);
const landed = read("scatter-stream-2-landed");
const landedRows = landed.scatter.tiles;
check(Object.values(landed.packs).every((v) => v === "ready") && landed.scatter.waiting.length === 0 && landedRows.length === 2 &&
  landedRows.every((t) => Object.keys(t.planned).every((p) => t.drawn[p] === t.planned[p])), "landed: both tiles draw their whole scatter", landedRows);
check(JSON.stringify(landedRows.find((t) => t.q === 0 && t.r === 0).planned) === JSON.stringify(heldMeadow.planned), "landed: the meadow's layout did not change when its pieces arrived", heldMeadow.planned);
console.log(`scatter: ${world.scatter.drawn} instances (${JSON.stringify(world.scatter.drawn_by_piece)}), ${world.scatter.entities} entities`);

if (failed) {
  console.error(`${failed} check(s) failed`);
  process.exit(1);
}
