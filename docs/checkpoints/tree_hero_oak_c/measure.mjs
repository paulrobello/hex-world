// Measure a kit GLB (staged, or meshopt-compressed as shipped): per mesh max and min height, radial
// reach from the trunk axis and tris (the hero-variant caps: height for Q49, reach against FOOTPRINTS),
// and optionally dump the LOD0 canopy, leaf and bark triangles (runtime frame) as JSON for perches.py.
// Usage: node docs/checkpoints/tree_hero_oak_c/measure.mjs <glb> [dump.json]
import { createRequire } from "node:module";
import fs from "node:fs";
const require = createRequire(new URL("../../../scripts/assets/package.json", import.meta.url));
const { NodeIO } = require("@gltf-transform/core");
const { ALL_EXTENSIONS } = require("@gltf-transform/extensions");
const { MeshoptDecoder } = require("meshoptimizer");

const [glb, dump] = process.argv.slice(2);
await MeshoptDecoder.ready;
const io = new NodeIO().registerExtensions(ALL_EXTENSIONS).registerDependencies({ "meshopt.decoder": MeshoptDecoder });
const doc = await io.read(glb);
const out = { meshes: {}, tris: {} };
for (const mesh of doc.getRoot().listMeshes()) {
  let ymax = -1e9, ymin = 1e9, reach = 0, tris = 0;
  for (const prim of mesh.listPrimitives()) {
    const pos = prim.getAttribute("POSITION");
    const idx = prim.getIndices();
    const n = pos.getCount();
    const p = [0, 0, 0];
    for (let i = 0; i < n; i++) {
      pos.getElement(i, p);
      ymax = Math.max(ymax, p[1]);
      ymin = Math.min(ymin, p[1]);
      reach = Math.max(reach, Math.hypot(p[0], p[2]));
    }
    tris += (idx ? idx.getCount() : n) / 3;
    const mat = prim.getMaterial()?.getName() ?? "";
    if (dump && mesh.getName().endsWith("_lod0") && (mat.endsWith("_canopy") || mat.endsWith("_leaves") || mat.endsWith("_bark"))) {
      const arr = [];
      const a = [0, 0, 0];
      const ic = idx.getCount();
      for (let k = 0; k < ic; k++) {
        pos.getElement(idx.getScalar(k), a);
        arr.push(a[0], a[1], a[2]);
      }
      out.tris[mat.endsWith("_canopy") ? "canopy" : mat.endsWith("_bark") ? "bark" : "leaves"] = arr;
    }
  }
  out.meshes[mesh.getName()] = { ymax: +ymax.toFixed(3), ymin: +ymin.toFixed(3), reach: +reach.toFixed(3), tris };
}
console.log(JSON.stringify(out.meshes, null, 1));
if (dump) fs.writeFileSync(dump, JSON.stringify(out.tris));
