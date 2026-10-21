// A tile's picture in the sheet: the per-rotation thumbnail when `make thumbs` output was present
// at build time (assets/thumbs, baked in by build.mjs), otherwise a glyph drawn from its sockets.
import type { SocketKind } from "./bridge.ts";
import { h, motionMs, reduceMotion, s } from "./dom.ts";
import { type TileDef, turnAngle } from "./rules.ts";
import { versioned } from "./versioned.ts";

declare const HEXW_THUMBS: { tiles: Record<string, string[]>; size: [number, number] } | null;
export const THUMBS = typeof HEXW_THUMBS === "undefined" ? null : HEXW_THUMBS;

export const KIND_CLASS: Record<SocketKind, string> = {
  Grass: "k-G",
  Forest: "k-F",
  Road: "k-R",
  River: "k-V",
  Beach: "k-B",
  Ocean: "k-O",
  Rail: "k-T",
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
  if (def.id === "oak") g.append(s("circle", { class: "g-tree", cx: C, cy: C, r: 15 }));
  if (def.id === "windmill") g.append(s("circle", { class: "g-house", cx: C, cy: C, r: 8 }));
  if (def.id === "farm")
    g.append(
      s("rect", { class: "g-house", x: 54, y: 24, width: 16, height: 11, rx: 1.5, transform: "rotate(30 62 29.5)" }),
    );
  if (def.id === "wheat_field")
    for (const y of [38, 46, 54, 62]) g.append(s("line", { class: "g-wheat", x1: 30, y1: y, x2: 70, y2: y }));
  if (def.id === "bakery") g.append(s("rect", { class: "g-house", x: 30, y: 24, width: 14, height: 18, rx: 1.5 }));
  if (def.id === "house_cluster")
    for (const [x, y] of [
      [44, 44],
      [58, 52],
      [42, 60],
    ] as const)
      g.append(s("rect", { class: "g-house", x: x - 5, y: y - 5, width: 10, height: 10, rx: 1.5 }));
  for (let e = 0; e < 6; e++) {
    const k = def.edges[e]!;
    if (k !== "Road" && k !== "River" && k !== "Rail") continue;
    const [x1, y1] = vertex(e);
    const [x2, y2] = vertex(e + 1);
    g.append(s("line", { class: `g-lane ${KIND_CLASS[k]}`, x1: (x1 + x2) / 2, y1: (y1 + y2) / 2, x2: C, y2: C }));
  }
  if (new Set(def.edges.filter((k) => k === "Road" || k === "River" || k === "Rail")).size > 1)
    g.append(s("rect", { class: "g-bridge", x: C - 7, y: C - 7, width: 14, height: 14, rx: 2 }));
  for (let e = 0; e < 6; e++) {
    const [x1, y1] = vertex(e);
    const [x2, y2] = vertex(e + 1);
    g.append(s("line", { class: `g-edge ${KIND_CLASS[def.edges[e]!]}`, x1, y1, x2, y2 }));
  }
  return s("svg", { class: "glyph", viewBox: "0 0 100 100", "aria-hidden": "true" }, g);
}

/**
 * The tile's sockets as a hex in the slot key's geometry, one coloured line per edge, at rotation 0;
 * its inner `.g-rot` group turns with the row picture.
 */
export function socketHex(def: TileDef): SVGElement {
  const g = s("g", { class: "g-rot" }, s("polygon", { class: "key__slot", points: hexPoints(34) }));
  for (let e = 0; e < 6; e++) {
    const [x1, y1] = vertex(e, 34);
    const [x2, y2] = vertex(e + 1, 34);
    g.append(s("line", { class: `key__edge ${KIND_CLASS[def.edges[e]!]}`, x1, y1, x2, y2 }));
  }
  return s("svg", { class: "row__hex", viewBox: "0 0 100 100", "aria-hidden": "true" }, g);
}

/** The thumbnail URL for a tile at rotation k, or null without baked thumbnails. */
export function thumbUrl(id: string, k: number): string | null {
  const file = THUMBS?.tiles[id]?.[((k % 6) + 6) % 6];
  return file ? versioned(`assets/thumbs/${file}`) : null;
}

/**
 * How a thumbnail turns on R (owner 2026-10-21): the old and new rotation's pictures crossfade while
 * both turn TURN_DEG in the press direction. The renders look down at 58°, so the turn is a rotation
 * in the ground plane: unsquash the hex top (THUMB_TILT), rotate, squash it back.
 */
const TURN_DEG = 30;
const THUMB_TILT = 0.82;
const EASE = "cubic-bezier(.22,.9,.3,1)";
const plane = (deg: number): string => `scaleY(${THUMB_TILT}) rotate(${deg}deg) scaleY(${1 / THUMB_TILT})`;

/** A row picture that can turn: an <img> swapped per rotation, or a glyph rotated in CSS. */
export class TilePicture {
  readonly el: HTMLElement;
  private readonly img: HTMLImageElement | null;
  private readonly rot: SVGElement | null;
  private readonly def: TileDef;
  private k = -1;
  private deg = 0;
  /** The outgoing picture and both animations while a turn runs. */
  private turning: { old: HTMLImageElement; anims: Animation[] } | null = null;

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

  /**
   * Shows rotation k. `dir` (1 clockwise, -1 back) turns the thumbnail into it under full motion; 0,
   * or reduced motion, swaps it at once. A turn never holds anything back: a new rotation mid-turn
   * drops the running one and starts again from the picture on screen. `prev` is the picture to turn
   * from when another group member's picture was on screen.
   */
  show(k: number, dir = 0, prev: string | null = null): void {
    if (k === this.k && !(dir && prev)) return;
    const first = this.k < 0;
    this.k = k;
    if (this.img) {
      const from = prev ?? this.img.getAttribute("src");
      this.stopTurn();
      this.img.src = thumbUrl(this.def.id, k)!;
      if (dir && !first && from && !reduceMotion()) this.turn(from, Math.sign(dir));
    } else if (this.rot) {
      this.deg = turnAngle(this.deg, k);
      (this.rot as SVGGElement).style.transform = `rotate(${this.deg}deg)`;
    }
  }

  /** The thumbnail on screen, or null for a glyph. */
  get src(): string | null {
    return this.img?.getAttribute("src") ?? null;
  }

  private turn(from: string, dir: number): void {
    const img = this.img!;
    const old = h("img", { class: "row__thumb row__thumb--out", alt: "", width: 48, height: 48, src: from });
    this.el.append(old);
    const timing = { duration: motionMs("--t", 180), easing: EASE };
    const a = TURN_DEG * dir;
    const out = old.animate(
      [
        { opacity: 1, transform: plane(0) },
        { opacity: 0, transform: plane(a) },
      ],
      { ...timing, fill: "forwards" },
    );
    const into = img.animate(
      [
        { opacity: 0, transform: plane(-a) },
        { opacity: 1, transform: plane(0) },
      ],
      timing,
    );
    const turning = { old, anims: [out, into] };
    this.turning = turning;
    out.onfinish = () => {
      if (this.turning === turning) this.stopTurn();
    };
  }

  private stopTurn(): void {
    if (!this.turning) return;
    for (const a of this.turning.anims) a.cancel();
    this.turning.old.remove();
    this.turning = null;
  }
}
