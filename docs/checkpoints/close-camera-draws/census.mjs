// Draw census of the 20-tile scatter world (docs/checkpoints/lod-band-instanced/draws.json) from the
// state dumps `make shots-piece` writes: draws over every pass and cascade, as
// docs/checkpoints/scatter/check.mjs counts them, plus the instanced kit's draws by piece
// (`diag.kit_draws`).
//
// Usage: node docs/checkpoints/close-camera-draws/census.mjs <shots dir> [label] > census-<label>.json
import { readFileSync } from "node:fs";
import { join } from "node:path";

const dir = process.argv[2];
const label = process.argv[3] ?? "";
const CAMERAS = ["overview", "forest-cluster", "meadow", "band"];
const out = { label, cameras: {} };
for (const name of CAMERAS) {
  const dump = JSON.parse(readFileSync(join(dir, `draws-${name}.json`), "utf8"));
  const passes = dump.diag?.passes ?? [];
  const count = (p) => p.multidraw_sets + p.batchable_bins + p.unbatchable + p.non_mesh;
  const kit = dump.diag?.kit_draws ?? null;
  out.cameras[name] = {
    draws: passes.reduce((n, p) => n + count(p), 0),
    passes: Object.fromEntries(passes.map((p) => [`${p.view}/${p.phase}`, count(p)])),
    kit_slots: kit?.slots ?? null,
    kit_draws: kit?.total ?? null,
    kit_by_piece: kit ? Object.fromEntries(Object.entries(kit.by_piece).map(([k, v]) => [k, [v.camera, v.shadow]])) : null,
  };
}
console.log(JSON.stringify(out, null, 1));
