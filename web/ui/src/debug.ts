// Debug tab (owner request 2026-10-14) and its on-screen readouts. The tab sends the `debug` command
// and reads `state().debug` back every frame while open, so the `debug=` flag and scripts show here
// too. Nothing is stored: every overlay starts off on each load and never queues an autosave.
// The performance panel and the grid's axial labels are drawn by `DebugHud`, which the Hud renders
// every frame whether or not the Settings sheet is open.
import type { Command, DebugState, ForceLod, HexWorld, PlayState, State } from "./bridge.ts";
import { h } from "./dom.ts";
import { icon } from "./icons.ts";
import { tileRise } from "./rules.ts";
import { seg } from "./photo.ts";

type Switch = "lod_tint" | "perf" | "grid" | "sockets" | "colliders" | "scatter" | "networks";

const SWITCHES: { id: Switch; label: string; hint: string }[] = [
  { id: "lod_tint", label: "LOD tint", hint: "Colours every kit piece by the level it draws at." },
  { id: "perf", label: "Performance", hint: "Frame rate, frame time, draws and GPU passes in the top left corner." },
  { id: "grid", label: "Hex grid", hint: "Tile and free slot outlines with their axial q, r." },
  { id: "sockets", label: "Edge sockets", hint: "Each placed tile's six edges in their socket colours." },
  { id: "colliders", label: "Colliders", hint: "Walk obstacle footprints, where walking stops." },
  {
    id: "scatter",
    label: "Scatter footprints",
    hint: "Each scattered prop's ground footprint, and the wider disc grass keeps clear around it.",
  },
  {
    id: "networks",
    label: "Road and river graph",
    hint: "The routing links cars and rivers follow; white rings are car destinations.",
  },
];

/** Levels, their distance bands and the swatch class (the engine's `LOD_TINTS`, viridis stops). */
const LODS = [
  { name: "LOD 0", band: "Under 150 m" },
  { name: "LOD 1", band: "150 to 400 m" },
  { name: "LOD 2", band: "Beyond 400 m" },
];

const FORCE: { value: ForceLod; text: string }[] = [
  { value: "auto", text: "Auto" },
  { value: "0", text: "0" },
  { value: "1", text: "1" },
  { value: "2", text: "2" },
];

const num = (n: number): string => n.toLocaleString("en-US");

/** The Debug tab's panel. */
export class DebugPanel {
  readonly el: HTMLElement;
  private readonly hw: HexWorld;
  private readonly switches = new Map<Switch, HTMLButtonElement>();
  private readonly force: ReturnType<typeof seg<ForceLod>>;
  private readonly counts: HTMLElement[] = [];
  private sig = "";

  constructor(hw: HexWorld) {
    this.hw = hw;
    this.force = seg<ForceLod>("Force LOD", FORCE, (v) => this.send({ cmd: "debug", force_lod: v }));
    const legend = h("ul", { class: "lod-legend", "aria-label": "LOD tint key" });
    LODS.forEach((l, i) => {
      const count = h("span", { class: "lod-legend__count" }, "0");
      this.counts.push(count);
      count.setAttribute("aria-label", "drawn parts");
      legend.append(
        h(
          "li",
          { class: "lod-legend__row", "data-lod": i },
          h("span", { class: `lod-swatch lod-swatch--${i}`, "aria-hidden": "true" }),
          h("b", {}, l.name),
          h("small", {}, l.band),
          count,
        ),
      );
    });
    const field = (id: Switch) => {
      const s = SWITCHES.find((x) => x.id === id)!;
      const btn = h("button", {
        class: "switch",
        type: "button",
        role: "switch",
        "aria-checked": "false",
        "aria-label": s.label,
        "data-debug": id,
      });
      btn.addEventListener("click", () => {
        const on = btn.getAttribute("aria-checked") !== "true";
        btn.setAttribute("aria-checked", String(on));
        this.send({ cmd: "debug", [id]: on } as Command);
      });
      this.switches.set(id, btn);
      return h(
        "div",
        { class: "field" },
        h("span", { class: "field__label" }, s.label),
        btn,
        h("span", { class: "field__hint" }, s.hint),
      );
    };
    this.el = h(
      "div",
      { class: "panel panel--debug", role: "tabpanel", hidden: true },
      h("h2", { class: "section-h" }, "Level of detail"),
      field("lod_tint"),
      legend,
      h(
        "div",
        { class: "field field--force" },
        h("span", { class: "field__label", id: "force-lod-h" }, "Force LOD"),
        this.force.el,
        h("span", { class: "field__hint" }, "Pins every kit piece to one level to inspect its quality."),
      ),
      h("h2", { class: "section-h" }, "Readouts"),
      field("perf"),
      h("h2", { class: "section-h" }, "World"),
      field("grid"),
      field("sockets"),
      field("colliders"),
      field("scatter"),
      field("networks"),
      h(
        "div",
        { class: "note" },
        icon("alert"),
        h("div", {}, h("b", {}, "Developer overlays"), "They start off on every load and are never saved."),
      ),
    );
    this.force.el.removeAttribute("aria-label");
    this.force.el.setAttribute("aria-labelledby", "force-lod-h");
    this.force.set("auto");
  }

