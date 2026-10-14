#!/usr/bin/env node
// Builds every Hex World brand raster from the committed sources in design/brand/ (`make brand`).
//
// Deterministic by construction: one pinned resvg renders every PNG (no timestamps or tIME chunks),
// the ICO is assembled here from those PNGs, every input is a committed file, and text is set in the
// committed Manrope TTF with system fonts disabled. Running it twice gives identical bytes.
//
// Outputs: favicon.ico (repo root, Trunk copy-file), web/brand/* (shipped by Trunk's copy-dir web),
// design/brand/icon-concepts.png (concept sheet, not shipped).
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const RESVG_VERSION = "0.48.1";
const repo = fileURLToPath(new URL("../..", import.meta.url));
const src = join(repo, "design/brand");
const web = join(repo, "web/brand");
const tmp = join(repo, "target/brand");
const FONT = join(src, "fonts/manrope-latin-wght.ttf");
const FAMILY = "Manrope ExtraLight"; // the family name inside the variable TTF; weight picks the instance

const T = {
  sheet: "#12273a", sheet2: "#19344c", ink: "#eef3f6", ink2: "#b0c5d6", ink3: "#849cb0", pick: "#c62e68",
  grass: "#9cc464", forest: "#3f8a4f", road: "#e2683c", river: "#4aa3df", beach: "#e3c27a", ocean: "#2d6fb0",
};

const CONCEPTS = [
  { id: "a", file: "concept-a-socket-key.svg", name: "A  Socket Key",
    line: ["The six socket colours as one code round an empty", "slot, with the magenta pick at its centre."] },
  { id: "b", file: "concept-b-diorama.svg", name: "B  Diorama",
    line: ["A raised river-bend tile floating over the survey", "sheet: the diorama itself."] },
  { id: "c", file: "concept-c-legend-glyph.svg", name: "C  Legend Glyph",
    line: ["The flat River bridge glyph the inventory draws,", "rimmed in its socket code."] },
];
const RECOMMENDED = "a";
const REASON = ["The Legend signature itself, the", "socket code. It stays a crisp", "six-colour hex at 16 px."];

function resvgVersion() {
  let v;
  try {
    v = execFileSync("resvg", ["--version"], { encoding: "utf8" }).trim();
  } catch {
    throw new Error(`resvg not found; install resvg ${RESVG_VERSION} (brew install resvg)`);
  }
  if (v !== RESVG_VERSION) throw new Error(`resvg ${v} found, ${RESVG_VERSION} required for byte-identical output`);
}

function render(svg, png, size) {
  const args = ["--skip-system-fonts", "--use-font-file", FONT, "--resources-dir", src];
  if (size) args.push("-w", String(size[0]), "-h", String(size[1]));
  execFileSync("resvg", [...args, svg, png], { stdio: ["ignore", "ignore", "inherit"] });
  return readFileSync(png);
}

const sq = (n) => [n, n];
const dataUri = (buf, type = "image/png") => `data:${type};base64,${buf.toString("base64")}`;
const esc = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;");
const text = (x, y, size, weight, fill, s, extra = "") =>
  `<text x="${x}" y="${y}" font-family="${FAMILY}" font-size="${size}" font-weight="${weight}" fill="${fill}"${extra}>${esc(s)}</text>`;

// PNG-compressed ICO (Windows Vista+, every current browser): 6-byte header, 16-byte entries, PNG blobs.
function ico(entries) {
  const head = Buffer.alloc(6 + 16 * entries.length);
  head.writeUInt16LE(0, 0);
  head.writeUInt16LE(1, 2);
  head.writeUInt16LE(entries.length, 4);
  let offset = head.length;
  entries.forEach(({ size, buf }, i) => {
    const o = 6 + 16 * i;
    head.writeUInt8(size >= 256 ? 0 : size, o);
    head.writeUInt8(size >= 256 ? 0 : size, o + 1);
    head.writeUInt16LE(1, o + 4); // colour planes
    head.writeUInt16LE(32, o + 6); // bits per pixel
    head.writeUInt32LE(buf.length, o + 8);
    head.writeUInt32LE(offset, o + 12);
    offset += buf.length;
  });
  return Buffer.concat([head, ...entries.map((e) => e.buf)]);
}

