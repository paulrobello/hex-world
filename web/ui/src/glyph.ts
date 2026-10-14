// A tile's picture in the sheet: the per-rotation thumbnail when `make thumbs` output was present
// at build time (assets/thumbs, baked in by build.mjs), otherwise a glyph drawn from its sockets.
import type { SocketKind } from "./bridge.ts";
import { h, s } from "./dom.ts";
import type { TileDef } from "./rules.ts";

declare const HEXW_THUMBS: { tiles: Record<string, string[]>; size: [number, number] } | null;
export const THUMBS = typeof HEXW_THUMBS === "undefined" ? null : HEXW_THUMBS;

export const KIND_CLASS: Record<SocketKind, string> = {
  Grass: "k-G",
  Forest: "k-F",
  Road: "k-R",
  River: "k-V",
  Beach: "k-B",
  Ocean: "k-O",
};

const R = 40;
const C = 50;
const vertex = (i: number, r = R): [number, number] => {
  const a = ((-30 + 60 * i) * Math.PI) / 180;
  return [C + r * Math.cos(a), C + r * Math.sin(a)];
};
export const hexPoints = (r = R): string =>
  [0, 1, 2, 3, 4, 5]
    .map((i) =>
      vertex(i, r)
        .map((v) => v.toFixed(1))
        .join(","),
    )
    .join(" ");

function fillKind(def: TileDef): SocketKind {
  if (def.edges.every((e) => e === "Ocean")) return "Ocean";
  return def.edges.includes("Forest") ? "Forest" : "Grass";
}

/** The socket glyph at rotation 0; `turn` rotates its inner group (60° per rotation step). */
export function glyph(def: TileDef): SVGElement {
  const g = s("g", { class: "g-rot" });
  g.append(s("polygon", { class: `g-fill ${KIND_CLASS[fillKind(def)]}`, points: hexPoints() }));
  if (def.id === "beach") {
    const [a, b, c, f] = [vertex(0), vertex(1), vertex(2), vertex(5)];
    g.append(s("path", { class: "g-sand", d: `M${a} L${b} L${c} Q${C},${C - 6} ${f} Z` }));
  }
  if (def.id === "forest")
    for (const [x, y] of [
      [38, 40],
      [60, 36],
      [50, 56],
      [34, 62],
      [64, 60],
    ] as const)
      g.append(s("circle", { class: "g-tree", cx: x, cy: y, r: 7 }));
  if (def.id === "house_cluster")
    for (const [x, y] of [
      [44, 44],
      [58, 52],
      [42, 60],
    ] as const)
      g.append(s("rect", { class: "g-house", x: x - 5, y: y - 5, width: 10, height: 10, rx: 1.5 }));
  for (let e = 0; e < 6; e++) {
    const k = def.edges[e]!;
    if (k !== "Road" && k !== "River") continue;
    const [x1, y1] = vertex(e);
    const [x2, y2] = vertex(e + 1);
    g.append(s("line", { class: `g-lane ${KIND_CLASS[k]}`, x1: (x1 + x2) / 2, y1: (y1 + y2) / 2, x2: C, y2: C }));
  }
  if (def.edges.includes("Road") && def.edges.includes("River"))
    g.append(s("rect", { class: "g-bridge", x: C - 7, y: C - 7, width: 14, height: 14, rx: 2 }));
  for (let e = 0; e < 6; e++) {
    const [x1, y1] = vertex(e);
    const [x2, y2] = vertex(e + 1);
    g.append(s("line", { class: `g-edge ${KIND_CLASS[def.edges[e]!]}`, x1, y1, x2, y2 }));
  }
  return s("svg", { class: "glyph", viewBox: "0 0 100 100", "aria-hidden": "true" }, g);
}

/** The thumbnail URL for a tile at rotation k, or null without baked thumbnails. */
export function thumbUrl(id: string, k: number): string | null {
  const file = THUMBS?.tiles[id]?.[((k % 6) + 6) % 6];
  return file ? `assets/thumbs/${file}` : null;
}

/** A row picture that can turn: an <img> swapped per rotation, or a glyph rotated in CSS. */
export class TilePicture {
  readonly el: HTMLElement;
  private readonly img: HTMLImageElement | null;
  private readonly rot: SVGElement | null;
  private readonly def: TileDef;
  private k = -1;

  constructor(def: TileDef) {
    this.def = def;
    this.el = h("span", { class: "row__glyph" });
    if (thumbUrl(def.id, 0)) {
      this.img = h("img", { class: "row__thumb", alt: "", width: 48, height: 48, decoding: "async" });
      this.el.append(this.img);
      this.rot = null;
    } else {
      const svg = glyph(def);
      this.el.append(svg);
      this.img = null;
      this.rot = svg.querySelector(".g-rot");
    }
    this.show(0);
  }

  show(k: number): void {
    if (k === this.k) return;
    this.k = k;
    if (this.img) this.img.src = thumbUrl(this.def.id, k)!;
    else if (this.rot) (this.rot as SVGGElement).style.transform = `rotate(${k * 60}deg)`;
  }
}
