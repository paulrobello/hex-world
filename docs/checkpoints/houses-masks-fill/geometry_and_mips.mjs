// Two checks for the houses masks fill, before vs after:
// 1. Geometry: per LOD mesh, the sorted set of vertex positions (rounded to 1e-5 m) and the triangle
//    count must match between the two GLBs (the fix may move UV0, never positions).
// 2. Mips: the masks PNG box-filtered (2x2 mean on raw RGBA data) to mips 0-3, sampled nearest at every
//    LOD triangle's UV0 centroid; per slot, mean A and the share of triangles with A < 0.1, so a smaller
//    island gap or bake margin that lets the empty background bleed into an island shows as a darker slot.
// Usage: node docs/checkpoints/houses-masks-fill/geometry_and_mips.mjs <before.glb> <before.masks.png> <after.glb> <after.masks.png>
import { createRequire } from "node:module";
import { basename } from "node:path";

const require = createRequire(new URL("../../../scripts/assets/package.json", import.meta.url));
const { NodeIO } = require("@gltf-transform/core");
const { EXTMeshoptCompression } = require("@gltf-transform/extensions");
const { MeshoptDecoder } = require("meshoptimizer");
const sharp = require("sharp");

await MeshoptDecoder.ready;
const io = new NodeIO().registerExtensions([EXTMeshoptCompression]).registerDependencies({ "meshopt.decoder": MeshoptDecoder });

async function load(glb, png) {
  const piece = basename(glb, ".glb");
  const doc = await io.read(glb);
  const { data, info } = await sharp(png).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
  const mips = [{ w: info.width, h: info.height, px: Float32Array.from(data, (v) => v / 255) }];
  for (let m = 1; m <= 3; m++) {
    const { w, h, px } = mips[m - 1];
    const [nw, nh] = [w / 2, h / 2];
    const out = new Float32Array(nw * nh * 4);
    for (let y = 0; y < nh; y++)
      for (let x = 0; x < nw; x++)
        for (let c = 0; c < 4; c++)
          out[(y * nw + x) * 4 + c] =
            (px[((2 * y) * w + 2 * x) * 4 + c] + px[((2 * y) * w + 2 * x + 1) * 4 + c] +
              px[((2 * y + 1) * w + 2 * x) * 4 + c] + px[((2 * y + 1) * w + 2 * x + 1) * 4 + c]) / 4;
    mips.push({ w: nw, h: nh, px: out });
  }
  const lods = [];
  for (let lod = 0; lod < 3; lod++) {
    const mesh = doc.getRoot().listMeshes().find((x) => x.getName() === `${piece}_lod${lod}`);
    const pos = new Set();
    let tris = 0;
    const cents = [];
    for (const prim of mesh.listPrimitives()) {
      const slot = (prim.getMaterial()?.getName() ?? "").slice(piece.length + 1);
      const p = prim.getAttribute("POSITION").getArray();
      const uv = prim.getAttribute("TEXCOORD_0").getArray();
      const idx = prim.getIndices().getArray();
      for (let i = 0; i < p.length; i += 3) pos.add([p[i], p[i + 1], p[i + 2]].map((v) => Math.round(v * 1e5)).join(","));
      for (let t = 0; t < idx.length; t += 3) {
        tris++;
        const [a, b, c] = [idx[t], idx[t + 1], idx[t + 2]];
        cents.push([slot, (uv[2 * a] + uv[2 * b] + uv[2 * c]) / 3, (uv[2 * a + 1] + uv[2 * b + 1] + uv[2 * c + 1]) / 3]);
      }
    }
    // Area-weighted: every mip-0 texel centre inside a slot's triangles, read at mip m (x >> m, y >> m),
    // so a slot's mean is what its whole surface shows at that mip, not a few centroid samples.
    const W0 = mips[0].w;
    const cover = {};
    for (const prim of mesh.listPrimitives()) {
      const slot = (prim.getMaterial()?.getName() ?? "").slice(piece.length + 1);
      const uv = prim.getAttribute("TEXCOORD_0").getArray();
      const idx = prim.getIndices().getArray();
      const set = (cover[slot] ??= new Set());
      for (let t = 0; t < idx.length; t += 3) {
        const P = [idx[t], idx[t + 1], idx[t + 2]].map((i) => [uv[2 * i] * W0, uv[2 * i + 1] * W0]);
        const d = (P[1][1] - P[2][1]) * (P[0][0] - P[2][0]) + (P[2][0] - P[1][0]) * (P[0][1] - P[2][1]);
        if (Math.abs(d) < 1e-12) continue;
        const x0 = Math.max(0, Math.floor(Math.min(P[0][0], P[1][0], P[2][0])));
        const x1 = Math.min(W0 - 1, Math.ceil(Math.max(P[0][0], P[1][0], P[2][0])));
        const y0 = Math.max(0, Math.floor(Math.min(P[0][1], P[1][1], P[2][1])));
        const y1 = Math.min(W0 - 1, Math.ceil(Math.max(P[0][1], P[1][1], P[2][1])));
        for (let y = y0; y <= y1; y++)
          for (let x = x0; x <= x1; x++) {
            const [px_, py_] = [x + 0.5, y + 0.5];
            const l1 = ((P[1][1] - P[2][1]) * (px_ - P[2][0]) + (P[2][0] - P[1][0]) * (py_ - P[2][1])) / d;
            const l2 = ((P[2][1] - P[0][1]) * (px_ - P[2][0]) + (P[0][0] - P[2][0]) * (py_ - P[2][1])) / d;
            if (l1 >= 0 && l2 >= 0 && 1 - l1 - l2 >= 0) set.add(y * W0 + x);
          }
      }
    }
    const areaMip = mips.map(({ w, px }, m) =>
      Object.fromEntries(Object.entries(cover).map(([k, set]) => {
        let s = 0;
        for (const i of set) s += px[(((Math.floor(i / W0)) >> m) * w + ((i % W0) >> m)) * 4 + 3];
        return [k, set.size ? +(s / set.size).toFixed(3) : null];
      })));
    const perMip = mips.map(({ w, h, px }) => {
      const s = {};
      for (const [slot, u, v] of cents) {
        const x = Math.min(Math.floor(u * w), w - 1);
        const y = Math.min(Math.floor(v * h), h - 1);
        const A = px[(y * w + x) * 4 + 3];
        const r = (s[slot] ??= { n: 0, sumA: 0, dark: 0 });
        r.n++;
        r.sumA += A;
        r.dark += A < 0.1 ? 1 : 0;
      }
      return Object.fromEntries(Object.entries(s).map(([k, r]) => [k, { meanA: +(r.sumA / r.n).toFixed(3), darkShare: +(r.dark / r.n).toFixed(3) }]));
    });
    lods.push({ tris, pos: [...pos].sort(), perMip, areaMip });
  }
  return { piece, lods };
}

