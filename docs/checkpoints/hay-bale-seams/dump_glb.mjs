// Dumps every `<piece>_lod<n>` primitive of a (meshopt or plain) kit GLB as JSON for measure.py:
// POSITION, NORMAL, TANGENT, TEXCOORD_0 and the triangle indices, in glTF space.
// Usage: node docs/checkpoints/hay-bale-seams/dump_glb.mjs <piece.glb> <out.json>
import { createRequire } from "node:module";
import { writeFileSync } from "node:fs";

const require = createRequire(new URL("../../../scripts/assets/package.json", import.meta.url));
const { NodeIO } = require("@gltf-transform/core");
const { EXTMeshoptCompression } = require("@gltf-transform/extensions");
const { MeshoptDecoder } = require("meshoptimizer");

await MeshoptDecoder.ready;
const io = new NodeIO().registerExtensions([EXTMeshoptCompression]).registerDependencies({ "meshopt.decoder": MeshoptDecoder });
const doc = await io.read(process.argv[2]);
const out = {};
for (const mesh of doc.getRoot().listMeshes()) {
  if (!/_lod\d$/.test(mesh.getName())) continue;
  out[mesh.getName()] = mesh.listPrimitives().map((p) => ({
    material: p.getMaterial()?.getName(),
    position: Array.from(p.getAttribute("POSITION").getArray()),
    normal: Array.from(p.getAttribute("NORMAL").getArray()),
    tangent: Array.from(p.getAttribute("TANGENT").getArray()),
    uv: Array.from(p.getAttribute("TEXCOORD_0").getArray()),
    indices: Array.from(p.getIndices().getArray()),
  }));
}
writeFileSync(process.argv[3], JSON.stringify(out));
console.log(`${process.argv[3]}: ${Object.keys(out).join(", ")}`);
