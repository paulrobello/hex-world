// Lists LOD triangles whose masks centroid reads A < 0.1 on texels the bake DID write (G > 0), and
// checks whether each sits buried: its centroid within `tol` of another triangle of the same LOD
// (touching or inside another member), or facing down under the eave line. A buried face's AO 0 is
// correct (no sky reaches it) and it is never seen.
// Usage: node docs/checkpoints/houses-masks-fill/dark_centroids.mjs <piece.glb> <piece.masks.png> [lod] [tol m]
import { createRequire } from "node:module";
import { basename } from "node:path";

const require = createRequire(new URL("../../../scripts/assets/package.json", import.meta.url));
const { NodeIO } = require("@gltf-transform/core");
const { EXTMeshoptCompression } = require("@gltf-transform/extensions");
const { MeshoptDecoder } = require("meshoptimizer");
const sharp = require("sharp");

await MeshoptDecoder.ready;
const io = new NodeIO().registerExtensions([EXTMeshoptCompression]).registerDependencies({ "meshopt.decoder": MeshoptDecoder });
const [glb, png, lodArg, tolArg] = process.argv.slice(2);
const lod = Number(lodArg ?? 0);
const tol = Number(tolArg ?? 0.01);
const piece = basename(glb, ".glb");
const { data, info } = await sharp(png).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
const doc = await io.read(glb);
const mesh = doc.getRoot().listMeshes().find((m) => m.getName() === `${piece}_lod${lod}`);
const tris = [];
for (const prim of mesh.listPrimitives()) {
  const slot = (prim.getMaterial()?.getName() ?? "").slice(piece.length + 1);
  const p = prim.getAttribute("POSITION").getArray();
  const uv = prim.getAttribute("TEXCOORD_0").getArray();
  const idx = prim.getIndices().getArray();
  for (let t = 0; t < idx.length; t += 3) {
    const v = [idx[t], idx[t + 1], idx[t + 2]].map((i) => [p[3 * i], p[3 * i + 1], p[3 * i + 2]]);
    const c = [0, 1].map((k) => (uv[2 * idx[t] + k] + uv[2 * idx[t + 1] + k] + uv[2 * idx[t + 2] + k]) / 3);
    const x = Math.min(Math.floor(c[0] * info.width), info.width - 1);
    const y = Math.min(Math.floor(c[1] * info.height), info.height - 1);
    const o = (y * info.width + x) * info.channels;
    tris.push({ slot, v, G: data[o + 1], A: data[o + 3] / 255 });
  }
}
const sub = (a, b) => a.map((x, i) => x - b[i]);
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
// Point-triangle distance (Ericson, Real-Time Collision Detection 5.1.5).
function dist(p, [a, b, c]) {
  const ab = sub(b, a), ac = sub(c, a), ap = sub(p, a);
  const d1 = dot(ab, ap), d2 = dot(ac, ap);
  let q;
  if (d1 <= 0 && d2 <= 0) q = a;
  else {
    const bp = sub(p, b), d3 = dot(ab, bp), d4 = dot(ac, bp);
    if (d3 >= 0 && d4 <= d3) q = b;
    else {
      const vc = d1 * d4 - d3 * d2;
      if (vc <= 0 && d1 >= 0 && d3 <= 0) q = a.map((x, i) => x + ab[i] * (d1 / (d1 - d3)));
      else {
        const cp = sub(p, c), d5 = dot(ab, cp), d6 = dot(ac, cp);
        if (d6 >= 0 && d5 <= d6) q = c;
        else {
          const vb = d5 * d2 - d1 * d6;
          if (vb <= 0 && d2 >= 0 && d6 <= 0) q = a.map((x, i) => x + ac[i] * (d2 / (d2 - d6)));
          else {
            const va = d3 * d6 - d5 * d4;
            if (va <= 0 && d4 - d3 >= 0 && d5 - d6 >= 0) {
              const w = (d4 - d3) / (d4 - d3 + (d5 - d6));
              q = b.map((x, i) => x + (c[i] - x) * w);
            } else {
              const den = 1 / (va + vb + vc);
              const v = vb * den, w = vc * den;
              q = a.map((x, i) => x + ab[i] * v + ac[i] * w);
            }
          }
        }
      }
    }
  }
  return Math.hypot(...sub(p, q));
}
const dark = tris.filter((t) => t.G > 0 && t.A < 0.1);
const bySlot = {};
for (const t of dark) {
  const cen = [0, 1, 2].map((k) => (t.v[0][k] + t.v[1][k] + t.v[2][k]) / 3);
  const n = cross(sub(t.v[1], t.v[0]), sub(t.v[2], t.v[0]));
  const nl = Math.hypot(...n);
  const nz = nl ? n[1] / nl : 0; // runtime +Y is up
  let near = Infinity;
  for (const u of tris) if (u !== t) near = Math.min(near, dist(cen, u.v));
  const s = (bySlot[t.slot] ??= { dark: 0, touching: 0, facingDown: 0, open: [] });
  s.dark++;
  if (near <= tol) s.touching++;
  else if (nz < -0.5) s.facingDown++;
  else if (s.open.length < 6) s.open.push({ centroid: cen.map((x) => +x.toFixed(3)), normalY: +nz.toFixed(2), nearestM: +near.toFixed(3), A: +t.A.toFixed(3) });
}
console.log(JSON.stringify({ piece, lod, tol, darkTris: dark.length, bySlot }, null, 1));
