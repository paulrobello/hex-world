// Compares fence_paddock's runtime (meshopt-compressed) GLB at a git revision against the working tree,
// mesh by mesh on decoded values: for each <piece>_lod<n> and the collider, triangle counts and whether
// POSITION, NORMAL, TANGENT, TEXCOORD_0 and the indices are identical. The LOD2 fix must leave LOD0, LOD1
// and the collider identical (adapted from docs/checkpoints/animal-lod2-head/compare_compressed.mjs).
//
// Usage (from the repo root): node docs/checkpoints/fence-lod2/compare_lods.mjs <rev> [piece]
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
      const d = { material: p.getMaterial()?.getName() ?? null };
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

const eq = (a, b) => (Array.isArray(a) ? a.length === b.length && a.every((v, i) => v === b[i]) : a === b);
const tris = (prims) => prims.reduce((s, p) => s + p.indices.length / 3, 0);

const [rev, piece = "fence_paddock"] = process.argv.slice(2);
const rel = `assets/kit/p1/${piece}/${piece}.glb`;
const A = await meshes(execFileSync("git", ["show", `${rev}:${rel}`], { maxBuffer: 1 << 26 }));
const B = await meshes(readFileSync(rel));
let changed = [];
for (const n of [`${piece}_lod0`, `${piece}_lod1`, `${piece}_lod2`, `${piece}_collider`]) {
  const pa = A[n], pb = B[n];
  const same = pa.length === pb.length && pa.every((x, i) => {
    const y = pb[i];
    const keys = Object.keys(x);
    return eq(keys, Object.keys(y)) && keys.every((k) => eq(x[k], y[k]));
  });
  if (!same) changed.push(n);
  console.log(`${n}: tris ${tris(pa)} -> ${tris(pb)}, identical ${same}`);
}
console.log(`changed: ${changed.join(", ") || "none"}`);