// Concept sheet: each concept at 512 (shown at 280), then 64/32/16 at 1:1 and 16 zoomed 6x
// nearest-neighbour, on Chrome's dark (#202124) and light (#f1f3f4) tab strips.
function conceptSheet(renders, maskable) {
  const W = 1800, H = 1120, colW = 540, x0 = 60, gap = 40;
  let s = `<svg xmlns="http://www.w3.org/2000/svg" width="${W}" height="${H}" viewBox="0 0 ${W} ${H}">\n`;
  s += `<rect width="${W}" height="${H}" fill="${T.sheet}"/>\n`;
  s += text(x0, 78, 40, 800, T.ink, "Hex World app icon concepts") + "\n";
  s += text(x0, 110, 20, 500, T.ink2, "Legend direction. Each concept at 512, 64, 32 and 16 px on dark and light browser chrome, then 16 px zoomed 6x.") + "\n";
  s += text(x0, 136, 16, 500, T.ink3, "A's 32 and 16 px come from its small-size source, icon-small.svg, exactly as shipped.") + "\n";
  CONCEPTS.forEach((c, i) => {
    const x = x0 + i * (colW + gap);
    const r = renders[c.id];
    const rec = c.id === RECOMMENDED;
    s += `<rect x="${x}" y="150" width="${colW}" height="930" rx="6" fill="${T.sheet2}"${rec ? ` stroke="${T.pick}" stroke-width="3"` : ""}/>\n`;
    s += text(x + 28, 200, 28, 800, T.ink, c.name) + "\n";
    c.line.forEach((l, j) => (s += text(x + 28, 232 + j * 24, 17, 500, T.ink2, l) + "\n"));
    s += `<image x="${x + 130}" y="290" width="280" height="280" href="${dataUri(r[512])}"/>\n`;
    s += text(x + 28, 612, 14, 600, T.ink3, "64") + text(x + 120, 612, 14, 600, T.ink3, "32") +
      text(x + 182, 612, 14, 600, T.ink3, "16") + text(x + 236, 612, 14, 600, T.ink3, "16 at 6x") + "\n";
    [["#202124", 624], ["#f1f3f4", 744]].forEach(([bg, y]) => {
      s += `<rect x="${x + 16}" y="${y}" width="${colW - 32}" height="108" rx="4" fill="${bg}"/>\n`;
      s += `<image x="${x + 28}" y="${y + 22}" width="64" height="64" href="${dataUri(r[64])}"/>\n`;
      s += `<image x="${x + 120}" y="${y + 38}" width="32" height="32" href="${dataUri(r[32])}"/>\n`;
      s += `<image x="${x + 182}" y="${y + 46}" width="16" height="16" href="${dataUri(r[16])}"/>\n`;
      s += `<image x="${x + 236}" y="${y + 6}" width="96" height="96" image-rendering="optimizeSpeed" href="${dataUri(r[16])}"/>\n`;
    });
    if (rec) {
      s += `<rect x="${x + 28}" y="884" width="140" height="30" rx="4" fill="${T.pick}"/>\n`;
      s += text(x + 42, 905, 16, 800, T.ink, "Recommended") + "\n";
      REASON.forEach((l, j) => (s += text(x + 28, 946 + j * 22, 16, 500, T.ink2, l) + "\n"));
      s += text(x + 316, 900, 14, 600, T.ink3, "Maskable 512, cropped") + "\n";
      s += `<clipPath id="circ"><circle cx="${x + 360}" cy="976" r="44"/></clipPath>\n`;
      s += `<image x="${x + 316}" y="932" width="88" height="88" clip-path="url(#circ)" href="${dataUri(maskable)}"/>\n`;
      s += `<clipPath id="sqir"><rect x="${x + 420}" y="932" width="88" height="88" rx="20"/></clipPath>\n`;
      s += `<image x="${x + 420}" y="932" width="88" height="88" clip-path="url(#sqir)" href="${dataUri(maskable)}"/>\n`;
    }
  });
  return s + "</svg>\n";
}

function main() {
  resvgVersion();
  rmSync(tmp, { recursive: true, force: true });
  mkdirSync(tmp, { recursive: true });
  mkdirSync(web, { recursive: true });
  const written = [];
  const put = (path, buf) => {
    writeFileSync(path, buf);
    written.push(path);
  };

  // Shipped icon set, from the recommended concept's three sources.
  const small = join(src, "icon-small.svg"); // tuned for 16-48 px; also the shipped favicon.svg
  const full = join(src, "icon.svg"); // rounded tile, purpose "any"
  const bleed = join(src, "icon-maskable.svg"); // full-bleed, mark inside the 80% safe circle
  const ico3 = [16, 32, 48].map((n) => ({ size: n, buf: render(small, join(tmp, `small-${n}.png`), sq(n)) }));
  put(join(repo, "favicon.ico"), ico(ico3));
  put(join(web, "favicon-32.png"), ico3[1].buf);
  copyFileSync(small, join(web, "favicon.svg"));
  written.push(join(web, "favicon.svg"));
  put(join(web, "apple-touch-icon.png"), render(bleed, join(tmp, "apple.png"), sq(180)));
  for (const n of [192, 512]) {
    put(join(web, `icon-${n}.png`), render(full, join(tmp, `full-${n}.png`), sq(n)));
    put(join(web, `icon-maskable-${n}.png`), render(bleed, join(tmp, `bleed-${n}.png`), sq(n)));
  }

  // Social share image: in-engine shot plus wordmark, set in Manrope.
  put(join(web, "og-image.png"), render(join(src, "og-image.svg"), join(tmp, "og.png"), [1200, 630]));

  // Concept sheet.
  const renders = {};
  for (const c of CONCEPTS) {
    renders[c.id] = {};
    for (const n of [512, 64, 32, 16]) renders[c.id][n] = render(join(src, c.file), join(tmp, `${c.id}-${n}.png`), sq(n));
  }
  if (RECOMMENDED === "a") {
    renders.a[16] = ico3[0].buf; // the shipped 16 px comes from the small-size source
    renders.a[32] = ico3[1].buf;
  }
  const sheetSvg = join(tmp, "icon-concepts.svg");
  writeFileSync(sheetSvg, conceptSheet(renders, readFileSync(join(tmp, "bleed-512.png"))));
  put(join(src, "icon-concepts.png"), render(sheetSvg, join(tmp, "icon-concepts.png")));

  for (const p of written) console.log(`${String(statSync(p).size).padStart(8)}  ${relative(repo, p)}`);
}

main();
