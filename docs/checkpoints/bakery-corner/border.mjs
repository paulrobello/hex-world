// Distance (m) from every mesh's vertices to the hex border with the piece in the bakery tile's hero slot
// (data/tiles/bakery.ron: at (-4.1, -8.5), yaw 90; the engine turns a prop by Quat::from_rotation_y(-yaw)),
// per mesh (LOD0-2 and the collider), with the nearest vertex in Blender (x, y, z) and tile (x, z).
// Usage: node docs/checkpoints/bakery-corner/border.mjs <label>=<file.glb> [...]   (meshopt or plain GLBs)
import { createRequire } from "node:module";

const require = createRequire(new URL("../../../scripts/assets/package.json", import.meta.url));
const { NodeIO } = require("@gltf-transform/core");
const { EXTMeshoptCompression } = require("@gltf-transform/extensions");
const { MeshoptDecoder } = require("meshoptimizer");

await MeshoptDecoder.ready;
const io = new NodeIO().registerExtensions([EXTMeshoptCompression]).registerDependencies({ "meshopt.decoder": MeshoptDecoder });
const APOTHEM = 8 * Math.sqrt(3);
const [AX, AZ, YAW] = [-4.1, -8.5, 90];
const th = (-YAW * Math.PI) / 180;
const normals = [0, 1, 2, 3, 4, 5].map((k) => [Math.cos((k * Math.PI) / 3), Math.sin((k * Math.PI) / 3)]);
const r4 = (x) => Math.round(x * 1e4) / 1e4;
const out = {};
for (const arg of process.argv.slice(2)) {
  const [label, file] = arg.split("=");
  const doc = await io.read(file);
  const res = {};
  for (const mesh of doc.getRoot().listMeshes()) {
    let best = { d: Infinity };
    let tris = 0;
    for (const prim of mesh.listPrimitives()) {
      const p = prim.getAttribute("POSITION").getArray();
      tris += prim.getIndices().getCount() / 3;
      for (let i = 0; i < p.length; i += 3) {
        const [gx, gy, gz] = [p[i], p[i + 1], p[i + 2]];
        const tx = AX + gx * Math.cos(th) + gz * Math.sin(th);
        const tz = AZ - gx * Math.sin(th) + gz * Math.cos(th);
        const d = APOTHEM - Math.max(...normals.map(([nx, nz]) => tx * nx + tz * nz));
        if (d < best.d) best = { d, blender: [gx, -gz, gy].map(r4), tile: [tx, tz].map(r4) };
      }
    }
    res[mesh.getName()] = { tris, borderM: r4(best.d), blender: best.blender, tile: best.tile };
  }
  out[label] = res;
}
console.log(JSON.stringify(out, null, 1));
