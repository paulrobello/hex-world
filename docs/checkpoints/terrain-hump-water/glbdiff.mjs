// Geometry-identity check for a normals-only rebuild: compares two shipped (meshopt) GLBs of one piece,
// primitive by primitive (matched by mesh and material name). Order-independent, so a re-compress that
// reorders vertices still compares: the vertex sets (POSITION + TEXCOORD_0 + material, exact float32
// bits), the triangle sets (as position triples) and the node/mesh/material names must be identical; the
// NORMAL attribute is compared per vertex and reported by where it changed (border ring, land part
// beyond the bank-top falloff, elsewhere). Exit 1 on any geometry difference.
// Usage: node glbdiff.mjs <repo> <before.glb> <after.glb>
import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

const [repo, before, after] = process.argv.slice(2);
const require = createRequire(pathToFileURL(`${repo}/scripts/assets/package.json`).href);
const { NodeIO } = require("@gltf-transform/core");
const { ALL_EXTENSIONS } = require("@gltf-transform/extensions");
const { MeshoptDecoder } = require("meshoptimizer");
await MeshoptDecoder.ready;
const io = new NodeIO().registerExtensions(ALL_EXTENSIONS).registerDependencies({ "meshopt.decoder": MeshoptDecoder });
const A = 16 * Math.sqrt(3) / 2;
const f32 = new Float32Array(1), u32 = new Uint32Array(f32.buffer);
const bits = (v) => { f32[0] = v; return u32[0].toString(16); };
const keyOf = (p, uv) => [...p, ...(uv ?? [])].map(bits).join(",");
const dBorder = (x, y) => {
  let best = -1e9;
  for (let e = 0; e < 6; e++) { const a = (-60 * e * Math.PI) / 180; best = Math.max(best, x * Math.cos(a) + y * Math.sin(a)); }
  return A - best;
};

async function load(path) {
  const doc = await io.read(path);
  const prims = new Map();
  for (const mesh of doc.getRoot().listMeshes()) {
    for (const prim of mesh.listPrimitives()) {
      const name = `${mesh.getName()}/${prim.getMaterial()?.getName() ?? ""}`;
      const P = prim.getAttribute("POSITION"), N = prim.getAttribute("NORMAL"), T = prim.getAttribute("TEXCOORD_0");
      const verts = new Map(), pos = [];
      for (let i = 0; i < P.getCount(); i++) {
        const p = P.getElement(i, [0, 0, 0]), uv = T ? T.getElement(i, [0, 0]) : null, n = N ? N.getElement(i, [0, 0, 0]) : null;
        const k = keyOf(p, uv);  // a skirt corner repeats position and UV with split normals
        if (!verts.has(k)) verts.set(k, { p, ns: [] });
        if (n) verts.get(k).ns.push(n);
        pos.push(p.map(bits).join(","));
      }
      const idx = prim.getIndices(), tris = [];
      for (let i = 0; idx && i < idx.getCount(); i += 3) {
        const t = [idx.getScalar(i), idx.getScalar(i + 1), idx.getScalar(i + 2)].map((j) => pos[j]);
        const r = t.indexOf([...t].sort()[0]);  // rotate so the smallest key leads (keeps winding)
        tris.push([t[r], t[(r + 1) % 3], t[(r + 2) % 3]].join("|"));
      }
      prims.set(name, { verts, tris: tris.sort(), count: P.getCount() });
    }
  }
  const nodes = doc.getRoot().listNodes().map((n) => `${n.getName()}@${n.getTranslation().map(bits)}`).sort();
  return { prims, nodes };
}

const a = await load(before), b = await load(after);
let bad = 0;
const fail = (m) => { console.log("GEOMETRY DIFF: " + m); bad++; };
if (a.nodes.join() !== b.nodes.join()) fail("node names or translations differ");
if ([...a.prims.keys()].sort().join() !== [...b.prims.keys()].sort().join()) fail("primitive set differs");
for (const [name, pa] of a.prims) {
  const pb = b.prims.get(name);
  if (!pb) continue;
  if (pa.count !== pb.count) fail(`${name}: vertex count ${pa.count} -> ${pb.count}`);
  if (pa.tris.join() !== pb.tris.join()) fail(`${name}: triangle set differs`);
  const changed = { ring: 0, near: 0, far: 0, max: 0 };
  let same = 0;
  for (const [k, va] of pa.verts) {
    const vb = pb.verts.get(k);
    if (!vb) { fail(`${name}: vertex ${va.p} missing after`); break; }
    if (va.ns.length !== vb.ns.length) { fail(`${name}: vertex ${va.p} split differently`); break; }
    const sa = va.ns.map((n) => n.map(bits).join(",")).sort(), sb = vb.ns.map((n) => n.map(bits).join(",")).sort();
    if (sa.join() === sb.join()) { same++; continue; }
    let deg = 0;
    for (const na of va.ns) {  // largest angle from a before-normal to its nearest after-normal
      let best = 180;
      for (const nb of vb.ns) {
        const dot = na[0] * nb[0] + na[1] * nb[1] + na[2] * nb[2];
        best = Math.min(best, Math.acos(Math.max(-1, Math.min(1, dot))) * 180 / Math.PI);
      }
      deg = Math.max(deg, best);
    }
    const d = dBorder(va.p[0], -va.p[2]);
    changed[d < 0.01 ? "ring" : d < 2.0 ? "near" : "far"]++;
    changed.max = Math.max(changed.max, deg);
  }
  console.log(`${name}: ${pa.count} verts, normals same ${same}, changed: ring ${changed.ring}, `
    + `within 2 m of the border ${changed.near}, beyond ${changed.far}, max ${changed.max.toFixed(2)} deg`);
}
console.log(bad ? `FAIL: ${bad} geometry difference(s)` : "geometry identical (positions, UV0, triangles, nodes)");
process.exit(bad ? 1 : 0);
