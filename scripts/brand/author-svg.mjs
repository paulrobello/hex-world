// Re-authors the icon SVGs in design/brand/ from hex geometry (pointy-top, edge 0 east, as in
// hex-core and the Legend comp). The committed SVGs are the source of truth and `make brand` never
// runs this. Run it only to change the geometry, then review the diff:
//
//     node scripts/brand/author-svg.mjs design/brand
import { writeFileSync } from "node:fs";

const out = process.argv[2] ?? "design/brand";
const T = {
  sheet: "#12273a", sheet2: "#19344c", ink: "#eef3f6", pick: "#c62e68",
  grass: "#9cc464", forest: "#3f8a4f", road: "#e2683c", river: "#4aa3df", beach: "#e3c27a", ocean: "#2d6fb0",
};
const f = (n) => Number(n.toFixed(1));
const vtx = (cx, cy, r, i, sy = 1) => {
  const a = ((-30 + 60 * i) * Math.PI) / 180;
  return [f(cx + r * Math.cos(a)), f(cy + r * Math.sin(a) * sy)];
};
const hex = (cx, cy, r, sy = 1) => [0, 1, 2, 3, 4, 5].map((i) => vtx(cx, cy, r, i, sy));
const pts = (p) => p.map((q) => q.join(",")).join(" ");
const mid = (a, b) => [f((a[0] + b[0]) / 2), f((a[1] + b[1]) / 2)];
const head = (title, extra = "") =>
  `<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512">\n  <title>${title}</title>${extra}\n`;
const tile = (full) => full
  ? `  <rect width="512" height="512" fill="${T.sheet}"/>\n`
  : `  <rect width="512" height="512" rx="112" fill="${T.sheet}"/>\n`;

// A: Socket Key. Six socket-colour blocks wrapped round an empty slot, magenta pick at the centre.
function socketKey({ full = false, small = false, scale = 1 } = {}) {
  const C = 256;
  const ro = (small ? 236 : 200) * scale, ri = (small ? 132 : 132) * scale, rp = (small ? 62 : 44) * scale;
  const gap = small ? 0 : 12;
  // NE, E... in edge order e0..e5 = E, SE, SW, W, NW, NE.
  const kinds = ["beach", "grass", "forest", "ocean", "river", "road"];
  const O = hex(C, C, ro), I = hex(C, C, ri);
  let s = head("Hex World: Socket Key") + tile(full);
  s += `  <polygon points="${pts(I)}" fill="${T.sheet2}"/>\n`;
  s += `  <g stroke="${T.sheet}" stroke-width="${gap}" stroke-linejoin="round">\n`;
  for (let e = 0; e < 6; e++) {
    const q = [O[e], O[(e + 1) % 6], I[(e + 1) % 6], I[e]];
    s += `    <polygon points="${pts(q)}" fill="${T[kinds[e]]}"${gap ? "" : ' stroke="none"'}/>\n`;
  }
  s += `  </g>\n`;
  s += `  <polygon points="${pts(hex(C, C, rp))}" fill="${T.pick}"/>\n</svg>\n`;
  return s;
}

// B: Diorama. A raised river-bend tile floating over the sheet.
function diorama({ full = false, scale = 1 } = {}) {
  const C = 256, cy = 222, R = 188 * scale, sy = 0.58, h = 64 * scale;
  const top = hex(C, cy, R, sy);
  const dn = (p) => [p[0], f(p[1] + h)];
  const sideR = [top[0], top[1], top[2], dn(top[2]), dn(top[1]), dn(top[0])];
  const sideL = [top[2], top[3], dn(top[3]), dn(top[2])];
  const w = mid(top[3], top[4]), ne = mid(top[5], top[0]);
  let s = head("Hex World: Diorama") + tile(full);
  s += `  <ellipse cx="${C}" cy="${f(cy + h + R * sy * 0.98)}" rx="${f(R * 0.86)}" ry="${f(R * 0.12)}" fill="#0b1824" opacity=".7"/>\n`;
  s += `  <polygon points="${pts(sideR)}" fill="#6b5039"/>\n`;
  s += `  <polygon points="${pts(sideL)}" fill="#54402e"/>\n`;
  // strata
  for (const k of [0.38, 0.7]) {
    const y = f(h * k);
    s += `  <polyline points="${pts([top[3], top[2], top[1], top[0]].map((p) => [p[0], f(p[1] + y)]))}" fill="none" stroke="#3e2f22" stroke-width="${f(5 * scale)}" opacity=".7"/>\n`;
  }
  s += `  <polygon points="${pts(top)}" fill="${T.grass}"/>\n`;
  // river bend from W edge to NE edge, with sand banks
  const cx1 = f(C + 10 * scale), cy1 = f(cy + 70 * scale);
  const d = `M${w.join(",")} Q${cx1},${cy1} ${ne.join(",")}`;
  s += `  <path d="${d}" fill="none" stroke="${T.beach}" stroke-width="${f(58 * scale)}" stroke-linecap="butt"/>\n`;
  s += `  <path d="${d}" fill="none" stroke="${T.river}" stroke-width="${f(34 * scale)}" stroke-linecap="butt"/>\n`;
  // trees on the near bank
  for (const [x, y, r] of [[300, 262, 22], [338, 238, 18], [196, 276, 20]]) {
    s += `  <circle cx="${f(C + (x - C) * scale)}" cy="${f(cy + (y - cy) * scale + 6)}" r="${f(r * scale)}" fill="#0b1824" opacity=".35"/>\n`;
    s += `  <circle cx="${f(C + (x - C) * scale)}" cy="${f(cy + (y - cy) * scale)}" r="${f(r * scale)}" fill="${T.forest}"/>\n`;
  }
  s += `  <polygon points="${pts(top)}" fill="none" stroke="#eef3f6" stroke-opacity=".18" stroke-width="${f(4 * scale)}" stroke-linejoin="round"/>\n`;
  s += `</svg>\n`;
  return s;
}

