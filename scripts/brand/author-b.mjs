#!/usr/bin/env node
// Re-authors the round-2 Diorama (B) icon variants in design/brand/ from a small orthographic
// scene model: a hex tile with a lit cap, banded strata, a river, trees and a cottage, under one
// key light from the top left. The committed SVGs are the source of truth and `make brand` never
// runs this. Run it only to change a variant, then review the diff:
//
//     node scripts/brand/author-b.mjs design/brand
//
// Each variant gets three sources: `concept-<id>.svg` (rounded plate, purpose "any"),
// `concept-<id>-maskable.svg` (full bleed, content inside the 80% safe circle, asserted here) and
// `concept-<id>-small.svg` (hand-placed on a 16-unit grid for 16/32/48 px, no filters).
import { writeFileSync } from "node:fs";

const out = process.argv[2] ?? "design/brand";
const f = (n) => Number(n.toFixed(1));
const pts = (p) => p.map((q) => `${f(q[0])},${f(q[1])}`).join(" ");

const C = {
  sheet: "#12273a", sheetHi: "#1f4262", sheetLo: "#0d1d2c", shadow: "#06111b",
  grassLit: "#bcdc78", grass: "#94c25c", grassShade: "#5f9440", turf: "#4c7d30", turfShade: "#3a6326",
  soilA: "#8f6645", soilB: "#7a553a", soilC: "#9a7752", soilD: "#664834", rock: "#53433a", lip: "#3a2a1c",
  river: "#4aa3df", riverDeep: "#2f78bd", riverHi: "#d4efff", sand: "#e8cb88", sandShade: "#caa461",
  leaf: "#4c9852", leafLit: "#93d06f", leafShade: "#22553a", trunk: "#5a3d28",
  wall: "#f2e7cf", roof: "#e2683c", window: "#ffd47a", door: "#5a3d28",
  pick: "#c62e68", ink: "#eef3f6",
};

// Key light from the top left, a little in front: lit left faces, shaded right faces.
const L = (() => {
  const v = [-0.55, 0.75, 0.38];
  const n = Math.hypot(...v);
  return v.map((c) => c / n);
})();
const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
const norm = (v) => {
  const n = Math.hypot(...v);
  return v.map((c) => c / n);
};
const hex2rgb = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
const rgb2hex = (c) => "#" + c.map((v) => Math.round(Math.max(0, Math.min(255, v))).toString(16).padStart(2, "0")).join("");
const mix = (a, b, t) => rgb2hex(hex2rgb(a).map((v, i) => v + (hex2rgb(b)[i] - v) * t));
// Lambert tone: k < 1 darkens toward the blue-black ambient, k > 1 warms toward the key light.
const tone = (hex, k) => (k < 1 ? mix(hex, "#101c28", 1 - k) : mix(hex, "#fff4d8", Math.min(1, k - 1)));
const lambert = (n) => 0.62 + 0.62 * Math.max(0, dot(n, L));

// Orthographic camera: x right, y up, z toward the viewer; `elev` is the camera's elevation.
function camera({ cx, cy, s, elev }) {
  const sa = Math.sin((elev * Math.PI) / 180), ca = Math.cos((elev * Math.PI) / 180);
  const view = [0, sa, ca];
  const P = (p) => [cx + s * p[0], cy + s * (p[2] * sa - p[1] * ca)];
  return { P, sa, ca, s, view, visible: (n) => dot(n, view) > 1e-3 };
}

const hexPlan = (R, theta0) => [0, 1, 2, 3, 4, 5].map((i) => {
  const a = ((theta0 + 60 * i) * Math.PI) / 180;
  return [R * Math.cos(a), R * Math.sin(a)];
});

