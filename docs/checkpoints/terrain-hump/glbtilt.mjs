// Shipped (meshopt) GLB: mean slope toward the border of the cap's NORMAL attribute, by distance from
// the border, per LOD. Usage: node glbtilt.mjs <repo> <glb> <piece>
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

const [repo, glb, piece] = process.argv.slice(2);
const require = createRequire(pathToFileURL(`${repo}/scripts/assets/package.json`).href);
const { NodeIO } = require("@gltf-transform/core");
const { ALL_EXTENSIONS } = require("@gltf-transform/extensions");
const { MeshoptDecoder } = require("meshoptimizer");
await MeshoptDecoder.ready;
const io = new NodeIO().registerExtensions(ALL_EXTENSIONS).registerDependencies({ "meshopt.decoder": MeshoptDecoder });
const doc = await io.read(glb);
const AP = 16 * Math.sqrt(3) / 2;
for (const mesh of doc.getRoot().listMeshes()) {
  if (!/_lod\d$/.test(mesh.getName())) continue;
  const bands = [[0, 0.01], [0.01, 0.5], [0.5, 1], [1, 1.5], [1.5, 2], [2, 4], [4, 14]].map((b) => ({ b, s: 0, n: 0 }));
  for (const prim of mesh.listPrimitives()) {
    const mat = prim.getMaterial()?.getName() ?? "";
    if (mat.includes("skirt")) continue;
    const P = prim.getAttribute("POSITION"), N = prim.getAttribute("NORMAL");
    const p = [0, 0, 0], q = [0, 0, 0];
    for (let i = 0; i < P.getCount(); i++) {
      P.getElement(i, p); N.getElement(i, q);
      const bx = p[0], by = -p[2], nx = q[0], ny = -q[2], nz = q[1];
      let best = -1e9, k = 0;
      for (let e = 0; e < 6; e++) {
        const a = (-60 * e * Math.PI) / 180, v = bx * Math.cos(a) + by * Math.sin(a);
        if (v > best) { best = v; k = e; }
      }
      const d = AP - best, a = (-60 * k * Math.PI) / 180;
      const toward = -(nx * Math.cos(a) + ny * Math.sin(a)) / nz;  // surface slope toward the border
      for (const band of bands) if (d >= band.b[0] && d < band.b[1]) { band.s += toward; band.n++; }
    }
  }
  console.log(`${mesh.getName()}: ` + bands.filter((x) => x.n).map((x) =>
    `[${x.b[0]},${x.b[1]}) ${(Math.atan(x.s / x.n) * 180 / Math.PI).toFixed(2)} deg n=${x.n}`).join("  "));
}
