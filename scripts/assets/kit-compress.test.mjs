// Local-only (needs tools/ktx-4.4.2): node --test scripts/assets/kit-compress.test.mjs
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdir, mkdtemp, readdir, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { Document, NodeIO } from "@gltf-transform/core";
import sharp from "sharp";
import { compressDir, decodeUastc, encodeMap, encodeUastc } from "./kit-compress.mjs";
import { KTX_BUDGETS, ktx2Header, VK } from "./kit-lib.mjs";

async function stage(dir) {
  const doc = new Document();
  const buf = doc.createBuffer();
  const tri = (n, t, a) => doc.createAccessor().setType(t).setArray(a).setBuffer(buf);
  const prim = doc.createPrimitive()
    .setAttribute("POSITION", tri(3, "VEC3", new Float32Array([0, 0, 0, 1, 0, 0, 0, 0, 1])))
    .setAttribute("NORMAL", tri(3, "VEC3", new Float32Array([0, 1, 0, 0, 1, 0, 0, 1, 0])))
    .setAttribute("TANGENT", tri(3, "VEC4", new Float32Array([1, 0, 0, 1, 1, 0, 0, 1, 1, 0, 0, 1])))
    .setAttribute("TEXCOORD_0", tri(3, "VEC2", new Float32Array([0, 0, 1, 0, 0, 1])));
  doc.createScene().addChild(doc.createNode("t").setMesh(doc.createMesh("t_lod0").addPrimitive(prim)));
  await new NodeIO().write(join(dir, "t.glb"), doc);
  const px = Buffer.alloc(64 * 64 * 4, 128);
  for (const ch of ["albedo", "normal", "masks"])
    await sharp(px, { raw: { width: 64, height: 64, channels: 4 } }).png().toFile(join(dir, `t.${ch}.png`));
}

test("linear channels encode UNORM, sRGB channels encode SRGB, full mips, zstd", async () => {
  const dir = await mkdtemp(join(tmpdir(), "kitc-"));
  await stage(dir);
  for (const [ch, srgb] of [["albedo", true], ["normal", false], ["masks", false]]) {
    const outs = await encodeMap(join(dir, `t.${ch}.png`), ch, join(dir, `out.${ch}`));
    for (const o of outs.filter((o) => o.family !== "png")) {
      const h = ktx2Header(await readFile(o.path));
      assert.equal(h.vkFormat, VK[o.family][srgb ? "srgb" : "linear"], `${ch} ${o.family}`);
      assert.equal(h.levels, 7); // 64 -> 1
      assert.equal(h.supercompression, 2); // zstd
    }
  }
});

test("compressDir writes a float-attribute meshopt GLB with no images and a report", async () => {
  const dir = await mkdtemp(join(tmpdir(), "kitc-"));
  await stage(dir);
  const out = join(dir, "out");
  const rep = await compressDir({ stageDir: dir, outDir: out, reportPath: join(dir, "rep.json"), name: "t" });
  const glb = await readFile(join(out, "t.glb"));
  const n = glb.readUInt32LE(12);
  const j = JSON.parse(glb.subarray(20, 20 + n).toString());
  assert.deepEqual(j.extensionsUsed, ["EXT_meshopt_compression"]);
  assert.equal(j.images, undefined);
  for (const a of j.accessors.filter((a) => a.type !== "SCALAR")) assert.equal(a.componentType, 5126);
  assert.ok(rep.textureEncodes.length === 9);
});

test("an embedded image fails the compress step", async () => {
  const dir = await mkdtemp(join(tmpdir(), "kitc-"));
  await stage(dir);
  const io = new NodeIO();
  const doc = await io.read(join(dir, "t.glb"));
  doc.createTexture("x").setImage(new Uint8Array(await readFile(join(dir, "t.albedo.png")))).setMimeType("image/png");
  await io.write(join(dir, "t.glb"), doc);
  await assert.rejects(compressDir({ stageDir: dir, outDir: join(dir, "o"), reportPath: join(dir, "r.json"), name: "t" }), /embeds images/);
});

// Pins --assign-tf: without it ktx assumes the PNG is sRGB and converts UNORM channels (128 -> ~55).
test("linear channels keep their values through the UASTC encode", async () => {
  const dir = await mkdtemp(join(tmpdir(), "kitc-"));
  for (const [ch, px] of [["normal", [128, 128, 255, 255]], ["orm", [128, 128, 128, 255]], ["masks", [128, 128, 128, 128]]]) {
    const raw = Buffer.alloc(64 * 64 * 4);
    for (let i = 0; i < raw.length; i += 4) raw.set(px, i);
    const png = join(dir, `u.${ch}.png`);
    await sharp(raw, { raw: { width: 64, height: 64, channels: 4 } }).png().toFile(png);
    const dec = decodeUastc(await encodeUastc(png, ch, join(dir, `u.${ch}.uastc.ktx2`)), join(dir, `u.${ch}.dec.png`));
    const { data } = await sharp(dec).raw().toBuffer({ resolveWithObject: true });
    for (let c = 0; c < 4; c++)
      if (px[c] === 128) assert.ok(Math.abs(data[c] - 128) <= 2, `${ch} channel ${c}: decoded ${data[c]}, expected 128 +-2`);
  }
});

test("an over-budget piece throws and leaves outDir and the report untouched", async () => {
  const dir = await mkdtemp(join(tmpdir(), "kitc-"));
  await stage(dir);
  const out = join(dir, "out"), rep = join(dir, "rep.json"), budgets = { piece: 100, hero: 1e9 };
  await mkdir(out);
  await writeFile(join(out, "sentinel"), "keep");
  await assert.rejects(compressDir({ stageDir: dir, outDir: out, reportPath: rep, name: "t", budgets }),
    /bc7 KTX2 total \d+ B > 100 B \(piece budget\)/);
  assert.deepEqual(await readdir(out), ["sentinel"]);
  assert.deepEqual((await readdir(dir)).filter((f) => f.startsWith("out.tmp")), []);
  await assert.rejects(readFile(rep), /ENOENT/);
  const ok = await compressDir({ stageDir: dir, outDir: out, reportPath: rep, name: "t", budgetClass: "hero", budgets });
  assert.equal(ok.budgets.class, "hero");
  assert.ok(!(await readdir(out)).includes("sentinel"));
  await assert.rejects(compressDir({ stageDir: dir, outDir: out, reportPath: rep, name: "t", budgetClass: "huge" }), /unknown budget class/);
});

test("KTX budgets are the owner's per-class values (mirrored by kit_manifest.rs)", () => {
  assert.deepEqual(KTX_BUDGETS, { piece: 1_500_000, tileable: 3_600_000, hero: 11_500_000 });
});
