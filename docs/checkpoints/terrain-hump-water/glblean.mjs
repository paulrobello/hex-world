// Border hump on the water-family bases (card "Terrain caps: remove the hump along tile borders",
// river/spring/coast/mouth pass). For each piece's shipped (meshopt) GLB and each LOD: over the land
// part only (the meadow beyond the channel's bank-top falloff, the grass beyond the beach), per band of
// distance from the hex border, the mean height of the cap vertices and the mean slope toward the
// border of their NORMAL attribute (atan, degrees; positive = the shading leans as if the cap rose to
// the border). A cap whose relief rises to z = 0 over the last 2 m shows a positive lean there.
// Usage: node glblean.mjs <repo> <assets-dir> [piece ...]   (prints a table and one JSON line per piece)
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

const [repo, assets, ...only] = process.argv.slice(2);
const require = createRequire(pathToFileURL(`${repo}/scripts/assets/package.json`).href);
const { NodeIO } = require("@gltf-transform/core");
const { ALL_EXTENSIONS } = require("@gltf-transform/extensions");
const { MeshoptDecoder } = require("meshoptimizer");
await MeshoptDecoder.ready;
const io = new NodeIO().registerExtensions(ALL_EXTENSIONS).registerDependencies({ "meshopt.decoder": MeshoptDecoder });

const A = 16 * Math.sqrt(3) / 2;
const BANK_FALLOFF_END = 4.33 + 2.5 + 2.0; // _river.BANK_TOP + 2: fc = 1 beyond (8.83)
const POOL_FALLOFF_END = 5.95 + 2.5 + 2.0; // _spring.POOL_TOP + 2 (10.45)
const C30 = Math.cos(Math.PI / 6), S30 = Math.sin(Math.PI / 6);
const ARC_C = 2 * A, ARC_R = 24;
const sStraight = (x, y) => x * C30 - y * S30; // Blender (x, y) -> runtime (x, -y)
const segDist = (x, y) => { // _spring.seg_dist: stream N = (-2, 0) .. E = (A, 0)
  const t = Math.min(Math.max((x + 2) / (A + 2), 0), 1);
  return Math.hypot(x - (-2 + t * (A + 2)), y);
};
const mouthDist = (edge) => (x, y) => { // _mouth_profile.dist: ray from edge midpoint through the centre
  const a = (-60 * edge * Math.PI) / 180, nx = Math.cos(a), ny = Math.sin(a);
  const ax = nx * A, ay = ny * A, ux = -nx, uy = -ny;
  const t = Math.max((x - ax) * ux + (y - ay) * uy, 0);
  return Math.hypot(x - (ax + ux * t), y - (ay + uy * t));
};
// Land part per piece: where the base's own relief (if any) is the plain meadow swell under the border
// falloff, or the flat grass of a coast tile.
const LAND = {
  meadow_base: () => true,
  forest_base: () => true,
  river_straight_base: (x, y) => Math.abs(y) >= BANK_FALLOFF_END,
  river_bend_base: (x, y) => Math.abs(Math.hypot(x - A, y + 24) - 24) >= BANK_FALLOFF_END,
  river_spring_base: (x, y) => segDist(x, y) >= BANK_FALLOFF_END && Math.hypot(x + 2, y) >= POOL_FALLOFF_END,
  beach_base: (x, y) => sStraight(x, y) <= -8,
  beach_outer_base: (x, y) => Math.hypot(x + ARC_C, -y) - ARC_R <= -8,
  beach_inner_base: (x, y) => ARC_R - Math.hypot(x - ARC_C, -y) <= -8,
  river_mouth_base: (x, y) => sStraight(x, y) <= -8 && mouthDist(3)(x, y) >= BANK_FALLOFF_END,
  river_mouth_l_base: (x, y) => sStraight(x, y) <= -8 && mouthDist(4)(x, y) >= BANK_FALLOFF_END,
};
const BANDS = [[0, 0.01], [0.01, 0.5], [0.5, 1], [1, 1.5], [1.5, 2], [2, 4], [4, 14]];
const pieces = only.length ? only : Object.keys(LAND);
for (const piece of pieces) {
  // <assets-dir> is an `assets/` tree, or a `target/kit-stage` dir for the uncompressed staged GLBs.
  const glb = assets.endsWith("kit-stage") ? `${assets}/${piece}/${piece}.glb` : `${assets}/kit/p1/${piece}/${piece}.glb`;
  const doc = await io.read(glb);
  const out = { piece, lods: {} };
  for (const mesh of doc.getRoot().listMeshes()) {
    if (!/_lod\d$/.test(mesh.getName())) continue;
    const bands = BANDS.map((b) => ({ b, s: 0, z: 0, n: 0 }));
    for (const prim of mesh.listPrimitives()) {
      const mat = prim.getMaterial()?.getName() ?? "";
      if (mat.includes("skirt")) continue;
      const P = prim.getAttribute("POSITION"), N = prim.getAttribute("NORMAL");
      const p = [0, 0, 0], q = [0, 0, 0];
      for (let i = 0; i < P.getCount(); i++) {
        P.getElement(i, p); N.getElement(i, q);
        const bx = p[0], by = -p[2], bz = p[1], nx = q[0], ny = -q[2], nz = q[1];
        if (bz < -0.5 || !LAND[piece](bx, by)) continue;
        let best = -1e9, k = 0;
        for (let e = 0; e < 6; e++) {
          const a = (-60 * e * Math.PI) / 180, v = bx * Math.cos(a) + by * Math.sin(a);
          if (v > best) { best = v; k = e; }
        }
        const d = A - best, a = (-60 * k * Math.PI) / 180;
        const toward = -(nx * Math.cos(a) + ny * Math.sin(a)) / nz;
        for (const band of bands) if (d >= band.b[0] && d < band.b[1]) { band.s += toward; band.z += bz; band.n++; }
      }
    }
    out.lods[mesh.getName().slice(-4)] = bands.filter((x) => x.n).map((x) => ({
      band: x.b, leanDeg: +(Math.atan(x.s / x.n) * 180 / Math.PI).toFixed(2), meanZ: +(x.z / x.n).toFixed(3), n: x.n,
    }));
  }
  for (const [lod, rows] of Object.entries(out.lods)) {
    console.log(`${piece} ${lod}: ` + rows.map((r) => `[${r.band[0]},${r.band[1]}) ${r.leanDeg.toFixed(2)} deg z ${r.meanZ.toFixed(3)} n=${r.n}`).join("  "));
  }
  console.log("JSON " + JSON.stringify(out));
}
