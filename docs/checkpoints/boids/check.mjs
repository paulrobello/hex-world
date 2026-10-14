// Asserts the boids checkpoint's state dumps (make shots-boids): the single start tile flies no
// birds, the world the script built, the bird slots (A13: 75% of the slots fish leave) and the
// birds flying (a flock per 4 land tiles, owner 2026-10-14), fish per water region (the coast one
// school, the river singles and groups of 2 to 5), roosts on the two oak tiles, birds roosting at
// the roost shot, roosts leaving with a bulldozed oak, returning with its undo and leaving again
// with the redo; then a spring, straight and mouth joined to the coast: river-spawned fish keep
// their small groups in the joined region, and fish cross the mouth's fan (read back from the GPU
// by the fixed-clock run's fish track) within the 6,240 frames (104 s) after the chain is placed.
import { readFileSync } from "node:fs";
import { join } from "node:path";

const dir = process.argv[2] ?? "docs/checkpoints/boids";
const read = (name) => JSON.parse(readFileSync(join(dir, `${name}.json`), "utf8"));
let failed = 0;
const check = (ok, what, got) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}: ${JSON.stringify(got)}`);
  if (!ok) failed++;
};

const start = read("boids-00-start");
check(start.tiles === 1 && start.boids.birds === 0 && start.boids.leaving === 0,
  "the single start tile flies no birds", { tiles: start.tiles, birds: start.boids.birds });

const placed = read("boids-01-placed");
const b = placed.boids;
check(placed.tiles === 29, "29 tiles placed (refused placements would be missing)", placed.tiles);
check((placed.pack_waiting ?? []).length === 0, "no tile waits on a pack", placed.pack_waiting);
check(b.mode === "play" && b.budget === 1000, "play boids on the High budget", { mode: b.mode, budget: b.budget });
check(b.water_tiles === 16, "16 water tiles", b.water_tiles);
check(b.fish === 192 && b.bird_slots === 606, "12 fish per water tile, bird slots 75% of the 808 slots left", { bird_slots: b.bird_slots, fish: b.fish });
check(b.birds === 36, "13 land tiles fly three flocks of 12", b.birds);
const regions = b.fish_regions.map((r) => [r.tiles, r.fish]).sort((x, y) => y[0] - x[0]);
check(JSON.stringify(regions) === JSON.stringify([[12, 144], [4, 48]]), "coast region 12 tiles 144 fish, river 4 tiles 48 fish", b.fish_regions);
const river = b.fish_regions.find((r) => r.tiles === 4);
const coast = b.fish_regions.find((r) => r.tiles === 12);
check(river?.river === true && river.largest_group <= 5 && river.largest_group >= 2 && river.singles * 2 > river.fish,
  "river fish: most alone, the rest in groups of 2 to 5", river);
check(coast?.river === false && coast.largest_group === coast.fish && coast.singles === 0, "coast fish: one school", coast);
check(b.roosts === 24, "12 roosts on each oak tile (its oak's canopy)", b.roosts);
check(b.dispatched === 1000, "the GPU pass ran every slot", b.dispatched);
check(b.pieces === "ready", "the flock draws the bird_starling and fish_trout kit pieces", b.pieces);
check(river?.river_palette === river?.fish && river?.fish > 0, "river fish take the river palette (owner Q5)", river);
check(coast?.river_palette === 0 && coast?.fish > 0, "coast and ocean fish take the ocean palette", coast);

const roost = read("boids-02-roost").boids;
check(roost.roosting === 24 && roost.settling === 0, "every roost taken at the roost shot", { roosting: roost.roosting, settling: roost.settling });

const bulldozed = read("boids-06-bulldozed").boids;
check(bulldozed.roosts === 12 && bulldozed.roosting === 12, "bulldozing an oak tile takes its roosts and launches its birds", { roosts: bulldozed.roosts, roosting: bulldozed.roosting });
check(bulldozed.birds === 36 && bulldozed.leaving === 0, "12 land tiles still fly three flocks", { birds: bulldozed.birds, leaving: bulldozed.leaving });
const undo = read("boids-07-undo").boids;
check(undo.roosts === 24 && undo.roosting + undo.settling === 24, "undo brings the roosts back and birds fly in", { roosts: undo.roosts, roosting: undo.roosting, settling: undo.settling });

const redoState = read("boids-08-redo");
const redo = redoState.boids;
check(redo.roosts === 12 && redo.roosting + redo.settling === 12, "redo bulldozes the oak tile again: its roosts go", { roosts: redo.roosts, roosting: redo.roosting, settling: redo.settling });
check(redo.fish_track?.samples > 0 && redo.fish_track.crossed === 0, "no mouth yet, no crossing (control)", redo.fish_track);

const mouthState = read("boids-09-mouth");
const m = mouthState.boids;
check(mouthState.tiles === 31 && m.water_tiles === 19 && m.fish === 228 && m.birds === 36,
  "spring, straight and mouth placed: 19 water tiles, 228 fish, birds unchanged", { tiles: mouthState.tiles, water: m.water_tiles, fish: m.fish, birds: m.birds });
const joined = m.fish_regions.find((r) => r.tiles === 15);
check(joined?.river === false && joined.fish === 180, "the chain joins the coast: one 15-tile region of 180 fish", m.fish_regions);
check(joined?.river_fish === 24 && joined.river_largest_group >= 2 && joined.river_largest_group <= 5 && joined.river_singles * 2 > joined.river_fish,
  "river-spawned fish in the joined region: most alone, the rest in groups of 2 to 5", joined);
check(joined?.river_palette === joined?.river_fish, "river-spawned fish take the river palette", joined);
check(joined?.largest_group === joined.fish - joined.river_fish, "sea-spawned fish school as one", joined);

const crossed = read("boids-12-mouth-crossed").boids;
const late = crossed.fish_regions.find((r) => r.tiles === 15);
check(crossed.fish_track?.crossed >= 1, "at least one fish crossed the mouth's fan", crossed.fish_track);
check(late?.river_largest_group <= 5, "river groups stay at 5 or fewer", late);

if (failed) {
  console.error(`${failed} check(s) failed`);
  process.exit(1);
}
