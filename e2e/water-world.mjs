// A deterministic water- and scatter-heavy play world for perf runs (P2 re-run): a hex disk grown
// breadth-first from the start meadow, east of a coastline ocean, a beach band along it, a river
// chain on the west, forests in the north and meadows with a road and houses in between. Every tile
// is placed at the rotation the game picks on its own (`place` hovers first, which snaps to the
// first legal rotation, since no `rotate` is ever sent). The builder's offline fit oracle reads the
// published catalog's edges and land sides: this tile set is level-0 flat, lane-symmetric within a
// kind and directional only on the beaches, so kind compatibility plus beach side decides it
// (ENH-002: the sheet's rules mirror is retired; hex-core validates every place command anyway).
// Cells nothing fits stay empty.
// usage: node e2e/water-world.mjs <out.json> [radius]   (also importable: waterWorld(radius))
import { writeFile } from "node:fs/promises";
import schema from "../web/ui/src/bridge-schema.json" with { type: "json" };
import { TILES, cellKey, isSlot, rotated, tileDef } from "../web/ui/src/rules.ts";

const dist = (q, r) => (Math.abs(q) + Math.abs(r) + Math.abs(q + r)) / 2;
const DIRS = [[1, 0], [0, 1], [-1, 1], [-1, 0], [0, -1], [1, -1]];
const neighbor = (c, e) => {
  const [dq, dr] = DIRS[((e % 6) + 6) % 6];
  return { q: c.q + dq, r: c.r + dr };
};
const COMPAT = schema.catalog.compatible;
/** The neighbour's facing edge's land side, null where it has none (a turn never changes a side). */
const rotatedLand = (def, k) => [0, 1, 2, 3, 4, 5].map((w) => def.land[(((w - k) % 6) + 6) % 6]);
/** What the occupant of the cell across `c`'s world edge `w` shows toward `c`, null when empty. */
const needsAt = (c, tiles) =>
  [0, 1, 2, 3, 4, 5].map((w) => {
    const n = tiles.get(cellKey(neighbor(c, w)));
    const def = n && tileDef(n.tile);
    if (!def) return null;
    const e = (w + 3) % 6;
    return { kind: rotated(def, n.rotation)[e], land: rotatedLand(def, n.rotation)[e] };
  });
/** The rotations of `def` whose every edge meets its neighbour's facing edge, land sides included. */
const fits = (def, needs) =>
  [0, 1, 2, 3, 4, 5].filter((k) =>
    needs.every((n, w) => {
      if (!n) return true;
      const theirs = COMPAT[n.kind];
      return (
        theirs.includes(rotated(def, k)[w]) &&
        (n.land === null || rotatedLand(def, k)[w] === null
          ? n.land === rotatedLand(def, k)[w]
          : n.land !== rotatedLand(def, k)[w])
      );
    }),
  );

// Preferred tiles per region, in order; the catch-all keeps the disk filled where a coast closes.
function prefs(q, r) {
  const x = q + r / 2; // east-west position in hex widths
  const y = r; // north (negative) to south
  const h = (((q * 73856093) ^ (r * 19349663)) >>> 0) % 100; // fixed per-cell variety
  if (x >= 3.5) return ["ocean", "beach_outer", "beach", "beach_inner"];
  if (x >= 2) return ["beach", "beach_inner", "beach_outer", "ocean", "meadow"];
  const river = x <= -1 && (y === Math.round(-x / 3) || (x <= -3 && y === Math.round(x / 2) - 1));
  if (river) return ["river_straight", "river_bend", "river_road_bridge", "meadow"];
  if (y <= -2) return h < 75 ? ["forest", "meadow"] : ["meadow", "forest"];
  if (y >= 3 && x < 1) return h < 20 ? ["house_cluster", "meadow"] : ["meadow", "forest"];
  return h < 50 ? ["meadow", "forest"] : ["forest", "meadow"];
}
const FALLBACK = ["meadow", "forest", "beach", "beach_inner", "beach_outer", "ocean", "river_straight", "river_bend",
  "road_straight", "road_bend_120", "house_cluster"];

export function waterWorld(radius = 8) {
  const tiles = new Map([["0,0", { q: 0, r: 0, tile: "meadow", rotation: 0 }]]);
  const cmds = [];
  // Breadth-first over the disk, ring by ring, cells in a fixed order within a ring.
  for (let pass = 0; pass < 3; pass++)
    for (let k = 1; k <= radius; k++) {
      const ring = [];
      for (let q = -k; q <= k; q++)
        for (let r = -k; r <= k; r++) if (dist(q, r) === k) ring.push({ q, r });
      ring.sort((a, b) => a.r - b.r || a.q - b.q);
      for (const c of ring) {
        if (tiles.has(cellKey(c)) || !isSlot(c, tiles)) continue;
        const needs = needsAt(c, tiles);
        const order = [...prefs(c.q, c.r), ...FALLBACK];
        for (const id of order) {
          const legal = fits(tileDef(id), needs);
          if (!legal.length) continue;
          tiles.set(cellKey(c), { ...c, tile: id, rotation: legal[0] });
          cmds.push({ cmd: "select_tile", id }, { cmd: "place", q: c.q, r: c.r });
          break;
        }
      }
    }
  const counts = {};
  for (const t of tiles.values()) counts[t.tile] = (counts[t.tile] ?? 0) + 1;
  return { tiles: [...tiles.values()], counts, commands: cmds };
}

// The perf script: build the world, frame it, let every rise and pack settle.
export function waterWorldScript(radius = 8) {
  const w = waterWorld(radius);
  const camera = { cmd: "camera", pos: [30, 300, 360], look_at: [30, 0, 0] };
  return { ...w, script: [
    camera,
    ...w.commands,
    { cmd: "hover", q: 99, r: 99 },
    camera,
    { cmd: "wait", frames: 240 },
  ] };
}

if (import.meta.url === `file://${process.argv[1]}`) {
  const { counts, tiles, script } = waterWorldScript(Number(process.argv[3] ?? 8));
  console.log(JSON.stringify({ tiles: tiles.length, counts, catalog: TILES.length }));
  if (process.argv[2]) await writeFile(process.argv[2], JSON.stringify(script) + "\n");
}
