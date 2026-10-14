// The Legend icon set (design/ui/legend/proto.js): 24 px grid, 1.8 px round-capped stroke, one sprite.
import { s } from "./dom.ts";

const ICONS: Record<string, string[]> = {
  place: ["M12 2.8 20 7.4v9.2L12 21.2 4 16.6V7.4Z", "M12 8.6v6.8M8.6 12h6.8"],
  rotate: ["M19.5 12a7.5 7.5 0 1 1-2.2-5.3", "M19.6 3.8v4.6H15"],
  undo: ["M9 14 4 9l5-5", "M4 9h10.5a5.5 5.5 0 0 1 0 11H11"],
  redo: ["m15 14 5-5-5-5", "M20 9H9.5a5.5 5.5 0 0 0 0 11H13"],
  bulldoze: ["M12 2.8 20 7.4v9.2L12 21.2 4 16.6V7.4Z", "m9 9 6 6M15 9l-6 6"],
  walk: [
    "M14.6 4.6a1.8 1.8 0 1 1-3.6 0 1.8 1.8 0 0 1 3.6 0Z",
    "m9.2 20.5 2.3-6.2 2.6 2.4.8 3.8M11.5 14.3l1-4.8M8 12l2-2.4 2.5-.1 2 2.6 2.5.8",
  ],
  photo: ["M3.5 8h3.2l1.9-2.8h6.8L17.3 8h3.2v11h-17Z", "M15.4 13.2a3.4 3.4 0 1 1-6.8 0 3.4 3.4 0 0 1 6.8 0Z"],
  settings: [
    "M4 7h8.5M17.5 7H20M4 17h2.5M11.5 17H20",
    "M17.4 7a2.4 2.4 0 1 1-4.8 0 2.4 2.4 0 0 1 4.8 0ZM11.4 17a2.4 2.4 0 1 1-4.8 0 2.4 2.4 0 0 1 4.8 0Z",
  ],
  close: ["m15 6-6 6 6 6"],
  inventory: [
    "M7.5 3.5 11 5.5v4L7.5 11.5 4 9.5v-4ZM16.5 3.5 20 5.5v4l-3.5 2L13 9.5v-4ZM12 12.5l3.5 2v4L12 20.5l-3.5-2v-4Z",
  ],
  eye: ["M3.5 12S6.7 6 12 6s8.5 6 8.5 6-3.2 6-8.5 6-8.5-6-8.5-6Z", "M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z"],
  alert: ["M12 4 21 19.5H3Z", "M12 10v4.2M12 17v.01"],
  check: ["m5 12.5 4.5 4.5L19 7.5"],
  down: ["m6.5 9.5 5.5 5.5 5.5-5.5"],
  focus: ["M4 8.5V4h4.5M15.5 4H20v4.5M20 15.5V20h-4.5M8.5 20H4v-4.5", "M14.5 12a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0Z"],
  eyeoff: [
    "M3.5 12s3.2-6 8.5-6c1.5 0 2.8.5 3.9 1.1M20.5 12s-3.2 6-8.5 6c-1.5 0-2.8-.5-3.9-1.1",
    "M9.9 14.1a3 3 0 0 1 4.2-4.2M4 20 20 4",
  ],
  exit: ["M14.5 4.5h5v15h-5", "M10 8 6 12l4 4M6 12h9.5"],
  export: ["M12 15V4M7.5 8.5 12 4l4.5 4.5", "M4.5 14v5.5h15V14"],
  retry: ["M4.5 12a7.5 7.5 0 0 0 13.3 4.7M19.5 12A7.5 7.5 0 0 0 6.2 7.3", "M6 3.5v4h4M18 20.5v-4h-4"],
  import: ["M12 4v11M7.5 10.5 12 15l4.5-4.5", "M4.5 14v5.5h15V14"],
  pencil: ["m14.5 5.5 4 4L9 19H5v-4Z", "m12.5 7.5 4 4"],
  trash: ["M4.5 7h15M9.5 7V4.5h5V7M6.5 7l1 12.5h9l1-12.5"],
  plus: ["M12 5v14M5 12h14"],
  copy: ["M8.5 8.5h11v11h-11Z", "M15.5 8.5V4.5h-11v11h4"],
  more: ["M6 12h.01M12 12h.01M18 12h.01"],
};

export function sprite(): SVGElement {
  const defs = s("defs");
  for (const [id, paths] of Object.entries(ICONS)) {
    defs.append(s("symbol", { id: `hw-i-${id}`, viewBox: "0 0 24 24" }, ...paths.map((d) => s("path", { d }))));
  }
  return s("svg", { class: "sprite", "aria-hidden": "true", focusable: "false" }, defs);
}

export const icon = (id: string, cls = "ic"): SVGElement =>
  s("svg", { class: cls, "aria-hidden": "true" }, s("use", { href: `#hw-i-${id}` }));
