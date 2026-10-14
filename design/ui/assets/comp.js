// Shared behaviour for the Hex World play-screen comps (design only, not game code).
// Tile data follows docs/superpowers/plans/2026-10-13-p2-wave1.md F1/F2. The hovered
// slot in every comp touches a River edge and a Grass edge, so only the three river
// tiles have legal rotations (2 each); everything else is dimmed with its reason.
(() => {
  const TILES = [
    { id: "river_bend", name: "River bend", s: "VGVGGG", rots: [1, 3] },
    { id: "river_straight", name: "River straight", s: "VGGVGG", rots: [0, 3] },
    { id: "river_road_bridge", name: "River bridge", s: "VRGVRG", rots: [0, 3] },
    { id: "meadow", name: "Meadow", s: "GGGGGG", why: "Needs a river edge. Grass can't cap a river." },
    { id: "forest", name: "Forest", s: "FFFFFF", why: "Needs a river edge. Forest can't cap a river." },
    { id: "house_cluster", name: "House cluster", s: "RGGGGG", why: "Needs a river edge." },
    { id: "road_straight", name: "Road straight", s: "RGGRGG", why: "Needs a river edge. Roads can't cross water here." },
    { id: "road_bend_120", name: "Road bend 120°", s: "RGRGGG", why: "Needs a river edge." },
    { id: "road_bend_60", name: "Road bend 60°", s: "RRGGGG", why: "Needs a river edge." },
    { id: "road_junction", name: "Road junction", s: "RGRGRG", why: "Needs a river edge." },
    { id: "beach", name: "Beach", s: "OOBGGB", why: "Needs a river edge." },
    { id: "ocean", name: "Ocean", s: "OOOOOO", why: "Needs a river edge, and a beach between ocean and grass." },
  ];
  const KIND = { G: "Grass", F: "Forest", R: "Road", V: "River", B: "Beach", O: "Ocean" };
  const reduce = matchMedia("(prefers-reduced-motion: reduce)");

  // Pointy-top hex, edge 0 = east, clockwise on screen.
  const R = 40, C = 50;
  const vtx = (i) => {
    const a = ((-30 + 60 * i) * Math.PI) / 180;
    return [C + R * Math.cos(a), C + R * Math.sin(a)];
  };
  const mid = (e) => {
    const [x1, y1] = vtx(e), [x2, y2] = vtx(e + 1);
    return [(x1 + x2) / 2, (y1 + y2) / 2];
  };
  const fillKind = (s) => {
    if (s === "OOOOOO") return "O";
    if (s.includes("F")) return "F";
    return "G";
  };
  function glyph(t) {
    const pts = [0, 1, 2, 3, 4, 5].map((i) => vtx(i).join(",")).join(" ");
    let edges = "", lanes = "", extra = "";
    for (let e = 0; e < 6; e++) {
      const [x1, y1] = vtx(e), [x2, y2] = vtx(e + 1);
      edges += `<line class="g-edge" x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" style="stroke:var(--k-${t.s[e]})"/>`;
      if (t.s[e] === "R" || t.s[e] === "V") {
        const [mx, my] = mid(e);
        lanes += `<line class="g-lane g-lane-${t.s[e]}" x1="${mx}" y1="${my}" x2="${C}" y2="${C}"/>`;
      }
    }
    if (t.id === "beach") extra = `<path class="g-sand" d="M${vtx(0).join(",")} L${vtx(1).join(",")} L${vtx(2).join(",")} Q${C},${C - 6} ${vtx(5).join(",")} Z"/>`;
    if (t.id === "forest") extra = [[38, 40], [60, 36], [50, 56], [34, 62], [64, 60]].map(([x, y]) => `<circle class="g-tree" cx="${x}" cy="${y}" r="7"/>`).join("");
    if (t.id === "house_cluster") extra = [[44, 44], [58, 52], [42, 60]].map(([x, y]) => `<rect class="g-house" x="${x - 5}" y="${y - 5}" width="10" height="10" rx="1.5"/>`).join("");
    if (t.s.includes("R") && t.s.includes("V")) extra = `<rect class="g-bridge" x="${C - 7}" y="${C - 7}" width="14" height="14" rx="2"/>`;
    return `<svg class="glyph" viewBox="0 0 100 100" aria-hidden="true"><g class="g-rot"><polygon class="g-fill" points="${pts}" style="fill:var(--k-${fillKind(t.s)})"/>${extra}${lanes}${edges}</g></svg>`;
  }

  const tpl = document.querySelector("template[data-tile-template]");
  const lists = { fit: document.querySelector("[data-list=fit]"), nofit: document.querySelector("[data-list=nofit]") };
  let selected = "river_bend", rotIndex = 0;

  function render() {
    if (!tpl || !lists.fit) return;
    lists.fit.innerHTML = "";
    lists.nofit.innerHTML = "";
    TILES.forEach((t, n) => {
      const node = tpl.content.firstElementChild.cloneNode(true);
      const legal = !!t.rots;
      node.dataset.tile = t.id;
      node.classList.toggle("is-legal", legal);
      node.classList.toggle("is-blocked", !legal);
      node.setAttribute("aria-pressed", String(t.id === selected));
      node.querySelector("[data-glyph]").innerHTML = glyph(t);
      node.querySelector("[data-name]").textContent = t.name;
      const meta = node.querySelector("[data-meta]");
      if (meta) meta.textContent = legal ? `${t.rots.length} rotations fit` : t.why;
      const key = node.querySelector("[data-key]");
      if (key) key.textContent = n < 9 ? String(n + 1) : "";
      const code = node.querySelector("[data-code]");
      if (code) code.innerHTML = [...t.s].map((k) => `<i style="background:var(--k-${k})" title="${KIND[k]}"></i>`).join("");
      if (!legal) node.setAttribute("aria-describedby", "");
      (legal ? lists.fit : lists.nofit).appendChild(node);
    });
    document.querySelectorAll("[data-count-fit]").forEach((el) => (el.textContent = TILES.filter((t) => t.rots).length));
    document.querySelectorAll("[data-count-nofit]").forEach((el) => (el.textContent = TILES.filter((t) => !t.rots).length));
    applyRotation(false);
    sync();
  }

  function current() { return TILES.find((t) => t.id === selected); }

  function applyRotation(animate = true) {
    const t = current();
    const deg = t.rots ? t.rots[rotIndex % t.rots.length] * 60 : 0;
    document.querySelectorAll("[data-tile].is-legal .g-rot").forEach((g) => {
      g.style.transition = animate && !reduce.matches ? "transform 220ms cubic-bezier(.2,.8,.2,1)" : "none";
      g.style.transform = `rotate(${deg}deg)`;
    });
    document.querySelectorAll("[data-ghost-variant]").forEach((p) => {
      p.hidden = Number(p.dataset.ghostVariant) !== rotIndex % 2;
    });
  }

  function sync() {
    const t = current();
    document.querySelectorAll("[data-tile]").forEach((n) => n.setAttribute("aria-pressed", String(n.dataset.tile === selected)));
    document.querySelectorAll("[data-selected-name]").forEach((el) => (el.textContent = t.name));
    document.querySelectorAll("[data-selected-glyph]").forEach((el) => (el.innerHTML = glyph(t)));
    document.querySelectorAll("[data-selected-fit]").forEach((el) => {
      el.textContent = t.rots ? `Rotation ${(rotIndex % t.rots.length) + 1} of ${t.rots.length}` : t.why;
    });
    document.body.classList.toggle("ghost-invalid", !t.rots);
  }

  function toast(text, action) {
    const el = document.querySelector("[data-toast]");
    if (!el) return;
    el.querySelector("[data-toast-text]").textContent = text;
    const btn = el.querySelector("[data-toast-action]");
    if (btn) btn.textContent = action;
    el.classList.remove("is-shown");
    void el.offsetWidth;
    el.classList.add("is-shown");
    clearTimeout(toast.t);
    toast.t = setTimeout(() => el.classList.remove("is-shown"), 4000);
  }

  function act(name, back = false) {
    const t = current();
    if (name === "rotate" && t.rots) { rotIndex = (rotIndex + (back ? t.rots.length - 1 : 1)) % t.rots.length; applyRotation(); sync(); }
    if (name === "undo") toast("Undid: placed Road bend 60° at (2, −1)", "Redo");
    if (name === "redo") toast("Redid: placed Road bend 60° at (2, −1)", "Undo");
    if (name === "inventory") document.querySelector("[data-panel]")?.classList.toggle("is-closed");
    if (name === "bulldoze") document.body.classList.toggle("is-bulldoze");
    if (name === "place") toast(t.rots ? `Placed ${t.name}` : `${t.name} doesn't fit here`, t.rots ? "Undo" : "Why?");
    document.querySelectorAll(`[data-action=${name}]`).forEach((b) => {
      b.classList.remove("is-fired");
      void b.offsetWidth;
      b.classList.add("is-fired");
    });
  }

  document.addEventListener("click", (e) => {
    const tile = e.target.closest("[data-tile]");
    if (tile) { selected = tile.dataset.tile; rotIndex = 0; applyRotation(); sync(); return; }
    const a = e.target.closest("[data-action]");
    if (a) act(a.dataset.action, e.shiftKey);
  });
  document.addEventListener("keydown", (e) => {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key.toLowerCase() === "z") { e.preventDefault(); act(e.shiftKey ? "redo" : "undo"); return; }
    if (e.key === "Tab" && !e.altKey) { e.preventDefault(); act("inventory"); return; }
    const k = e.key.toLowerCase();
    if (k === "r") act("rotate", e.shiftKey);
    if (k === "b") act("bulldoze");
    if (k === "enter") act("place");
  });

  render();
})();
