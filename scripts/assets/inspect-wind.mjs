// Prints TEXCOORD_0/1 presence and ranges per mesh primitive of a (meshopt or plain) GLB.
// Usage: node scripts/assets/inspect-wind.mjs <file.glb>
import { NodeIO } from "@gltf-transform/core";
import { EXTMeshoptCompression } from "@gltf-transform/extensions";
import { MeshoptDecoder } from "meshoptimizer";

await MeshoptDecoder.ready;
const io = new NodeIO().registerExtensions([EXTMeshoptCompression]).registerDependencies({ "meshopt.decoder": MeshoptDecoder });
const doc = await io.read(process.argv[2]);
const out = {};
for (const mesh of doc.getRoot().listMeshes()) {
  mesh.listPrimitives().forEach((prim, i) => {
    const r = {};
    for (const sem of ["TEXCOORD_0", "TEXCOORD_1", "COLOR_0"]) {
      const acc = prim.getAttribute(sem);
      if (!acc) { r[sem] = null; continue; }
      const a = acc.getArray();
      const lo = [Infinity, Infinity], hi = [-Infinity, -Infinity];
      for (let k = 0; k < a.length; k += 2) for (const c of [0, 1]) { lo[c] = Math.min(lo[c], a[k + c]); hi[c] = Math.max(hi[c], a[k + c]); }
      r[sem] = { type: acc.getComponentType(), count: acc.getCount(), min: lo.map((x) => +x.toFixed(4)), max: hi.map((x) => +x.toFixed(4)) };
    }
    const pos = prim.getAttribute("POSITION").getArray();
    const uv1 = prim.getAttribute("TEXCOORD_1")?.getArray();
    if (uv1) {
      let minY = Infinity;
      for (let k = 1; k < pos.length; k += 3) minY = Math.min(minY, pos[k]);
      let maxU = 0;
      for (let k = 0; k < pos.length / 3; k++) if (pos[3 * k + 1] < minY + 0.3) maxU = Math.max(maxU, uv1[2 * k]);
      r.lowestVertsMaxU = +maxU.toFixed(4);
    }
    out[`${mesh.getName()}#${i}:${prim.getMaterial()?.getName()}`] = r;
  });
}
console.log(JSON.stringify(out, null, 1));
