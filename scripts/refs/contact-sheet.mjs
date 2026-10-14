// Builds the owner-review contact sheet: one row per piece, its refs side by side (as many as its refs.json lists), labeled.
// Usage: node scripts/refs/contact-sheet.mjs <out.png|out.jpg> <piece>...
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
const sharp = createRequire(new URL("../assets/package.json", import.meta.url))("sharp");

const [out, ...pieces] = process.argv.slice(2);
const W = 800, H = 597, LABEL = 36, GAP = 8;
const counts = pieces.map((id) => JSON.parse(readFileSync(`assets-source/refs/${id}/refs.json`, "utf8")).length);
const cols = Math.max(...counts);
const sheetW = cols * W + (cols + 1) * GAP;
const rowH = H + LABEL + GAP;
const layers = [];
for (const [r, id] of pieces.entries()) {
  for (let c = 0; c < counts[r]; c++) {
    const left = GAP + c * (W + GAP), top = GAP + r * rowH;
    const label = `${id} / ref-${c + 1}`;
    const svg = `<svg width="${W}" height="${LABEL}"><rect width="100%" height="100%" fill="#1e1e1e"/>` +
      `<text x="12" y="25" font-family="Helvetica, Arial, sans-serif" font-size="20" fill="#e6e6e6">${label}</text></svg>`;
    layers.push({ input: Buffer.from(svg), left, top });
    const img = await sharp(`assets-source/refs/${id}/ref-${c + 1}.jpg`).resize(W, H, { fit: "contain", background: "#2a2a2a" }).toBuffer();
    layers.push({ input: img, left, top: top + LABEL });
  }
}
// An explicit .png() would override the extension, so a .jpg sheet needs .jpeg() to really be JPEG.
const sheet = sharp({ create: { width: sheetW, height: GAP + pieces.length * rowH, channels: 3, background: "#121212" } })
  .composite(layers);
await (/\.jpe?g$/i.test(out) ? sheet.jpeg({ quality: 90 }) : sheet.png()).toFile(out);
console.log("wrote", out);
