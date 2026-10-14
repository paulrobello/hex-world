// C1 budget probe: encodes realistic albedo/normal/ORM/masks maps through the kit pipeline (encodeMap)
// and prints exact BC7/ASTC KTX2 bytes. Run from the repo root after `make fetch-tools`:
//   node scripts/assets/kit-texture-probe.mjs [outDir] [--part maps|variants|levers]
// Default outDir target/kit-probe; each part writes probe-<part>.json there. levers needs maps first.
// Sources are the owner-approved reference JPEGs, cropped at native resolution (never upscaled).
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import sharp from "sharp";
import { encodeMap } from "./kit-compress.mjs";
import { sha256 } from "./kit-lib.mjs";

const ROOT = resolve(dirname(new URL(import.meta.url).pathname), "../..");
const OUT = resolve(ROOT, process.argv[2]?.startsWith("--") === false ? process.argv[2] : "target/kit-probe");
const SOURCES = {
  meadow: "assets-source/refs/meadow_base/ref-1.jpg",
  road: "assets-source/refs/road_straight/ref-1.jpg",
  tree: "assets-source/refs/tree_hero_oak/ref-1.jpg",
};
const SIZES = [2048, 1024, 512];

// Native-resolution square crop. The refs are 2400x1792, so 2048 takes a 2048x1792 centre crop and
// mirror-extends 256 rows; 1024 and 512 are plain centre crops.
async function rgbSquare(src, size) {
  const img = sharp(join(ROOT, src));
  const { width, height } = await img.metadata();
  const w = Math.min(size, width), h = Math.min(size, height);
  const crop = img.extract({ left: (width - w) >> 1, top: (height - h) >> 1, width: w, height: h });
  const padded = h < size || w < size
    ? sharp(await crop.toBuffer()).extend({ bottom: size - h, right: size - w, extendWith: "mirror" })
    : crop;
  return padded.removeAlpha().raw().toBuffer();
}

const lumOf = (rgb, n) => {
  const l = new Float32Array(n);
  for (let i = 0; i < n; i++) l[i] = (0.2126 * rgb[3 * i] + 0.7152 * rgb[3 * i + 1] + 0.0722 * rgb[3 * i + 2]) / 255;
  return l;
};
function blur(src, s, r) { // separable box blur with wrap
  const tmp = new Float32Array(s * s), out = new Float32Array(s * s), k = 2 * r + 1;
  for (let y = 0; y < s; y++) for (let x = 0; x < s; x++) {
    let a = 0; for (let d = -r; d <= r; d++) a += src[y * s + ((x + d + s) % s)]; tmp[y * s + x] = a / k;
  }
  for (let y = 0; y < s; y++) for (let x = 0; x < s; x++) {
    let a = 0; for (let d = -r; d <= r; d++) a += tmp[((y + d + s) % s) * s + x]; out[y * s + x] = a / k;
  }
  return out;
}
const u8 = (v) => Math.max(0, Math.min(255, Math.round(v * 255)));

async function writeRgba(buf, s, path) {
  await mkdir(dirname(path), { recursive: true });
  await sharp(buf, { raw: { width: s, height: s, channels: 4 } }).png().toFile(path);
}

// Builds the four channel PNGs for one source at one size. `cutout` gives the albedo a leaf-card
// alpha (hard edges, as an alpha-tested tree atlas has). `tileAo` sets ORM.R = 1 (bake_tileable_set).
async function makeMaps(src, s, dir, { cutout, tileAo }) {
  const rgb = await rgbSquare(src, s), n = s * s;
  const lum = lumOf(rgb, n), h = blur(lum, s, 1), ao = blur(lum, s, Math.max(2, s >> 8));
  const hp = blur(lum, s, 2);
  const albedo = Buffer.alloc(n * 4), normal = Buffer.alloc(n * 4), orm = Buffer.alloc(n * 4), masks = Buffer.alloc(n * 4);
  const soft = blur(lum, s, Math.max(4, s >> 6));
  for (let y = 0; y < s; y++) for (let x = 0; x < s; x++) {
    const i = y * s + x, at = (xx, yy) => h[((yy + s) % s) * s + ((xx + s) % s)];
    albedo[4 * i] = rgb[3 * i]; albedo[4 * i + 1] = rgb[3 * i + 1]; albedo[4 * i + 2] = rgb[3 * i + 2];
    albedo[4 * i + 3] = cutout ? (soft[i] < 0.55 ? 255 : 0) : 255;
    const gx = (at(x + 1, y - 1) + 2 * at(x + 1, y) + at(x + 1, y + 1)) - (at(x - 1, y - 1) + 2 * at(x - 1, y) + at(x - 1, y + 1));
    const gy = (at(x - 1, y + 1) + 2 * at(x, y + 1) + at(x + 1, y + 1)) - (at(x - 1, y - 1) + 2 * at(x, y - 1) + at(x + 1, y - 1));
    const k = 4.0, nx = -gx * k, ny = -gy * k, len = Math.hypot(nx, ny, 1);
    normal[4 * i] = u8(nx / len * 0.5 + 0.5); normal[4 * i + 1] = u8(ny / len * 0.5 + 0.5);
    normal[4 * i + 2] = u8(1 / len * 0.5 + 0.5); normal[4 * i + 3] = 255;
    orm[4 * i] = tileAo ? 255 : u8(0.35 + 0.65 * ao[i]); orm[4 * i + 1] = u8(0.7 + 1.5 * (lum[i] - hp[i])); orm[4 * i + 2] = 0; orm[4 * i + 3] = 255;
    masks[4 * i] = u8(soft[i]); masks[4 * i + 1] = u8(1 - soft[i]); masks[4 * i + 2] = 0; masks[4 * i + 3] = u8(0.35 + 0.65 * ao[i]);
  }
  const out = {};
  for (const [ch, b] of Object.entries({ albedo, normal, orm, masks })) await writeRgba(b, s, (out[ch] = join(dir, `src.${ch}.png`)));
  return out;
}