function hull(points) {
  const p = [...points].sort((a, b) => a[0] - b[0] || a[1] - b[1]);
  const side = (o, a, b) => (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
  const lo = [], up = [];
  for (const q of p) {
    while (lo.length >= 2 && side(lo[lo.length - 2], lo[lo.length - 1], q) <= 0) lo.pop();
    lo.push(q);
  }
  for (const q of p.reverse()) {
    while (up.length >= 2 && side(up[up.length - 2], up[up.length - 1], q) <= 0) up.pop();
    up.push(q);
  }
  return lo.slice(0, -1).concat(up.slice(0, -1));
}

// Shadow of a 3D point cast along the key light onto the plane y = ground.
const castTo = (p, ground) => {
  const t = (p[1] - ground) / L[1];
  return [p[0] - L[0] * t, ground, p[2] - L[2] * t];
};

function bezier(p0, p1, p2, p3, t) {
  const u = 1 - t;
  return [0, 1].map((i) => u * u * u * p0[i] + 3 * u * u * t * p1[i] + 3 * u * t * t * p2[i] + t * t * t * p3[i]);
}

// ---- Scene description per variant -------------------------------------------------------------

const VARIANTS = {
  b1: {
    title: "Hex World: Diorama, lit",
    elev: 36, theta0: -90, R: 1, T: 0.36, gap: 0.05,
    river: { from: 4, to: 0, bend: 1.15, w: 0.2, bank: 0.1 },
    trees: [[0.46, 0.36, 0.13], [0.6, 0.0, 0.11], [-0.36, -0.42, 0.11]],
    bands: [[0.0, 0.07, "turf"], [0.07, 0.1, "lip"], [0.1, 0.36, "soilA"], [0.36, 0.43, "soilD"], [0.43, 0.66, "soilC"], [0.66, 0.74, "soilD"], [0.74, 1.0, "soilB"]],
  },
  b2: {
    title: "Hex World: Diorama, slice",
    elev: 27, theta0: 0, R: 1, T: 0.52, gap: 0.03,
    river: { from: 3, to: 1, bend: 0.55, w: 0.2, bank: 0.08, cut: true },
    trees: [[0.18, -0.52, 0.19]],
    house: { at: [0.5, 0.2], w: 0.3, d: 0.22, hw: 0.17, hr: 0.31, rot: 30 },
    bands: [[0.0, 0.06, "turf"], [0.06, 0.09, "lip"], [0.09, 0.3, "soilA"], [0.3, 0.36, "soilD"], [0.36, 0.58, "soilC"], [0.58, 0.64, "soilD"], [0.64, 0.84, "soilB"], [0.84, 1.0, "rock"]],
  },
  b4: {
    title: "Hex World: Diorama, rising",
    elev: 36, theta0: -90, R: 1, T: 0.3, gap: 0.24, slot: true,
    river: { from: 4, to: 0, bend: 1.15, w: 0.2, bank: 0.1 },
    trees: [[0.46, 0.36, 0.13], [-0.36, -0.42, 0.11]],
    bands: [[0.0, 0.08, "turf"], [0.08, 0.11, "lip"], [0.11, 0.42, "soilA"], [0.42, 0.5, "soilD"], [0.5, 1.0, "soilB"]],
  },
};

// Builds one variant's scene at a given camera; returns the SVG body, its defs and key points.
function scene(v, cam, uid) {
  const { P } = cam;
  const plan = hexPlan(v.R, v.theta0);
  const cap = plan.map(([x, z]) => [x, 0, z]);
  const T = v.T;
  const ground = -T - v.gap;
  const key = []; // screen points that must sit inside the safe zone
  const defs = [];
  let body = "";
  const k = (p) => (key.push(p), p);

  // Visible side faces, ordered left to right along the front perimeter.
  const faces = [];
  for (let i = 0; i < 6; i++) {
    const a = cap[i], b = cap[(i + 1) % 6];
    const n = norm([a[0] + b[0], 0, a[2] + b[2]]);
    if (n[2] > 0.05) faces.push({ a, b, n, i });
  }
  faces.sort((p, q) => (p.a[0] + p.b[0]) - (q.a[0] + q.b[0]));
  // Front perimeter vertices left to right.
  const front = [];
  for (const fc of faces) {
    const [l, r] = fc.a[0] < fc.b[0] ? [fc.a, fc.b] : [fc.b, fc.a];
    if (!front.length) front.push(l);
    front.push(r);
  }
  const down = (p, d) => [p[0], p[1] - d, p[2]];
  const sideOutline = [...front, ...[...front].reverse().map((p) => down(p, T))];
  sideOutline.forEach((p) => k(P(p)));
  cap.forEach((p) => k(P(p)));

  // Ground: soft contact shadow, and in the rising variant the empty slot the tile came out of.
  const base = plan.map(([x, z]) => [x, ground, z]);
  const shadowPlan = hull(cap.map((p) => castTo(down(p, T), ground)).concat(base).map((p) => [p[0], p[2]]));
  const sh = shadowPlan.map(([x, z]) => P([x, ground, z]));
  defs.push(`<filter id="${uid}soft" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="${f(cam.s * 0.06)}"/></filter>`);
  defs.push(`<filter id="${uid}tight" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="${f(cam.s * 0.018)}"/></filter>`);
  if (v.slot) {
    const ring = hexPlan(v.R * 0.98, v.theta0).map(([x, z]) => P([x, ground, z]));
    ring.forEach((p) => k(p));
    body += `  <polygon points="${pts(ring)}" fill="${C.shadow}" fill-opacity=".5"/>\n`;
    body += `  <polygon points="${pts(sh)}" fill="${C.shadow}" opacity=".55" filter="url(#${uid}soft)"/>\n`;
    body += `  <polygon points="${pts(ring)}" fill="none" stroke="${C.pick}" stroke-width="${f(cam.s * 0.04)}" stroke-linejoin="round" opacity=".45" filter="url(#${uid}tight)"/>\n`;
    body += `  <polygon points="${pts(ring)}" fill="none" stroke="${C.pick}" stroke-width="${f(cam.s * 0.02)}" stroke-linejoin="round"/>\n`;
  } else {
    body += `  <polygon points="${pts(sh)}" fill="${C.shadow}" opacity=".6" filter="url(#${uid}soft)"/>\n`;
    const ao = base.map((p) => P(p));
    body += `  <polygon points="${pts(ao)}" fill="${C.shadow}" opacity=".7" filter="url(#${uid}tight)"/>\n`;
  }

  // Strata: wavy bands that run continuously round the visible faces.
  const perim = []; // samples along the front perimeter with arc length
  let sAcc = 0;
  for (let i = 0; i < front.length - 1; i++) {
    const a = front[i], b = front[i + 1];
    const len = Math.hypot(b[0] - a[0], b[2] - a[2]);
    const n = 24;
    for (let j = i === 0 ? 0 : 1; j <= n; j++) {
      const t = j / n;
      perim.push({ p: [a[0] + (b[0] - a[0]) * t, 0, a[2] + (b[2] - a[2]) * t], s: sAcc + len * t });
    }
    sAcc += len;
  }
  const wob = (d, s, seed) => (d <= 0 || d >= T * 0.999 ? 0 : T * 0.018 * Math.sin(s * 9 + seed * 2.1) + T * 0.01 * Math.sin(s * 23 + seed));
  const boundary = (t, seed) => perim.map(({ p, s }) => P(down(p, t * T + wob(t * T, s, seed))));
  const clipSide = `${uid}side`;
  defs.push(`<clipPath id="${clipSide}"><polygon points="${pts(sideOutline.map(P))}"/></clipPath>`);
  body += `  <g clip-path="url(#${clipSide})">\n`;
  v.bands.forEach(([t0, t1, col], bi) => {
    const top = boundary(t0, bi), bot = boundary(t1, bi + 1).reverse();
    body += `    <polygon points="${pts([...top, ...bot])}" fill="${C[col]}"/>\n`;
  });
  // Grain: horizontal turbulence streaks, dark only, kept faint.
  defs.push(`<filter id="${uid}grain" x="0" y="0" width="100%" height="100%"><feTurbulence type="fractalNoise" baseFrequency="${f(6 / cam.s * 10) / 10} ${f(28 / cam.s * 10) / 10}" numOctaves="2" seed="7"/><feColorMatrix values="0 0 0 0 0.05  0 0 0 0 0.03  0 0 0 0 0.02  2.4 0 0 0 -1.15"/></filter>`);
  const sb = sideOutline.map(P);
  const [minX, maxX] = [Math.min(...sb.map((q) => q[0])), Math.max(...sb.map((q) => q[0]))];
  const [minY, maxY] = [Math.min(...sb.map((q) => q[1])), Math.max(...sb.map((q) => q[1]))];
  body += `    <rect x="${f(minX)}" y="${f(minY)}" width="${f(maxX - minX)}" height="${f(maxY - minY)}" filter="url(#${uid}grain)" opacity=".55"/>\n`;
  // Per-face light, then occlusion toward the base.
  for (const fc of faces) {
    const kf = lambert(fc.n);
    const quad = [fc.a, fc.b, down(fc.b, T), down(fc.a, T)].map(P);
    if (kf < 1) body += `    <polygon points="${pts(quad)}" fill="#0c1621" opacity="${f(Math.min(0.7, (1 - kf) * 1.15) * 100) / 100}"/>\n`;
    else body += `    <polygon points="${pts(quad)}" fill="#ffe9c4" opacity="${f(Math.min(0.3, (kf - 1) * 0.55) * 100) / 100}"/>\n`;
  }
  defs.push(`<linearGradient id="${uid}ao" gradientUnits="userSpaceOnUse" x1="0" y1="${f(minY + (maxY - minY) * 0.25)}" x2="0" y2="${f(maxY)}"><stop offset="0" stop-color="#0c1621" stop-opacity="0"/><stop offset="1" stop-color="#0c1621" stop-opacity=".5"/></linearGradient>`);
  body += `    <rect x="${f(minX)}" y="${f(minY)}" width="${f(maxX - minX)}" height="${f(maxY - minY)}" fill="url(#${uid}ao)"/>\n`;
  body += `  </g>\n`;
  // Face seams: a faint light edge on each vertical corner of the slab.
  for (const p of front.slice(1, -1)) {
    const [a, b] = [P(p), P(down(p, T))];
    body += `  <line x1="${f(a[0])}" y1="${f(a[1])}" x2="${f(b[0])}" y2="${f(b[1])}" stroke="#ffe9c4" stroke-opacity=".12" stroke-width="${f(cam.s * 0.008)}"/>\n`;
  }

  // Cap: graded grass, fine grain, then the river, trees and the cottage.
  const capS = cap.map(P);
  const cb = [Math.min(...capS.map((q) => q[0])), Math.min(...capS.map((q) => q[1])), Math.max(...capS.map((q) => q[0])), Math.max(...capS.map((q) => q[1]))];
  defs.push(`<linearGradient id="${uid}cap" gradientUnits="userSpaceOnUse" x1="${f(cb[0])}" y1="${f(cb[1])}" x2="${f(cb[2])}" y2="${f(cb[3])}"><stop offset="0" stop-color="${C.grassLit}"/><stop offset=".5" stop-color="${C.grass}"/><stop offset="1" stop-color="${C.grassShade}"/></linearGradient>`);
  defs.push(`<clipPath id="${uid}capc"><polygon points="${pts(capS)}"/></clipPath>`);
  defs.push(`<filter id="${uid}turf" x="0" y="0" width="100%" height="100%"><feTurbulence type="fractalNoise" baseFrequency="${f(40 / cam.s * 100) / 100}" numOctaves="2" seed="3"/><feColorMatrix values="0 0 0 0 0.1  0 0 0 0 0.2  0 0 0 0 0.05  2.2 0 0 0 -1.2"/></filter>`);
  body += `  <polygon points="${pts(capS)}" fill="url(#${uid}cap)"/>\n`;
  body += `  <g clip-path="url(#${uid}capc)">\n`;
  body += `    <rect x="${f(cb[0])}" y="${f(cb[1])}" width="${f(cb[2] - cb[0])}" height="${f(cb[3] - cb[1])}" filter="url(#${uid}turf)" opacity=".5"/>\n`;

  // River: a cubic in plan from one edge midpoint to another, offset in plan so it foreshortens.
  const r = v.river;
  const mid = (i) => [(plan[i][0] + plan[(i + 1) % 6][0]) / 2, (plan[i][1] + plan[(i + 1) % 6][1]) / 2];
  const A = mid(r.from), B = mid(r.to);
  const inA = [-A[0], -A[1]], inB = [-B[0], -B[1]];
  const ext = (p, d, t) => [p[0] - d[0] * t, p[1] - d[1] * t];
  const p0 = ext(A, inA, 0.12), p3 = ext(B, inB, 0.12);
  const p1 = [A[0] + inA[0] * r.bend, A[1] + inA[1] * r.bend], p2 = [B[0] + inB[0] * r.bend, B[1] + inB[1] * r.bend];
  const N = 48;
  const cl = Array.from({ length: N + 1 }, (_, i) => bezier(p0, p1, p2, p3, i / N));
  const offset = (w, side = 0) => cl.map((c, i) => {
    const a = cl[Math.max(0, i - 1)], b = cl[Math.min(N, i + 1)];
    const t = norm([b[0] - a[0], 0, b[1] - a[1]]);
    return [c[0] - t[2] * w, c[1] + t[0] * w];
  });
  const band = (w) => [...offset(w / 2), ...offset(-w / 2).reverse()].map(([x, z]) => P([x, 0, z]));
  const along = (w, t0, t1) => offset(w).slice(Math.round(t0 * N), Math.round(t1 * N) + 1).map(([x, z]) => P([x, 0, z]));
  defs.push(`<linearGradient id="${uid}water" gradientUnits="userSpaceOnUse" x1="${f(cb[0])}" y1="${f(cb[1])}" x2="${f(cb[2])}" y2="${f(cb[3])}"><stop offset="0" stop-color="#5cb4ea"/><stop offset="1" stop-color="${C.riverDeep}"/></linearGradient>`);
  body += `    <polygon points="${pts(band(r.w + r.bank * 2))}" fill="${C.sand}"/>\n`;
  body += `    <polyline points="${pts(along(-(r.w / 2 + r.bank * 0.7), 0, 1))}" fill="none" stroke="${C.sandShade}" stroke-width="${f(cam.s * 0.02)}" stroke-opacity=".6"/>\n`;
  body += `    <polygon points="${pts(band(r.w))}" fill="url(#${uid}water)"/>\n`;
  body += `    <polygon points="${pts(band(r.w * 0.42))}" fill="${C.riverDeep}" opacity=".35"/>\n`;
  body += `    <polyline points="${pts(along(r.w * 0.3, 0.06, 0.4))}" fill="none" stroke="${C.riverHi}" stroke-width="${f(cam.s * 0.018)}" stroke-linecap="round" stroke-opacity=".85"/>\n`;
  body += `    <polyline points="${pts(along(r.w * 0.12, 0.55, 0.8))}" fill="none" stroke="${C.riverHi}" stroke-width="${f(cam.s * 0.014)}" stroke-linecap="round" stroke-opacity=".6"/>\n`;
  body += `    <polyline points="${pts(along(r.w * 0.5, 0, 1))}" fill="none" stroke="#ffffff" stroke-width="${f(cam.s * 0.01)}" stroke-opacity=".35"/>\n`;
  body += `  </g>\n`;
  // Where the river leaves through a visible face, its water shows in the cut.
  if (r.cut) {
    const ext2 = offset(r.w / 2).at(-1), ext3 = offset(-r.w / 2).at(-1);
    const fc = faces.find((q) => q.i === r.to);
    if (fc) {
      // Intersect the bank lines with the edge: project each onto the edge segment.
      const onEdge = (p) => {
        const [a, b] = [fc.a, fc.b];
        const d = [b[0] - a[0], b[2] - a[2]];
        const t = ((p[0] - a[0]) * d[0] + (p[1] - a[2]) * d[1]) / (d[0] * d[0] + d[1] * d[1]);
        return [a[0] + d[0] * t, 0, a[2] + d[1] * t];
      };
      const w0 = onEdge(ext2), w1 = onEdge(ext3);
      const depth = T * 0.16;
      const sand0 = onEdge(offset(r.w / 2 + r.bank).at(-1)), sand1 = onEdge(offset(-r.w / 2 - r.bank).at(-1));
      body += `  <polygon points="${pts([sand0, sand1, down(sand1, depth * 0.55), down(sand0, depth * 0.55)].map(P))}" fill="${tone(C.sand, 0.8)}"/>\n`;
      body += `  <polygon points="${pts([w0, w1, down(w1, depth), down(w0, depth)].map(P))}" fill="${tone(C.riverDeep, 0.95)}"/>\n`;
      body += `  <polyline points="${pts([w0, w1].map(P))}" fill="none" stroke="${C.riverHi}" stroke-width="${f(cam.s * 0.014)}" stroke-opacity=".9"/>\n`;
    }
  }

  // Cap rim: the bevel where the cap turns down catches the light.
  const frontS = front.map(P);
  defs.push(`<linearGradient id="${uid}rim" gradientUnits="userSpaceOnUse" x1="${f(cb[0])}" y1="0" x2="${f(cb[2])}" y2="0"><stop offset="0" stop-color="#f6ffd9" stop-opacity=".95"/><stop offset="1" stop-color="#f6ffd9" stop-opacity=".25"/></linearGradient>`);
  body += `  <polygon points="${pts(capS)}" fill="none" stroke="#f6ffd9" stroke-opacity=".22" stroke-width="${f(cam.s * 0.012)}" stroke-linejoin="round"/>\n`;
  body += `  <polyline points="${pts(frontS)}" fill="none" stroke="url(#${uid}rim)" stroke-width="${f(cam.s * 0.016)}" stroke-linejoin="round"/>\n`;

  // Props, back to front: cast shadows first, then the objects.
  const props = [];
  for (const [x, z, rr] of v.trees) props.push({ z, kind: "tree", x, rr });
  if (v.house) props.push({ z: v.house.at[1], kind: "house", ...v.house });
  props.sort((a, b) => a.z - b.z);
  let shadows = "", objects = "";
  for (const pr of props) {
    if (pr.kind === "tree") {
      const th = pr.rr * 0.7;
      const top = [pr.x, th + pr.rr * 1.55, pr.z];
      const sp = [[pr.x, 0, pr.z], castTo([pr.x, th + pr.rr * 0.8, pr.z], 0)];
      const c0 = P(sp[0]), c1 = P(sp[1]);
      const rx = pr.rr * cam.s * 0.95, ry = rx * cam.sa;
      shadows += `  <ellipse cx="${f((c0[0] + c1[0]) / 2)}" cy="${f((c0[1] + c1[1]) / 2)}" rx="${f(rx + Math.abs(c1[0] - c0[0]) / 2)}" ry="${f(ry + Math.abs(c1[1] - c0[1]) / 2)}" fill="#12301b" opacity=".42" filter="url(#${uid}tight)"/>\n`;
      const b = P([pr.x, 0, pr.z]), t = P([pr.x, th, pr.z]);
      const tw = pr.rr * cam.s * 0.2;
      objects += `  <rect x="${f(b[0] - tw / 2)}" y="${f(t[1])}" width="${f(tw)}" height="${f(b[1] - t[1] + tw * 0.3)}" rx="${f(tw * 0.3)}" fill="${C.trunk}"/>\n`;
      const cc = P([pr.x, th + pr.rr * 0.75, pr.z]);
      const R0 = pr.rr * cam.s;
      k(P(top));
      for (const [dx, dy, rs] of [[-0.42, -0.36, 0.68], [0.44, -0.12, 0.62], [0, 0.06, 0.82], [-0.08, -0.62, 0.55]]) {
        objects += `  <circle cx="${f(cc[0] + dx * R0)}" cy="${f(cc[1] + dy * R0)}" r="${f(rs * R0)}" fill="url(#${uid}leaf)"/>\n`;
      }
    } else {
      const { at, w, d, hw, hr } = pr;
      const [x0, z0] = at;
      const ro = (pr.rot * Math.PI) / 180;
      const V = (x, y, z) => [x0 + x * Math.cos(ro) - z * Math.sin(ro), y, z0 + x * Math.sin(ro) + z * Math.cos(ro)];
      const corners = [V(-w / 2, 0, -d / 2), V(w / 2, 0, -d / 2), V(w / 2, 0, d / 2), V(-w / 2, 0, d / 2)];
      const topc = corners.map((p) => [p[0], hw, p[2]]);
      const ridge = [V(-w / 2, hr, 0), V(w / 2, hr, 0)];
      const all = [...corners, ...topc, ...ridge];
      const sp = hull(all.map((p) => castTo(p, 0)).map((p) => [p[0], p[2]])).map(([x, z]) => P([x, 0, z]));
      shadows += `  <polygon points="${pts(sp)}" fill="#12301b" opacity=".45" filter="url(#${uid}tight)"/>\n`;
      const cen = [x0, hw * 0.6, z0];
      const facesH = [
        { p: [corners[3], corners[2], topc[2], topc[3]], c: C.wall, front: true }, // front wall (z+)
        { p: [corners[0], corners[1], topc[1], topc[0]], c: C.wall }, // back wall
        { p: [corners[1], corners[2], topc[2], ridge[1], topc[1]], c: C.wall, gable: true }, // east gable
        { p: [corners[0], corners[3], topc[3], ridge[0], topc[0]], c: C.wall, gable: true }, // west gable
        { p: [topc[3], topc[2], ridge[1], ridge[0]], c: C.roof }, // front roof
        { p: [topc[0], topc[1], ridge[1], ridge[0]], c: C.roof }, // back roof
      ];
      for (const fh of facesH) {
        const cen2 = fh.p.reduce((a, q) => [a[0] + q[0] / fh.p.length, a[1] + q[1] / fh.p.length, a[2] + q[2] / fh.p.length], [0, 0, 0]);
        let n = norm(cross(sub(fh.p[1], fh.p[0]), sub(fh.p[2], fh.p[0])));
        if (dot(n, sub(cen2, cen)) < 0) n = n.map((c) => -c);
        if (!cam.visible(n)) continue;
        objects += `  <polygon points="${pts(fh.p.map(P))}" fill="${tone(fh.c, lambert(n))}" stroke="${tone(fh.c, lambert(n) * 0.9)}" stroke-width="${f(cam.s * 0.004)}" stroke-linejoin="round"/>\n`;
        if (fh.front) {
          const kw = lambert(n);
          const win = [V(0.03, hw * 0.4, d / 2), V(0.11, hw * 0.4, d / 2), V(0.11, hw * 0.75, d / 2), V(0.03, hw * 0.75, d / 2)];
          const door = [V(-0.1, 0, d / 2), V(-0.04, 0, d / 2), V(-0.04, hw * 0.68, d / 2), V(-0.1, hw * 0.68, d / 2)];
          objects += `  <polygon points="${pts(win.map(P))}" fill="${C.window}"/>\n`;
          objects += `  <polygon points="${pts(door.map(P))}" fill="${tone(C.door, kw)}"/>\n`;
        }
      }
      // Roof eave line: a light edge along the ridge.
      const rr2 = ridge.map(P);
      objects += `  <line x1="${f(rr2[0][0])}" y1="${f(rr2[0][1])}" x2="${f(rr2[1][0])}" y2="${f(rr2[1][1])}" stroke="#ffd2b8" stroke-opacity=".7" stroke-width="${f(cam.s * 0.008)}" stroke-linecap="round"/>\n`;
      all.forEach((p) => k(P(p)));
    }
  }
  defs.push(`<radialGradient id="${uid}leaf" cx=".5" cy=".5" r=".62" fx=".3" fy=".26"><stop offset="0" stop-color="${C.leafLit}"/><stop offset=".55" stop-color="${C.leaf}"/><stop offset="1" stop-color="${C.leafShade}"/></radialGradient>`);
  body += `  <g clip-path="url(#${uid}capc)">\n${shadows}  </g>\n` + objects;
  return { body, defs, key };
}

// Fits a variant: renders at unit scale to find its bounds, then scales into a box or circle.
function fitted(v, { box, circle, cy0 = 0 }) {
  const probe = scene(v, camera({ cx: 0, cy: 0, s: 1, elev: v.elev }), "p");
  const xs = probe.key.map((q) => q[0]), ys = probe.key.map((q) => q[1]);
  const bx = [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
  const mx = (bx[0] + bx[2]) / 2, my = (bx[1] + bx[3]) / 2;
  let s;
  if (circle) s = circle / Math.max(...probe.key.map((q) => Math.hypot(q[0] - mx, q[1] - my)));
  else s = Math.min(box / (bx[2] - bx[0]), box / (bx[3] - bx[1]));
  const cam = camera({ cx: 256 - mx * s, cy: 256 - my * s + cy0, s, elev: v.elev });
  return { cam, ...scene(v, cam, "") };
}

function plate(full, uid = "") {
  const shape = full ? `<rect width="512" height="512" fill="url(#${uid}plate)"/>` : `<rect width="512" height="512" rx="112" fill="url(#${uid}plate)"/>`;
  const defs = `<radialGradient id="${uid}plate" gradientUnits="userSpaceOnUse" cx="226" cy="200" r="380"><stop offset="0" stop-color="${C.sheetHi}"/><stop offset=".55" stop-color="${C.sheet}"/><stop offset="1" stop-color="${C.sheetLo}"/></radialGradient>`;
  const edge = full ? "" : `  <rect x="1.5" y="1.5" width="509" height="509" rx="110.5" fill="none" stroke="#ffffff" stroke-opacity=".07" stroke-width="3"/>\n`;
  return { defs, svg: `  ${shape}\n${edge}` };
}

function svgDoc(title, defs, body) {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512">\n  <title>${title}</title>\n  <defs>\n    ${defs.join("\n    ")}\n  </defs>\n${body}</svg>\n`;
}

const SAFE = 0.4 * 512; // maskable safe-zone radius
for (const [id, v] of Object.entries(VARIANTS)) {
  const full = fitted(v, { box: id === "b2" ? 452 : 400, cy0: id === "b4" ? 0 : -4 });
  const pl = plate(false);
  writeFileSync(`${out}/concept-${id}.svg`, svgDoc(v.title, [pl.defs, ...full.defs], pl.svg + full.body));
  const mask = fitted(v, { circle: SAFE - 6 });
  const worst = Math.max(...mask.key.map((q) => Math.hypot(q[0] - 256, q[1] - 256)));
  if (worst > SAFE) throw new Error(`${id} maskable content reaches ${worst.toFixed(1)} px, outside the ${SAFE} px safe circle`);
  const pm = plate(true);
  writeFileSync(`${out}/concept-${id}-maskable.svg`, svgDoc(v.title, [pm.defs, ...mask.defs], pm.svg + mask.body));
  console.log(`${id}: maskable content radius ${worst.toFixed(1)} / ${SAFE}`);
}

// ---- Small-size sources: placed by hand on a 16-unit grid -----------------------------------------
// Primary edges sit on whole units so 16, 32 and 48 px land on pixels; no plate, so the tile's own
// silhouette is the shape on dark and light browser chrome alike; no filters or gradients.
const small = (title, body) =>
  `<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 16 16">\n  <title>${title}</title>\n${body}</svg>\n`;

// B1 and B4: the pointy tile from the front right. `y0` is the cap's top vertex, `e` the length of
// its vertical edges, `h` the side depth. B4 adds the magenta slot the tile rises out of.
function pointySmall({ y0, e, h, slot = false }) {
  const capPoly = `8,${y0} 16,${y0 + 3} 16,${y0 + 3 + e} 8,${y0 + 6 + e} 0,${y0 + 3 + e} 0,${y0 + 3}`;
  const yl = y0 + 3 + e, yf = y0 + 6 + e; // side top: the left/right corners and the front vertex
  let s = `  <defs><clipPath id="c"><polygon points="${capPoly}"/></clipPath></defs>\n`;
  if (slot) {
    const t = yf - 1, b = 15.5, m = (t + b) / 2;
    s += `  <polygon points="8,${t} 16,${m} 8,${b} 0,${m}" fill="${C.shadow}" fill-opacity=".45"/>\n`;
    s += `  <polygon points="8,${t + 0.5} 15.5,${m} 8,${b} 0.5,${m}" fill="none" stroke="${C.pick}" stroke-width="1" stroke-linejoin="round"/>\n`;
  }
  s += `  <polygon points="0,${yl} 8,${yf} 8,${yf + h} 0,${yl + h}" fill="#9b6e47"/>\n`;
  s += `  <polygon points="8,${yf} 16,${yl} 16,${yl + h} 8,${yf + h}" fill="#6a4830"/>\n`;
  const st = h / 2 + 0.5;
  s += `  <polygon points="0,${yl + st} 8,${yf + st} 16,${yl + st} 16,${yl + st + 0.75} 8,${yf + st + 0.75} 0,${yl + st + 0.75}" fill="#3a2a1c" fill-opacity=".5"/>\n`;
  s += `  <polygon points="0,${yl} 8,${yf} 16,${yl} 16,${yl + 1} 8,${yf + 1} 0,${yl + 1}" fill="${C.turfShade}"/>\n`;
  s += `  <polygon points="${capPoly}" fill="${C.grass}"/>\n`;
  s += `  <polygon points="8,${y0} 16,${y0 + 3} 8,${y0 + 6} 0,${y0 + 3}" fill="${C.grassLit}" fill-opacity=".55"/>\n`;
  const S = [0, y0 + 3 + e / 2], E = [12.4, y0 + 1.6];
  const d = `M${S[0]},${S[1]} C4,${S[1] + 1.8} 9,${E[1] + 5} ${E[0]},${E[1]}`;
  s += `  <g clip-path="url(#c)">\n`;
  s += `    <path d="${d}" fill="none" stroke="${C.sand}" stroke-width="3.4"/>\n`;
  s += `    <path d="${d}" fill="none" stroke="${C.river}" stroke-width="2"/>\n`;
  s += `    <ellipse cx="13" cy="${y0 + 8.6}" rx="2" ry="1" fill="#12301b" fill-opacity=".45"/>\n`;
  s += `  </g>\n`;
  s += `  <circle cx="12" cy="${y0 + 6.6}" r="2" fill="${C.leaf}"/>\n`;
  s += `  <circle cx="11.5" cy="${y0 + 6}" r="1" fill="${C.leafLit}"/>\n`;
  return s;
}
writeFileSync(`${out}/concept-b1-small.svg`, small("Hex World", pointySmall({ y0: 1, e: 4, h: 4 })));
writeFileSync(`${out}/concept-b4-small.svg`, small("Hex World", pointySmall({ y0: 0, e: 3, h: 2.5, slot: true })));
{
  // B2: the slab end-on. Cap back edge 4..12 at y 1, front edge 4..12 at y 7; front face 7 deep.
  let s = `  <defs><clipPath id="c"><polygon points="4,1 12,1 16,4 12,7 4,7 0,4"/></clipPath></defs>\n`;
  s += `  <polygon points="0,4 4,7 4,15 0,12" fill="#9b6e47"/>\n`;
  s += `  <polygon points="4,7 12,7 12,15 4,15" fill="#87603f"/>\n`;
  s += `  <polygon points="12,7 16,4 16,12 12,15" fill="#5e412c"/>\n`;
  s += `  <polygon points="0,9 4,12 12,12 16,9 16,10 12,13 4,13 0,10" fill="#3a2a1c" fill-opacity=".4"/>\n`;
  s += `  <polygon points="0,11.5 4,14.5 12,14.5 16,11.5 16,12 12,15 4,15 0,12" fill="#3a2a1c" fill-opacity=".5"/>\n`;
  s += `  <polygon points="0,4 4,7 12,7 16,4 16,5 12,8 4,8 0,5" fill="${C.turfShade}"/>\n`;
  // river cut on the front face
  s += `  <rect x="6.5" y="7" width="3" height="1.5" fill="${C.sandShade}"/>\n`;
  s += `  <rect x="7" y="7" width="2" height="2" fill="${C.riverDeep}"/>\n`;
  s += `  <polygon points="4,1 12,1 16,4 12,7 4,7 0,4" fill="${C.grass}"/>\n`;
  s += `  <polygon points="4,1 12,1 14,2.5 2,2.5" fill="${C.grassLit}" fill-opacity=".55"/>\n`;
  s += `  <g clip-path="url(#c)">\n`;
  s += `    <path d="M2,2.5 C5,3 8,4 8,7.5" fill="none" stroke="${C.sand}" stroke-width="3.2"/>\n`;
  s += `    <path d="M2,2.5 C5,3 8,4 8,7.5" fill="none" stroke="${C.river}" stroke-width="2"/>\n`;
  s += `    <ellipse cx="10.4" cy="3.6" rx="2" ry=".9" fill="#12301b" fill-opacity=".45"/>\n`;
  s += `  </g>\n`;
  s += `  <rect x="9" y="2.4" width=".8" height="1.4" fill="${C.trunk}"/>\n`;
  s += `  <circle cx="9.4" cy="1.6" r="2" fill="${C.leaf}"/>\n`;
  s += `  <circle cx="8.9" cy="1.1" r="1" fill="${C.leafLit}"/>\n`;
  s += `  <polygon points="11,4.5 14,4.5 14,6 11,6" fill="${C.wall}"/>\n`;
  s += `  <polygon points="10.5,4.6 12.5,3 14.5,4.6" fill="${C.roof}"/>\n`;
  writeFileSync(`${out}/concept-b2-small.svg`, small("Hex World", s));
}
console.log("ok");