const [bg, bp, ag, ap] = process.argv.slice(2);
const before = await load(bg, bp);
const after = await load(ag, ap);
let worst = 0;
let worstArea = 0;
const report = { piece: after.piece, lods: [] };
for (let lod = 0; lod < 3; lod++) {
  const [b, a] = [before.lods[lod], after.lods[lod]];
  const samePos = b.pos.length === a.pos.length && b.pos.every((v, i) => v === a.pos[i]);
  const mips = b.perMip.map((bm, m) => {
    const am = a.perMip[m];
    const d = {};
    for (const k of Object.keys(am)) {
      const delta = +(am[k].meanA - bm[k].meanA).toFixed(3);
      worst = Math.min(worst, delta);
      d[k] = { before: bm[k].meanA, after: am[k].meanA, delta, darkBefore: bm[k].darkShare, darkAfter: am[k].darkShare };
    }
    return d;
  });
  const areaMips = b.areaMip.map((bm, m) =>
    Object.fromEntries(Object.keys(a.areaMip[m]).map((k) => {
      const delta = +(a.areaMip[m][k] - bm[k]).toFixed(3);
      worstArea = Math.min(worstArea, delta);
      return [k, { before: bm[k], after: a.areaMip[m][k], delta }];
    })));
  report.lods.push({ lod, trisBefore: b.tris, trisAfter: a.tris, positionsBefore: b.pos.length, positionsAfter: a.pos.length, samePositions: samePos, mips, areaMips });
}
report.worstMeanADelta = worst;
report.worstAreaMeanADelta = worstArea;
console.log(JSON.stringify(report, null, 1));