  private send(c: Command): void {
    this.hw.cmd(c);
    this.sig = "";
  }

  render(d: DebugState | undefined): void {
    if (!d) return;
    const sig = JSON.stringify([
      d.lod_tint,
      d.force_lod,
      d.perf,
      d.grid,
      d.sockets,
      d.colliders,
      d.scatter,
      d.networks,
      d.lod.by_lod,
    ]);
    if (sig === this.sig) return;
    this.sig = sig;
    for (const [id, btn] of this.switches) btn.setAttribute("aria-checked", String(d[id]));
    this.force.set(d.force_lod);
    // The census runs only while LOD tint, Force LOD or perf is on; otherwise there is no count to show.
    const counting = d.lod_tint || d.force_lod !== "auto" || d.perf;
    d.lod.by_lod.forEach((n, i) => {
      this.counts[i]!.textContent = counting ? num(n) : "–";
    });
  }
}

/** The performance corner and the grid's axial labels, drawn every frame while their overlay is on. */
export class DebugHud {
  readonly perf: HTMLElement;
  readonly labels: HTMLElement;
  private readonly rows = new Map<string, HTMLElement>();
  private readonly pool: HTMLElement[] = [];
  private perfAt = 0;

  constructor() {
    this.perf = h("aside", { class: "debug-perf", "aria-label": "Performance", hidden: true });
    this.labels = h("div", { class: "debug-labels", "aria-hidden": "true", hidden: true });
  }

  render(hw: HexWorld, state: State, play: PlayState | null): void {
    const d = state.debug;
    this.perf.hidden = !d?.perf;
    this.labels.hidden = !d?.grid || !play;
    if (d?.perf) this.renderPerf(state, d);
    if (d?.grid && play) this.renderLabels(hw, play);
  }

  private row(key: string, label: string, value: string): void {
    let el = this.rows.get(key);
    if (!el) {
      el = h("div", { class: "debug-perf__row" }, h("span", {}, label), h("b", {}));
      this.rows.set(key, el);
      this.perf.append(el);
    }
    el.lastElementChild!.textContent = value;
  }

  /** Text rebuilt at 4 Hz, so the readout itself stays off the per-frame path. */
  private renderPerf(state: State, d: DebugState): void {
    const now = performance.now();
    if (now - this.perfAt < 250) return;
    this.perfAt = now;
    const f = d.frame;
    const lod = d.lod;
    const parts = lod.by_lod.reduce((a, b) => a + b, 0);
    const tris = lod.tris_by_lod.reduce((a, b) => a + b, 0);
    this.row("fps", "Frame rate", `${Math.round(f.fps)} fps`);
    this.row("ms", "Frame time", `${f.mean_ms.toFixed(1)} ms, worst ${f.worst_ms.toFixed(1)}`);
    this.row("entities", "Entities", num(f.entities));
    this.row("kit", "Kit draws", `${num(parts)} (${lod.by_lod.map(num).join(" / ")})`);
    this.row("tris", "Kit triangles", num(tris));
    const g = state.grass;
    this.row("grass", "Grass blades", g ? (g.status === "on" ? num(g.blades) : g.status) : "none");
    const gpu = state.diag?.gpu;
    const passes = gpu?.passes ? Object.entries(gpu.passes).sort((a, b) => b[1].mean - a[1].mean) : [];
    if (passes.length) {
      const total = gpu?.total?.mean;
      const top = passes
        .slice(0, 3)
        .map(([k, v]) => `${k} ${v.mean.toFixed(1)}`)
        .join(", ");
      this.row("gpu", "GPU passes", `${total != null ? `${total.toFixed(1)} ms: ` : ""}${top}`);
    } else {
      this.row("gpu", "GPU passes", gpu?.enabled ? "Waiting for samples" : "Off. Add gpu_timing=on to the URL");
    }
  }

  private renderLabels(hw: HexWorld, play: PlayState): void {
    const cells = [
      ...play.tiles.map((t) => ({ q: t.q, r: t.r, level: t.level, rise: tileRise(t.tile), slot: false })),
      ...play.frontier.map((c) => ({
        ...c,
        level: play.slot_levels.find((s) => s.q === c.q && s.r === c.r)?.level ?? 0,
        rise: 0,
        slot: true,
      })),
    ];
    let n = 0;
    for (const c of cells) {
      const rect = hw.cellRect(c.q, c.r, c.level, c.rise);
      if (!rect) continue;
      let el = this.pool[n];
      if (!el) {
        el = h("span", { class: "debug-label" });
        this.pool.push(el);
        this.labels.append(el);
      }
      el.hidden = false;
      el.textContent = `${c.q}, ${c.r}`;
      el.classList.toggle("debug-label--slot", c.slot);
      el.style.transform = `translate(${Math.round((rect[0] + rect[2]) / 2)}px, ${Math.round((rect[1] + rect[3]) / 2)}px) translate(-50%, -50%)`;
      n++;
    }
    for (let i = n; i < this.pool.length; i++) this.pool[i]!.hidden = true;
  }
}
