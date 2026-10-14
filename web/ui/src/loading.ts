// Loading (S1): the app icon, a hex key that fills in the six map colours as the stages finish, the
// current stage, the bytes actually downloaded (Resource Timing), and a failure panel with Retry.
// There is no byte-level progress for Trunk's wasm fetch, so the bar sweeps instead of inventing one.
import { h, s } from "./dom.ts";
import { KIND_CLASS, hexPoints } from "./glyph.ts";
import { icon } from "./icons.ts";

type Stage = "engine" | "renderer";
const STAGES: { id: Stage; name: string; label: string }[] = [
  { id: "engine", name: "Engine", label: "Downloading the engine" },
  { id: "renderer", name: "Renderer", label: "Preparing the renderer" },
];
const MB = 1_000_000;

export class Loading {
  readonly el: HTMLElement;
  private readonly stage: HTMLElement;
  private readonly bytes: HTMLElement;
  private readonly bar: HTMLElement;
  private readonly rows = new Map<Stage, { li: HTMLElement; amt: HTMLElement }>();
  private readonly edges: SVGElement[] = [];
  private readonly fail: HTMLElement;
  private total = 0;
  private wasm = 0;
  private state: Stage | "ready" | "failed" = "engine";

  constructor(root: HTMLElement) {
    this.stage = h("b", {}, STAGES[0]!.label);
    this.bytes = h("span", {});
    this.bar = h("span", { class: "progress is-indeterminate", "aria-hidden": "true" }, h("i"));
    const done = s("g", { class: "done" });
    const v = (i: number): [number, number] => {
      const a = ((-30 + 60 * i) * Math.PI) / 180;
      return [50 + 40 * Math.cos(a), 50 + 40 * Math.sin(a)];
    };
    (["Grass", "Forest", "Road", "River", "Beach", "Ocean"] as const).forEach((k, e) => {
      const [x1, y1] = v(e + 4);
      const [x2, y2] = v(e + 5);
      const line = s("line", { class: KIND_CLASS[k], x1, y1, x2, y2 });
      this.edges.push(line);
      done.append(line);
    });
    const hex = s(
      "svg",
      { class: "load__hex", viewBox: "0 0 100 100", "aria-hidden": "true" },
      s("polygon", { class: "fill", points: hexPoints(40) }),
      s("polygon", { class: "track", points: hexPoints(40) }),
      done,
    );
    const list = h("ul", { class: "stages" });
    for (const st of STAGES) {
      const amt = h("span", {});
      const li = h("li", { "data-st": "todo" }, h("span", { class: "mark" }), h("b", {}, st.name), amt);
      this.rows.set(st.id, { li, amt });
      list.append(li);
    }
    const retry = h("button", { class: "btn btn--ink", type: "button" }, icon("retry"), "Retry");
    retry.addEventListener("click", () => location.reload());
    this.fail = h(
      "div",
      { class: "fail", role: "alert", hidden: true },
      h("p", {}),
      h(
        "div",
        { class: "btn-row" },
        retry,
        h(
          "a",
          { class: "btn btn--quiet", href: "https://github.com/paulrobello/hex-world/issues", rel: "noopener" },
          "Report a problem",
        ),
      ),
    );
    this.el = h(
      "main",
      { class: "load", role: "status", "aria-live": "polite", "aria-label": "Loading Hex World" },
      h(
        "div",
        { class: "load__panel" },
        h("img", { class: "load__icon", src: "web/brand/icon-192.png", alt: "", width: 64, height: 64 }),
        h(
          "div",
          { class: "load__head" },
          hex,
          h("div", {}, h("h1", {}, "Hex World"), h("p", { class: "load__stage" }, this.stage, this.bytes)),
        ),
        this.bar,
        list,
        this.fail,
      ),
    );
    root.append(this.el);
    this.watchBytes();
    this.show("engine");
  }

  /** Real bytes as resources finish (transfer size, or the body size when served from cache). */
  private watchBytes(): void {
    const add = (e: PerformanceResourceTiming) => {
      const n = e.transferSize || e.encodedBodySize || 0;
      this.total += n;
      if (e.name.endsWith(".wasm")) this.wasm = n;
    };
    for (const e of performance.getEntriesByType("resource")) add(e as PerformanceResourceTiming);
    try {
      new PerformanceObserver((l) => {
        for (const e of l.getEntries()) add(e as PerformanceResourceTiming);
        this.paintBytes();
      }).observe({ type: "resource" });
    } catch {
      /* Resource Timing unavailable: the byte count stays hidden. */
    }
    this.paintBytes();
  }

  private paintBytes(): void {
    this.bytes.textContent = this.total ? `${(this.total / MB).toFixed(1)} MB` : "";
    const eng = this.rows.get("engine")!.amt;
    if (this.wasm && eng.textContent !== `${(this.wasm / MB).toFixed(1)} MB`)
      eng.textContent = `${(this.wasm / MB).toFixed(1)} MB`;
  }

  show(stage: Stage): void {
    if (this.state === "failed" || this.state === "ready") return;
    this.state = stage;
    const i = STAGES.findIndex((x) => x.id === stage);
    STAGES.forEach((st, n) => {
      const row = this.rows.get(st.id)!;
      row.li.dataset.st = n < i ? "done" : n === i ? "active" : "todo";
      if (st.id === "renderer") {
        row.amt.replaceChildren(n === i ? "Compiling" : "", n === i ? h("span", { class: "working" }, "…") : "");
      }
    });
    this.stage.textContent = STAGES[i]!.label;
    this.edges.forEach((l, e) => {
      l.classList.toggle("is-on", e < (i + 1) * 2);
    });
  }

  ready(): void {
    if (this.state === "failed") return;
    this.state = "ready";
    for (const r of this.rows.values()) r.li.dataset.st = "done";
    this.rows.get("renderer")!.amt.textContent = "Ready";
    this.stage.textContent = "Ready";
    for (const l of this.edges) l.classList.add("is-on");
    this.bar.classList.remove("is-indeterminate");
    this.bar.style.setProperty("--p", "100%");
    // Input is live at once; the layer fades over the first frame of the world.
    this.el.classList.add("is-ready");
    this.el.setAttribute("aria-hidden", "true");
    window.setTimeout(() => this.el.remove(), 400);
  }

  failed(what: string, detail: string): void {
    if (this.state === "ready") return;
    const cur = this.state === "failed" ? null : this.state;
    this.state = "failed";
    if (cur) this.rows.get(cur)!.li.dataset.st = "failed";
    this.stage.textContent = "Stopped";
    this.bar.classList.remove("is-indeterminate");
    this.fail.querySelector("p")!.replaceChildren(h("b", {}, what), detail);
    this.fail.hidden = false;
  }
}
