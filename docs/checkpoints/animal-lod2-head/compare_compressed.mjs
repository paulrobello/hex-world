// Compares a piece's runtime (meshopt-compressed) GLB at a git revision against the working tree, mesh
// by mesh on decoded values: for every <piece>_lod<n> and the collider, whether POSITION, NORMAL,
// TANGENT, TEXCOORD_0, TEXCOORD_1 and the indices are identical, plus triangle counts and the
// collider's bounds and ground reach (largest horizontal distance of a collider vertex from the pivot).
//
// Usage (from the repo root): node docs/checkpoints/animal-lod2-head/compare_compressed.mjs <rev> <piece>...
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(new URL("../../../scripts/assets/package.json", import.meta.url));
const { NodeIO } = require("@gltf-transform/core");
const { EXTMeshoptCompression } = require("@gltf-transform/extensions");
const { MeshoptDecoder } = require("meshoptimizer");

await MeshoptDecoder.ready;
const io = new NodeIO().registerExtensions([EXTMeshoptCompression]).registerDependencies({ "meshopt.decoder": MeshoptDecoder });
const ATTRS = ["POSITION", "NORMAL", "TANGENT", "TEXCOORD_0", "TEXCOORD_1"];

async function meshes(bytes) {
  const doc = await io.readBinary(new Uint8Array(bytes));
  const out = {};
  for (const m of doc.getRoot().listMeshes()) {
    out[m.getName()] = m.listPrimitives().map((p) => {
      const d = {};
      for (const a of ATTRS) {
        const acc = p.getAttribute(a);
        if (acc) d[a] = Array.from(acc.getArray());
      }
      d.indices = Array.from(p.getIndices().getArray());
      return d;
    });
  }
  return out;
}

const eq = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);
const tris = (prims) => prims.reduce((s, p) => s + p.indices.length / 3, 0);

const [rev, ...pieces] = process.argv.slice(2);
for (const piece of pieces) {
  const rel = `assets/kit/p1/${piece}/${piece}.glb`;
  const A = await meshes(execFileSync("git", ["show", `${rev}:${rel}`], { maxBuffer: 1 << 26 }));
  const B = await meshes(readFileSync(rel));
  for (let lv = 0; lv < 3; lv++) {
    const n = `${piece}_lod${lv}`;
    const pa = A[n], pb = B[n];
    const same = pa.length === pb.length && pa.every((x, i) => {
      const y = pb[i];
      const keys = Object.keys(x);
      return eq(keys, Object.keys(y)) && keys.every((k) => eq(x[k], y[k]));
    });
    console.log(`${n}: tris ${tris(pa)} -> ${tris(pb)}, identical ${same}`);
  }
  for (const [label, M] of [["before", A], ["after", B]]) {
    const P = M[`${piece}_collider`].flatMap((p) => p.POSITION);
    const lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
    let reach = 0;
    for (let i = 0; i < P.length; i += 3) {
      for (let k = 0; k < 3; k++) { lo[k] = Math.min(lo[k], P[i + k]); hi[k] = Math.max(hi[k], P[i + k]); }
      reach = Math.max(reach, Math.hypot(P[i], P[i + 2]));
    }
    const r = (v) => v.map((x) => +x.toFixed(4));
    console.log(`${piece}_collider ${label}: min ${r(lo)} max ${r(hi)} reach ${reach.toFixed(4)}`);
  }
}
