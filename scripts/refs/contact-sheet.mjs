// Builds the owner-review contact sheet: one row per piece, ref-1 and ref-2 side by side, labeled.
// Usage: node scripts/refs/contact-sheet.mjs <out.png> <piece>...
import { createRequire } from "node:module";
const sharp = createRequire(new URL("../assets/package.json", import.meta.url))("sharp");

const [out, ...pieces] = process.argv.slice(2);
const W = 800, H = 597, LABEL = 36, GAP = 8;
const cols = 2;
const sheetW = cols * W + (cols + 1) * GAP;
const rowH = H + LABEL + GAP;
const layers = [];
for (const [r, id] of pieces.entries()) {
  for (let c = 0; c < cols; c++) {
    const left = GAP + c * (W + GAP), top = GAP + r * rowH;
    const label = `${id} / ref-${c + 1}`;
    const svg = `<svg width="${W}" height="${LABEL}"><rect width="100%" height="100%" fill="#1e1e1e"/>` +
      `<text x="12" y="25" font-family="Helvetica, Arial, sans-serif" font-size="20" fill="#e6e6e6">${label}</text></svg>`;
    layers.push({ input: Buffer.from(svg), left, top });
    const img = await sharp(`assets-source/refs/${id}/ref-${c + 1}.jpg`).resize(W, H, { fit: "contain", background: "#2a2a2a" }).toBuffer();
    layers.push({ input: img, left, top: top + LABEL });
  }
}
await sharp({ create: { width: sheetW, height: GAP + pieces.length * rowH, channels: 3, background: "#121212" } })
  .composite(layers).png().toFile(out);
console.log("wrote", out);
