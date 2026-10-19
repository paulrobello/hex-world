// Masks centroid sampler on a reimported kit GLB (meshopt or plain), as kit_gate's hooks test samples:
// the masks PNG read nearest at every triangle's UV0 centroid, per LOD mesh `<id>_lod<n>`. A triangle
// is "unbaked" when its centroid texel has G = 0 (every house-family slot has porosity >= 0.05, so the
// bake never leaves G at 0 where it wrote), which draws black (A = 0) and takes no snow (R = 0).
// Also reports the masks density per LOD and per slot: px/m = sqrt(uv area x w x h / world area).
// Usage: node docs/checkpoints/houses-masks-fill/masks_centroids.mjs <piece.glb> <piece.masks.png> [...pairs]
// Exits 1 when any LOD of any piece has a triangle on an unbaked texel.
import { createRequire } from "node:module";
import { basename } from "node:path";

const require = createRequire(new URL("../../../scripts/assets/package.json", import.meta.url));
const { NodeIO } = require("@gltf-transform/core");
const { EXTMeshoptCompression } = require("@gltf-transform/extensions");
const { MeshoptDecoder } = require("meshoptimizer");
const sharp = require("sharp");

await MeshoptDecoder.ready;
const io = new NodeIO().registerExtensions([EXTMeshoptCompression]).registerDependencies({ "meshopt.decoder": MeshoptDecoder });

const median = (v) => {
  const s = [...v].sort((a, b) => a - b);
  return s.length ? s[Math.floor(s.length / 2)] : null;
};
const r2 = (x) => (x == null ? null : Math.round(x * 100) / 100);

async function sample(glbPath, masksPath) {
  const piece = basename(glbPath, ".glb");
  const { data, info } = await sharp(masksPath).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
  const { width: w, height: h, channels } = info;
  const doc = await io.read(glbPath);
  const out = {};
  let bad = 0;
  for (let lod = 0; lod < 3; lod++) {
    const mesh = doc.getRoot().listMeshes().find((m) => m.getName() === `${piece}_lod${lod}`);
    if (!mesh) throw new Error(`${glbPath}: mesh ${piece}_lod${lod} missing`);
    const r = { tris: 0, unbakedTris: 0, zeroATris: 0, unbakedAreaM2: 0, bySlot: {}, densityPxPerM: null };
    const dens = [];
    for (const prim of mesh.listPrimitives()) {
      const slot = (prim.getMaterial()?.getName() ?? "").slice(piece.length + 1);
      const uv = prim.getAttribute("TEXCOORD_0").getArray();
      const pos = prim.getAttribute("POSITION").getArray();
      const idx = prim.getIndices()?.getArray() ?? Uint32Array.from({ length: pos.length / 3 }, (_, i) => i);
      const s = (r.bySlot[slot] ??= { tris: 0, unbaked: 0, zeroA: 0, minA: 1, density: [] });
      for (let t = 0; t < idx.length; t += 3) {
        const [a, b, c] = [idx[t], idx[t + 1], idx[t + 2]];
        const cu = (uv[2 * a] + uv[2 * b] + uv[2 * c]) / 3;
        const cv = (uv[2 * a + 1] + uv[2 * b + 1] + uv[2 * c + 1]) / 3;
        const x = Math.min(Math.floor((((cu % 1) + 1) % 1) * w), w - 1);
        const y = Math.min(Math.floor((((cv % 1) + 1) % 1) * h), h - 1);
        const o = (y * w + x) * channels;
        const [G, A] = [data[o + 1], data[o + 3]];
        const P = (i) => [pos[3 * i], pos[3 * i + 1], pos[3 * i + 2]];
        const [p0, p1, p2] = [P(a), P(b), P(c)];
        const e1 = p1.map((v, k) => v - p0[k]);
        const e2 = p2.map((v, k) => v - p0[k]);
        const cr = [e1[1] * e2[2] - e1[2] * e2[1], e1[2] * e2[0] - e1[0] * e2[2], e1[0] * e2[1] - e1[1] * e2[0]];
        const area = Math.hypot(...cr) / 2;
        const uvArea = Math.abs((uv[2 * b] - uv[2 * a]) * (uv[2 * c + 1] - uv[2 * a + 1]) - (uv[2 * c] - uv[2 * a]) * (uv[2 * b + 1] - uv[2 * a + 1])) / 2;
        r.tris++;
        s.tris++;
        if (G === 0) {
          r.unbakedTris++;
          r.unbakedAreaM2 += area;
          s.unbaked++;
        }
        if (A === 0) {
          r.zeroATris++;
          s.zeroA++;
        }
        s.minA = Math.min(s.minA, A / 255);
        if (area > 1e-6) {
          const d = Math.sqrt((uvArea * w * h) / area);
          dens.push(d);
          s.density.push(d);
        }
      }
    }
    r.unbakedAreaM2 = r2(r.unbakedAreaM2);
    r.densityPxPerM = { min: r2(Math.min(...dens)), median: r2(median(dens)), max: r2(Math.max(...dens)) };
    for (const s of Object.values(r.bySlot)) {
      s.minA = r2(s.minA);
      s.density = r2(median(s.density));
    }
    bad += r.unbakedTris;
    out[`lod${lod}`] = r;
  }
  return { piece, masks: `${w}x${h}`, unbakedTotal: bad, lods: out };
}

const args = process.argv.slice(2);
if (args.length < 2 || args.length % 2) {
  console.error("usage: node masks_centroids.mjs <piece.glb> <piece.masks.png> [...pairs]");
  process.exit(2);
}
let fail = 0;
const res = [];
for (let i = 0; i < args.length; i += 2) {
  const r = await sample(args[i], args[i + 1]);
  fail += r.unbakedTotal;
  res.push(r);
}
console.log(JSON.stringify(res, null, 1));
process.exit(fail ? 1 : 0);
