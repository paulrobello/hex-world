// Asserts the boids checkpoint's state dumps (make shots-boids): the world the script built, the
// bird and fish split (A13: birds are 75% of the slots fish leave), fish per water region (the
// coast one school, the river singles and groups of 2 to 5), roosts on the two oak tiles, birds
// roosting at the roost shot, and roosts leaving with a bulldozed oak, returning with its undo and
// leaving again with the redo.
import { readFileSync } from "node:fs";
import { join } from "node:path";

const dir = process.argv[2] ?? "docs/checkpoints/boids";
const read = (name) => JSON.parse(readFileSync(join(dir, `${name}.json`), "utf8"));
let failed = 0;
const check = (ok, what, got) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}: ${JSON.stringify(got)}`);
  if (!ok) failed++;
};

const placed = read("boids-01-placed");
const b = placed.boids;
check(placed.tiles === 20, "20 tiles placed (refused placements would be missing)", placed.tiles);
check((placed.pack_waiting ?? []).length === 0, "no tile waits on a pack", placed.pack_waiting);
check(b.mode === "play" && b.budget === 1000, "play boids on the High budget", { mode: b.mode, budget: b.budget });
check(b.water_tiles === 16, "16 water tiles", b.water_tiles);
check(b.fish === 192 && b.birds === 606, "12 fish per water tile, birds 75% of the 808 slots left (808 before A13)", { birds: b.birds, fish: b.fish });
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
const undo = read("boids-07-undo").boids;
check(undo.roosts === 24 && undo.roosting + undo.settling === 24, "undo brings the roosts back and birds fly in", { roosts: undo.roosts, roosting: undo.roosting, settling: undo.settling });

const redo = read("boids-08-redo").boids;
check(redo.roosts === 12 && redo.roosting + redo.settling === 12, "redo bulldozes the oak tile again: its roosts go", { roosts: redo.roosts, roosting: redo.roosting, settling: redo.settling });

if (failed) {
  console.error(`${failed} check(s) failed`);
  process.exit(1);
}
