// Builds the P0 test hex (plain + meshopt GLB) and its procedural albedo PNG. Run from the repo root.
import { Document, NodeIO } from "@gltf-transform/core";
import { EXTMeshoptCompression } from "@gltf-transform/extensions";
import { reorder } from "@gltf-transform/functions";
import { MeshoptEncoder } from "meshoptimizer";
import sharp from "sharp";
import { mkdir } from "node:fs/promises";

const R = 16, H = 4, SEG = 24; // subdivided top cap so the mesh is big enough for compression to matter
const pos = [], nrm = [], uv = [], idx = [];
for (let ring = 0; ring <= SEG; ring++) {
  for (let k = 0; k < 6 * Math.max(ring, 1); k++) {
    const t = ring === 0 ? 0 : k / (6 * ring);
    const a = (30 + 360 * t) * Math.PI / 180;
    const rad = (ring / SEG) * R;
    pos.push(rad * Math.cos(a), H, rad * Math.sin(a)); nrm.push(0, 1, 0);
    uv.push(0.5 + rad * Math.cos(a) / (2 * R), 0.5 + rad * Math.sin(a) / (2 * R));
    if (ring === 0) break;
  }
}
// Triangulate rings: connect ring r-1 and ring r by nearest-angle walking.
const ringStart = (r) => (r === 0 ? 0 : 1 + 3 * (r - 1) * r);
const ringLen = (r) => (r === 0 ? 1 : 6 * r);
for (let r = 1; r <= SEG; r++) {
  const a0 = ringStart(r - 1), n0 = ringLen(r - 1), a1 = ringStart(r), n1 = ringLen(r);
  let i = 0, j = 0;
  while (i < n0 || j < n1) {
    const ti = n0 === 1 ? 1 : (i + 0.5) / n0, tj = (j + 0.5) / n1;
    if (j < n1 && (i >= n0 || tj <= ti)) { idx.push(a0 + (i % n0), a1 + ((j + 1) % n1), a1 + j); j++; }
    else { idx.push(a0 + (i % n0), a0 + ((i + 1) % n0), a1 + (j % n1)); i++; }
  }
}

function build() {
  const doc = new Document();
  const buf = doc.createBuffer();
  const prim = doc.createPrimitive()
    .setAttribute("POSITION", doc.createAccessor().setType("VEC3").setArray(new Float32Array(pos)).setBuffer(buf))
    .setAttribute("NORMAL", doc.createAccessor().setType("VEC3").setArray(new Float32Array(nrm)).setBuffer(buf))
    .setAttribute("TEXCOORD_0", doc.createAccessor().setType("VEC2").setArray(new Float32Array(uv)).setBuffer(buf))
    .setIndices(doc.createAccessor().setType("SCALAR").setArray(new Uint32Array(idx)).setBuffer(buf));
  doc.createScene().addChild(doc.createNode("hex_test").setMesh(doc.createMesh("hex_test").addPrimitive(prim)));
  return doc;
}

await MeshoptEncoder.ready;
await mkdir("assets/kit/p0", { recursive: true });
await mkdir("crates/hex-assets/tests/fixtures", { recursive: true });
const io = new NodeIO().registerExtensions([EXTMeshoptCompression]).registerDependencies({ "meshopt.encoder": MeshoptEncoder });

const plain = build();
await reorder({ encoder: MeshoptEncoder })(plain);
await io.write("crates/hex-assets/tests/fixtures/hex_test.plain.glb", plain);

const packed = build();
await reorder({ encoder: MeshoptEncoder })(packed);
packed.createExtension(EXTMeshoptCompression).setRequired(true)
  .setEncoderOptions({ method: EXTMeshoptCompression.EncoderMethod.QUANTIZE });
await io.write("assets/kit/p0/hex_test.glb", packed);
await io.write("crates/hex-assets/tests/fixtures/hex_test.meshopt.glb", packed);

const S = 1024, px = Buffer.alloc(S * S * 4);
for (let y = 0; y < S; y++) for (let x = 0; x < S; x++) {
  const n = (Math.sin(x * 0.05) * Math.cos(y * 0.07) + Math.sin((x + y) * 0.013)) * 0.25 + 0.5;
  const o = (y * S + x) * 4;
  px[o] = 60 + 70 * n; px[o + 1] = 110 + 80 * n; px[o + 2] = 50 + 40 * n; px[o + 3] = 255;
}
await sharp(px, { raw: { width: S, height: S, channels: 4 } }).png().toFile("assets/kit/p0/hex_test.png");
console.log("p0 assets written");