// --part maps (default): every source x size x channel at the shipping settings (kit-lib CHANNELS).
// --part variants: smoother "as-baked" normal (height blurred r=3) and masks (constant R/G/B, smooth AO),
//   at 1024 and 2048, to bracket how much the photo-derived maps above overstate a real bake.
// --part levers: UASTC RDO lambda sweep on the 1024 meadow albedo and normal (CHANNELS rdo mutated in-process).
const part = process.argv.includes("--part") ? process.argv[process.argv.indexOf("--part") + 1] : "maps";
const results = [];
const t0 = Date.now();
async function encodeRow(png, ch, stem, row) {
  for (const e of await encodeMap(png, ch, stem)) row[e.family] = e.bytes;
  row.pngSha256 = sha256(await readFile(png));
  results.push(row);
  console.log(JSON.stringify(row));
}
if (part === "maps") {
  for (const [name, src] of Object.entries(SOURCES)) for (const s of SIZES) {
    const dir = join(OUT, name, String(s));
    const pngs = await makeMaps(src, s, dir, { cutout: name === "tree", tileAo: name !== "tree" });
    for (const [ch, png] of Object.entries(pngs)) await encodeRow(png, ch, join(dir, `${name}.${ch}`), { source: name, size: s, channel: ch });
  }
} else if (part === "variants") {
  for (const s of [2048, 1024]) {
    const dir = join(OUT, "variants", String(s)), n = s * s;
    const lum = lumOf(await rgbSquare(SOURCES.tree, s), n), h = blur(lum, s, 3), ao = blur(lum, s, Math.max(8, s >> 5));
    const normal = Buffer.alloc(n * 4), masks = Buffer.alloc(n * 4);
    for (let y = 0; y < s; y++) for (let x = 0; x < s; x++) {
      const i = y * s + x, at = (xx, yy) => h[((yy + s) % s) * s + ((xx + s) % s)];
      const gx = (at(x + 1, y - 1) + 2 * at(x + 1, y) + at(x + 1, y + 1)) - (at(x - 1, y - 1) + 2 * at(x - 1, y) + at(x - 1, y + 1));
      const gy = (at(x - 1, y + 1) + 2 * at(x, y + 1) + at(x + 1, y + 1)) - (at(x - 1, y - 1) + 2 * at(x, y - 1) + at(x + 1, y - 1));
      const nx = -gx * 4, ny = -gy * 4, len = Math.hypot(nx, ny, 1);
      normal[4 * i] = u8(nx / len * 0.5 + 0.5); normal[4 * i + 1] = u8(ny / len * 0.5 + 0.5); normal[4 * i + 2] = u8(1 / len * 0.5 + 0.5); normal[4 * i + 3] = 255;
      masks[4 * i] = 153; masks[4 * i + 1] = 77; masks[4 * i + 2] = 0; masks[4 * i + 3] = u8(0.35 + 0.65 * ao[i]);
    }
    for (const [ch, b, v] of [["normal", normal, "normal_smooth"], ["masks", masks, "masks_baked"]]) {
      const png = join(dir, `src.${v}.png`);
      await writeRgba(b, s, png);
      await encodeRow(png, ch, join(dir, `tree.${v}`), { source: "tree", size: s, channel: v });
    }
  }
} else if (part === "levers") {
  const { CHANNELS } = await import("./kit-lib.mjs");
  for (const ch of ["albedo", "normal"]) for (const l of [0.25, 0.5, 1, 2, 4]) {
    CHANNELS[ch].rdo = l;
    await encodeRow(join(OUT, "meadow/1024", `src.${ch}.png`), ch, join(OUT, "levers", `meadow.${ch}.l${l}`),
      { source: "meadow", size: 1024, channel: ch, rdo: l });
  }
} else throw new Error(`unknown --part ${part}`);
await writeFile(join(OUT, `probe-${part}.json`), JSON.stringify({ date: new Date().toISOString(), seconds: (Date.now() - t0) / 1000, sources: SOURCES, results }, null, 2) + "\n");