// C: Legend Glyph. The flat River-bridge glyph (socket code VRGVRG) the inventory draws.
function glyph({ full = false, scale = 1 } = {}) {
  const C = 256, R = 196 * scale;
  const H = hex(C, C, R);
  const code = "VRGVRG";
  const col = { V: T.river, R: T.road, G: T.grass };
  let s = head("Hex World: Legend Glyph") + tile(full);
  s += `  <polygon points="${pts(H)}" fill="${T.grass}"/>\n`;
  const lane = (e, c, wd) => {
    const m = mid(H[e], H[(e + 1) % 6]);
    return `  <line x1="${m[0]}" y1="${m[1]}" x2="${C}" y2="${C}" stroke="${c}" stroke-width="${f(wd * scale)}"/>\n`;
  };
  s += lane(0, T.river, 64) + lane(3, T.river, 64);
  s += lane(1, T.road, 40) + lane(4, T.road, 40);
  s += `  <rect x="${f(C - 40 * scale)}" y="${f(C - 40 * scale)}" width="${f(80 * scale)}" height="${f(80 * scale)}" rx="${f(10 * scale)}" fill="${T.beach}" transform="rotate(-30 ${C} ${C})"/>\n`;
  // socket code: each edge banded by its socket colour, inset so it reads as a rim
  const Hi = hex(C, C, R - 14 * scale);
  for (let e = 0; e < 6; e++) {
    const a = Hi[e], b = Hi[(e + 1) % 6];
    s += `  <line x1="${a[0]}" y1="${a[1]}" x2="${b[0]}" y2="${b[1]}" stroke="${code[e] === "G" ? T.forest : col[code[e]]}" stroke-width="${f(28 * scale)}" stroke-linecap="round"/>\n`;
  }
  s += `</svg>\n`;
  return s;
}

writeFileSync(`${out}/concept-a-socket-key.svg`, socketKey());
writeFileSync(`${out}/concept-b-diorama.svg`, diorama());
writeFileSync(`${out}/concept-c-legend-glyph.svg`, glyph());
// Recommended concept (A) sources.
function socketKeySmall() {
  const C = 256, ro = 254, ri = 146, rp = 66;
  const kinds = ["beach", "grass", "forest", "ocean", "river", "road"];
  const O = hex(C, C, ro), I = hex(C, C, ri);
  let s = head("Hex World");
  s += `  <polygon points="${pts(I)}" fill="${T.sheet}"/>\n`;
  for (let e = 0; e < 6; e++) {
    const q = [O[e], O[(e + 1) % 6], I[(e + 1) % 6], I[e]];
    s += `  <polygon points="${pts(q)}" fill="${T[kinds[e]]}"/>\n`;
  }
  s += `  <polygon points="${pts(hex(C, C, rp))}" fill="${T.pick}"/>\n</svg>\n`;
  return s;
}
writeFileSync(`${out}/icon.svg`, socketKey().replace("Hex World: Socket Key", "Hex World"));
writeFileSync(`${out}/icon-small.svg`, socketKeySmall());
writeFileSync(`${out}/icon-maskable.svg`, socketKey({ full: true, scale: 0.9 }).replace("Hex World: Socket Key", "Hex World"));
console.log("ok");
