// Hex World, Legend: the play screen prototype. Reads window.WORLD (backdrop, slots, tiles, hints)
// from the page and drives the sheet, ghost, rotation, place, undo/redo and bulldoze.
(() => {
  const W = window.WORLD;
  const $ = (s, r = document) => r.querySelector(s);
  const $$ = (s, r = document) => [...r.querySelectorAll(s)];
  const NS = "http://www.w3.org/2000/svg";
  const st = {
    selected: W.selected || "river_bend",
    slot: null, // last slot pointed at; the filter sticks to it while the pointer is on the sheet
    ri: 0, // index into legal rotations (auto-rotate) or raw k (manual / no fit)
    manual: false,
    mode: "place",
    filled: new Set(),
    undo: (W.history || []).slice(),
    redo: [],
    toast: null,
    rising: [],
  };

  // ---------- Scene ----------
  const scene = $(".scene");
  scene.style.setProperty("--scene-x", `${W.scene.x}px`);
  scene.style.setProperty("--scene-y", `${W.scene.y}px`);
  scene.style.setProperty("--scene-s", W.scene.s);
  const img = scene.querySelector("img");
  img.src = W.scene.src;
  if (W.scene.clip) img.style.clipPath = W.scene.clip;
  const svg = scene.querySelector("svg.overlay");
  const layer = (cls) => { const g = document.createElementNS(NS, "g"); g.setAttribute("class", cls); svg.appendChild(g); return g; };
  const gTiles = layer("tiles"), gPlaced = layer("placed-layer"), gSlots = layer("slots"), gGhost = layer("ghost-g");
  const P = (pts) => pts.map((p) => p.join(",")).join(" ");
  // A region of another native capture taken at the same camera (a placed tile, a removed one),
  // clipped to the cell's prism outline so the engine's own pixels stand in the scene.
  let clipN = 0;
  function capture(parent, cls, src, hull) {
    const g = document.createElementNS(NS, "g");
    g.setAttribute("class", cls);
    const id = `cap-clip-${clipN++}`;
    g.innerHTML = `<clipPath id="${id}"><polygon points="${P(hull)}"/></clipPath><image href="${src}" width="1600" height="1000" clip-path="url(#${id})"/>`;
    parent.appendChild(g);
    return g;
  }
  const centre = (pts) => [pts.reduce((a, p) => a + p[0], 0) / 6, pts.reduce((a, p) => a + p[1], 0) / 6];
  const side = (pts, e) => [pts[(e + 2) % 6], pts[(e + 3) % 6]];
  const mid = ([a, b]) => [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];

  W.slots.forEach((s) => {
    const p = document.createElementNS(NS, "polygon");
    p.setAttribute("class", "slot");
    p.setAttribute("points", P(s.pts));
    p.dataset.slot = s.id;
    gSlots.appendChild(p);
    s.el = p;
  });
  (W.tiles || []).forEach((t) => {
    const mk = (cls, pts) => { const p = document.createElementNS(NS, "polygon"); p.setAttribute("class", cls); p.setAttribute("points", P(pts)); gTiles.appendChild(p); return p; };
    t.cutEls = (t.cut || []).map((c) => mk("t-cut", c));
    t.gone = t.goneSrc ? capture(gTiles, "t-gone t-gone--real", t.goneSrc, t.hull) : mk("t-gone", t.pts);
    t.out = mk("t-out", t.pts);
    t.hit = mk("t-hit", t.pts);
    t.hit.dataset.tile = t.id;
  });

  // ---------- Sheet rows: one node per tile, identity kept across slots ----------
  const lists = { fit: $("[data-list=fit]"), off: $("[data-list=off]") };
  const rows = new Map();
  HW.TILES.forEach((t, i) => {
    const li = document.createElement("li");
    li.className = "row";
    li.tabIndex = 0;
    li.setAttribute("role", "button");
    li.dataset.tile = t.id;
    li.innerHTML = `<span class="row__glyph">${HW.glyph(t)}</span><span class="row__text"><span class="row__name">${t.name}</span><span class="row__meta"></span></span><span class="row__code">${HW.code(t.s)}</span><span class="row__num" aria-hidden="true"></span>`;
    rows.set(t.id, li);
    lists.fit.appendChild(li);
  });

  const slotById = (id) => W.slots.find((s) => s.id === id);
  // A slot's needs: its placed neighbours from the backdrop, plus any tile the player placed beside it.
  st.placedBy = {};
  function needs(slot) {
    const n = { ...slot.needs };
    for (const [e, sid] of Object.entries(slot.adj || {})) {
      const p = st.placedBy[sid];
      if (p) n[e] = HW.rot(HW.tile(p.tile).s, p.k)[(+e + 3) % 6];
    }
    return n;
  }
  const tileFit = (t) => (st.slot ? HW.fit(t, needs(st.slot)) : { ok: true, rots: [0, 1, 2, 3, 4, 5], meta: "" });

  function renderKey() {
    const key = $("[data-key]");
    if (st.mode === "bulldoze") {
      key.className = "key";
      key.innerHTML = `<svg class="key__hex" viewBox="0 0 100 100" aria-hidden="true"><polygon points="${HW.hexPts(34)}" fill="rgb(255 93 82 / .15)" stroke="var(--bad)" stroke-width="5"/><path d="M40 40l20 20M60 40 40 60" stroke="var(--bad)" stroke-width="5" stroke-linecap="round"/></svg><div><p class="key__title">Bulldoze</p><ul class="key__list"><li><span class="key__swatch key__swatch--remove"></span>Click to remove</li><li><span class="key__swatch key__swatch--blocked"></span>Would split the world</li><li><span class="key__swatch key__swatch--cut"></span>The part it would cut off</li></ul></div>`;
      return;
    }
    if (!st.slot) {
      key.className = "key key--empty";
      key.innerHTML = `${HW.keyHex({})}<div><p class="key__title">Point at a dashed slot</p><p class="empty-key">The sheet sorts every tile by whether it fits there.</p></div>`;
      return;
    }
    const lines = HW.needLines(needs(st.slot));
    const anyFit = HW.TILES.some((t) => HW.fit(t, needs(st.slot)).ok);
    key.className = `key ${anyFit ? "" : "key--none"}`;
    const near = !anyFit && st.slot.nearest ? slotById(st.slot.nearest) : null;
    const nearCount = near ? HW.TILES.filter((t) => HW.fit(t, needs(near)).ok).length : 0;
    key.innerHTML = `${HW.keyHex(needs(st.slot), { none: !anyFit })}<div><p class="key__title">${anyFit ? "This slot needs" : "Nothing fits this slot"}</p><ul class="key__list">${lines.map((l) => `<li><i style="background:var(--k-${l.kind})"></i>${l.text}</li>`).join("")}</ul>${anyFit ? "" : `<p class="empty-key">${st.slot.noneWhy || "No tile has both."}</p>${near ? `<div class="key__more"><button class="btn btn--sm" type="button" data-goto="${near.id}">${HW.icon("focus")}Show the nearest slot</button><span class="empty-key" style="margin:0">${nearCount} tiles fit there</span></div>` : ""}`}</div>`;
  }

  function renderSheet(animate = true) {
    renderKey();
    const first = new Map();
    if (animate && !HW.instant()) rows.forEach((li, id) => first.set(id, li.getBoundingClientRect().top));
    const fits = [], offs = [];
    HW.TILES.forEach((t) => {
      const f = tileFit(t);
      const li = rows.get(t.id);
      li.classList.toggle("is-dimmed", !f.ok);
      li.querySelector(".row__meta").textContent = st.slot ? (f.ok ? f.meta : f.why) : HW.summary(t.s);
      li.setAttribute("aria-pressed", String(t.id === st.selected));
      li.setAttribute("aria-label", `${t.name}. ${st.slot ? (f.ok ? f.meta : "Doesn't fit here. " + f.why) : ""}`);
      (f.ok ? fits : offs).push(li);
    });
    const all = !st.slot;
    lists.fit.classList.toggle("list--all", all);
    fits.forEach((li, i) => { lists.fit.appendChild(li); li.querySelector(".row__num").textContent = i < 9 ? String(i + 1) : ""; });
    offs.forEach((li) => lists.off.appendChild(li));
    $("[data-group=fit]").firstChild.textContent = all ? "All tiles " : "Fits here ";
    $("[data-count=fit]").textContent = fits.length;
    $("[data-count=off]").textContent = offs.length;
    $("[data-group=fit]").hidden = !fits.length;
    $("[data-group=off]").hidden = !offs.length;
    if (first.size) {
      rows.forEach((li, id) => {
        const dy = first.get(id) - li.getBoundingClientRect().top;
        if (!dy) return;
        li.animate([{ transform: `translateY(${dy}px)` }, { transform: "none" }], { duration: 260, easing: "cubic-bezier(.22,.9,.3,1)" });
      });
    }
    applyGlyphRotation(false);
  }

  // ---------- Ghost ----------
  const tag = $(".ghost-tag");
  function currentK(t, f) {
    if (f.ok && !st.manual) return f.rots[st.ri % f.rots.length];
    if (f.ok && st.manual) return st.ri % 6;
    return (f.bestK + st.ri) % 6;
  }
  function applyGlyphRotation(animate = true) {
    const t = HW.tile(st.selected);
    const f = tileFit(t);
    const deg = st.slot && f.ok ? currentK(t, f) * 60 : 0;
    $$(".row:not(.is-dimmed) .g-rot").forEach((g) => {
      g.style.transition = animate && !HW.instant() ? "transform 220ms cubic-bezier(.22,.9,.3,1)" : "none";
      g.style.transform = `rotate(${deg}deg)`;
    });
  }
  function drawGhost() {
    gGhost.innerHTML = "";
    gGhost.setAttribute("class", "ghost-g");
    const t = HW.tile(st.selected);
    document.body.classList.remove("ghost-invalid");
    if (st.mode !== "place") return;
    if (!st.slot || st.filled.has(st.slot.id)) { tag.classList.add("is-gone"); return; }
    const f = HW.fit(t, needs(st.slot));
    const k = currentK(t, f);
    const r = HW.rot(t.s, k);
    const states = HW.edgeState(t, k, needs(st.slot));
    const pts = st.slot.pts, c = centre(pts);
    let html = `<polygon class="g-body" points="${P(pts)}"/>`;
    for (const kind of ["V", "R"]) {
      const es = [0, 1, 2, 3, 4, 5].filter((e) => r[e] === kind);
      if (es.length === 2) {
        const a = mid(side(pts, es[0])), b = mid(side(pts, es[1]));
        html += `<path class="g-lane is-${kind}" d="M${a} Q${c} ${b}"/>`;
      } else es.forEach((e) => { html += `<path class="g-lane is-${kind}" d="M${mid(side(pts, e))} L${c}"/>`; });
    }
    states.forEach((s, e) => {
      const [a, b] = side(pts, e);
      html += `<line class="g-edge is-${s}" x1="${a[0]}" y1="${a[1]}" x2="${b[0]}" y2="${b[1]}"/>`;
      if (s === "bad") { const m = mid([a, b]); html += `<path class="g-tick" d="M${m[0] - 7},${m[1] - 7}l14,14M${m[0] + 7},${m[1] - 7}l-14,14"/>`; }
      // Socket key mark: the neighbour's map colour along the middle of the touched edge, just inside
      // the slot, so the in-world edge reads in the same code as the sheet's slot key.
      const need = needs(st.slot)[e];
      if (need) {
        const m = mid([a, b]), dx = c[0] - m[0], dy = c[1] - m[1], len = Math.hypot(dx, dy) || 1, o = 11;
        const ox = (dx / len) * o, oy = (dy / len) * o;
        const p1 = [a[0] + (b[0] - a[0]) * 0.3 + ox, a[1] + (b[1] - a[1]) * 0.3 + oy], p2 = [a[0] + (b[0] - a[0]) * 0.7 + ox, a[1] + (b[1] - a[1]) * 0.7 + oy];
        html += `<line class="g-key" x1="${p1[0]}" y1="${p1[1]}" x2="${p2[0]}" y2="${p2[1]}" style="stroke:var(--k-${need})"/>`;
      }
    });
    gGhost.innerHTML = html;
    const bad = states.includes("bad");
    document.body.classList.toggle("ghost-invalid", bad);
    const clashes = states.filter((s) => s === "bad").length;
    let fitText;
    if (f.ok && !st.manual) fitText = f.rots.length === 1 ? "Fits" : `Rotation ${(st.ri % f.rots.length) + 1} of ${f.rots.length}`;
    else if (f.ok && st.manual) fitText = bad ? `${clashes} edge${clashes > 1 ? "s" : ""} clash · auto-rotate off` : `Rotation fits · auto-rotate off`;
    else fitText = f.why;
    tag.innerHTML = `${bad ? HW.icon("alert") : ""}<span>${t.name}</span><span class="ghost-tag__fit ${bad ? "is-bad" : ""}">${fitText}</span>${(f.ok && f.rots.length > 1) || st.manual ? "<kbd>R</kbd>" : ""}`;
    tag.classList.toggle("ghost-tag--bad", bad);
    tag.classList.remove("is-gone");
    placeTag(pts);
  }
  function placeTag(pts) {
    const rect = scene.getBoundingClientRect();
    const s = rect.width / 1600;
    const right = pts.reduce((m, p) => (p[0] > m[0] ? p : m), pts[0]);
    const cy = centre(pts)[1];
    let x = rect.left + right[0] * s + 14, y = rect.top + cy * s - 20;
    tag.style.left = `${Math.round(x)}px`;
    tag.style.top = `${Math.round(y)}px`;
    const tr = tag.getBoundingClientRect();
    const limit = innerWidth - ($(".sheet:not(.is-closed)") ? 420 : 24);
    if (tr.right > limit) { const b = pts.reduce((m, p) => (p[1] > m[1] ? p : m), pts[0]); tag.style.left = `${Math.round(Math.max(20, Math.min(limit - tr.width, rect.left + centre(pts)[0] * s - tr.width / 2)))}px`; tag.style.top = `${Math.round(rect.top + b[1] * s + 18)}px`; }
  }

  // ---------- Actions ----------
  const tool = (a) => $(`.tool[data-action=${a}]`);
  function fire(a) { const b = tool(a); if (!b) return; b.classList.remove("is-fired"); void b.offsetWidth; b.classList.add("is-fired"); }
  function syncTools() {
    tool("undo").disabled = !st.undo.length;
    tool("redo").disabled = !st.redo.length;
    tool("undo").dataset.tip = st.undo.length ? "Undo|⌘Z" : "Nothing to undo";
    tool("redo").dataset.tip = st.redo.length ? "Redo|⇧⌘Z" : "Nothing to redo";
    tool("place").classList.toggle("is-active", st.mode === "place");
    tool("bulldoze").classList.toggle("is-active", false);
    document.body.classList.toggle("is-bulldoze", st.mode === "bulldoze");
    $(".sheet").classList.toggle("is-inert", st.mode === "bulldoze");
  }
  function hover(slot) {
    if (st.mode !== "place" || !slot || st.filled.has(slot.id)) return;
    if (st.slot !== slot) { st.slot = slot; st.ri = 0; renderSheet(); hints.on("hover"); }
    W.slots.forEach((s) => s.el.classList.toggle("is-hover", s === slot));
    drawGhost();
  }
  function select(id) {
    st.selected = id;
    st.ri = 0;
    renderSheet(false);
    drawGhost();
  }
  function rotate(back) {
    const t = HW.tile(st.selected);
    if (!st.slot) return;
    const f = HW.fit(t, needs(st.slot));
    const n = f.ok && !st.manual ? f.rots.length : 6;
    st.ri = (st.ri + (back ? n - 1 : 1)) % n;
    fire("rotate");
    applyGlyphRotation(true);
    drawGhost();
    hints.on("rotate");
  }
  const coord = (slot) => slot.coord || "(0, 0)";
  function save() {
    const s = $("[data-save]");
    const n = $("[data-tiles]");
    if (n) {
      const placed = gPlaced.querySelectorAll(".placed:not(.is-leaving)").length;
      const removed = (W.tiles || []).filter((t) => t.gone.classList.contains("is-shown")).length;
      const seedsUndone = st.redo.filter((x) => x.kind === "seed").length;
      const count = +n.dataset.count + placed - removed - seedsUndone;
      n.textContent = `${count} tile${count === 1 ? "" : "s"}`;
    }
    if (!s || s.dataset.state === "off") return;
    s.dataset.state = "saving";
    HW.after(900, () => (s.dataset.state = "saved"));
  }
  function place() {
    if (st.mode !== "place" || !st.slot) return;
    const t = HW.tile(st.selected);
    const f = HW.fit(t, needs(st.slot));
    const k = currentK(t, f);
    const ok = !HW.edgeState(t, k, needs(st.slot)).includes("bad");
    if (!ok) {
      gGhost.classList.remove("is-pulse"); void gGhost.getBBox(); gGhost.classList.add("is-pulse");
      announce(`${t.name} doesn't fit here. ${f.why || "An edge clashes."}`);
      return;
    }
    st.rising.forEach((el) => el.classList.remove("is-rising"));
    const slot = st.slot;
    gGhost.classList.add("is-commit");
    const el = drawPlaced(slot, t, k);
    st.filled.add(slot.id);
    st.placedBy[slot.id] = { tile: t.id, k };
    slot.el.style.display = "none";
    st.undo.push({ kind: "place", slot: slot.id, tile: t.id, k, el, text: `placed ${t.name} at ${coord(slot)}` });
    st.redo = [];
    fire("place");
    tag.innerHTML = `${HW.icon("check")}<span>Placed ${t.name}</span>`;
    tag.classList.remove("ghost-tag--bad");
    HW.after(80, () => { gGhost.innerHTML = ""; });
    setTimeout(() => { if (!st.slot || st.filled.has(st.slot.id)) tag.classList.add("is-gone"); }, 1100);
    st.slot = null;
    renderSheet();
    save();
    syncTools();
    announce(`Placed ${t.name}.`);
    hints.on("place");
  }
  const BASE = { G: "rgb(84 132 56)", F: "rgb(52 104 58)", O: "rgb(38 112 130)" };
  function drawPlaced(slot, t, k) {
    const real = (W.real || []).find((c) => c.slot === slot.id && c.tile === t.id && c.k === k);
    if (real) {
      const g = capture(gPlaced, "placed is-rising", real.src, slot.hull);
      g.style.setProperty("--rise", `${W.rise || 1800}ms`);
      standin();
      st.rising.push(g);
      if (!HW.instant()) setTimeout(() => g.classList.remove("is-rising"), (W.rise || 1800) + 50);
      return g;
    }
    // No native capture of this tile at this slot and rotation: a flat stand-in, captioned as one.
    const g = document.createElementNS(NS, "g");
    g.setAttribute("class", "placed is-rising");
    g.dataset.standin = "";
    g.style.setProperty("--rise", `${W.rise || 1800}ms`);
    const pts = slot.pts, c = centre(pts);
    const d = 22;
    const down = (p) => [p[0], p[1] + d];
    const sides = [[pts[2], pts[3]], [pts[3], pts[4]], [pts[4], pts[5]], [pts[5], pts[0]]].map(([a, b]) => `<polygon class="p-side" points="${P([a, b, down(b), down(a)])}"/>`).join("");
    const r = HW.rot(t.s, k);
    const base = t.s === "OOOOOO" ? BASE.O : t.s.includes("F") ? BASE.F : BASE.G;
    // The cap is textured with grass sampled from the same in-engine capture, so the stand-in sits
    // in the capture's material; lanes take the capture's water, bank and asphalt colours.
    const pid = `cap-${slot.id}-${Date.now()}`;
    const ga = W.grassAt || c;
    const tex = base === BASE.G ? `<defs><pattern id="${pid}" patternUnits="userSpaceOnUse" x="${c[0] - ga[0]}" y="${c[1] - ga[1]}" width="1600" height="1000"><image href="${W.scene.src}" width="1600" height="1000"/></pattern></defs>` : "";
    let lanes = "";
    const lane = (d, kind) => kind === "V"
      ? `<path class="p-lane" stroke="rgb(124 102 80)" stroke-width="34" d="${d}"/><path class="p-lane" stroke="rgb(76 108 112)" stroke-width="24" d="${d}"/>`
      : `<path class="p-lane" stroke="rgb(46 46 48)" stroke-width="18" d="${d}"/>`;
    for (const kind of ["V", "R"]) {
      const es = [0, 1, 2, 3, 4, 5].filter((e) => r[e] === kind);
      if (es.length === 2) lanes += lane(`M${mid(side(pts, es[0]))} Q${c} ${mid(side(pts, es[1]))}`, kind);
      else es.forEach((e) => (lanes += lane(`M${mid(side(pts, e))} L${c}`, kind)));
    }
    g.innerHTML = `${tex}${sides}<polygon class="p-top" points="${P(pts)}" fill="${tex ? `url(#${pid})` : base}"/>${lanes}`;
    gPlaced.appendChild(g);
    standin();
    st.rising.push(g);
    if (!HW.instant()) setTimeout(() => g.classList.remove("is-rising"), (W.rise || 1800) + 50);
    return g;
  }
  function undoRedo(isRedo) {
    const from = isRedo ? st.redo : st.undo, to = isRedo ? st.undo : st.redo;
    const a = from.pop();
    if (!a) return;
    to.push(a);
    fire(isRedo ? "redo" : "undo");
    if (a.kind === "place") {
      const slot = slotById(a.slot);
      if (!isRedo) {
        a.el.classList.remove("is-rising"); a.el.classList.add("is-leaving");
        HW.after(260, () => { a.el.remove(); standin(); });
        st.filled.delete(a.slot); delete st.placedBy[a.slot]; slot.el.style.display = "";
      } else {
        a.el = drawPlaced(slot, HW.tile(a.tile), a.k);
        st.filled.add(a.slot); st.placedBy[a.slot] = { tile: a.tile, k: a.k }; slot.el.style.display = "none";
      }
    }
    if (a.kind === "remove") {
      const t = W.tiles.find((x) => x.id === a.tile);
      t.gone.classList.toggle("is-shown", isRedo);
    }
    const verb = isRedo ? "Redid" : "Undid";
    const same = st.toast && st.toast.verb === verb && $("[data-toast]").classList.contains("is-shown");
    st.toast = { verb, count: same ? st.toast.count + 1 : 1 };
    const text = st.toast.count > 1 ? `${verb} ${st.toast.count} actions` : `${verb}: ${a.text}`;
    HW.toast({ text, count: st.toast.count, action: isRedo ? "Undo" : "Redo", onAction: () => undoRedo(!isRedo), icon: isRedo ? "redo" : "undo" });
    save();
    syncTools();
    drawGhost();
    hints.on("undo");
  }
  function setMode(m) {
    st.mode = st.mode === m ? "place" : m;
    fire(m);
    W.slots.forEach((s) => s.el.classList.remove("is-hover"));
    (W.tiles || []).forEach(clearTile);
    tag.classList.add("is-gone");
    syncTools();
    renderKey();
    drawGhost();
    if (st.mode === "bulldoze") hints.on("bulldoze");
  }
  function clearTile(t) { t.out.setAttribute("class", "t-out"); t.cutEls.forEach((c) => c.classList.remove("is-shown")); }
  function hoverTile(t) {
    (W.tiles || []).forEach(clearTile);
    if (!t || t.gone.classList.contains("is-shown")) { tag.classList.add("is-gone"); return; }
    const icon = t.block ? HW.icon("alert") : HW.icon("bulldoze");
    if (t.block) {
      t.out.setAttribute("class", "t-out is-blocked");
      t.cutEls.forEach((c) => c.classList.add("is-shown"));
      tag.innerHTML = `${icon}<span class="tag-stack"><span>${t.block}</span>${t.blockSub ? `<small>${t.blockSub}</small>` : ""}</span>`;
      tag.classList.add("ghost-tag--bad");
    } else {
      t.out.setAttribute("class", "t-out is-remove");
      tag.innerHTML = `${icon}<span>Remove ${t.name}</span><span class="ghost-tag__fit" style="color:var(--ink-2)">Click</span>`;
      tag.classList.remove("ghost-tag--bad");
    }
    tag.classList.remove("is-gone");
    placeTag(t.pts);
  }
  function removeTile(t) {
    if (t.block) { t.out.classList.remove("is-blocked"); void t.out.getBBox(); t.out.classList.add("is-blocked"); fire("bulldoze"); announce(t.block); return; }
    t.gone.classList.add("is-shown");
    clearTile(t);
    tag.classList.add("is-gone");
    st.undo.push({ kind: "remove", tile: t.id, text: `removed ${t.name}` });
    st.redo = [];
    HW.toast({ text: `Removed ${t.name}`, action: "Undo", icon: "bulldoze", onAction: () => undoRedo(false) });
    st.toast = null;
    save();
    syncTools();
  }
  function toggleSheet(force) {
    const sh = $(".sheet--tiles");
    const closed = force != null ? !force : !sh.classList.contains("is-closed");
    sh.classList.toggle("is-closed", closed);
    sh.toggleAttribute("inert", closed);
    fire("inventory");
    if (st.slot) drawGhost();
  }
  const cap = document.createElement("p");
  cap.className = "standin";
  cap.textContent = "No capture of this tile here: a flat stand-in. The engine draws the real tile.";
  document.body.appendChild(cap);
  function standin() { cap.classList.toggle("is-shown", !!gPlaced.querySelector(".placed[data-standin]:not(.is-leaving)")); }
  const live = $("[data-live]");
  function announce(msg) { if (live) live.textContent = msg; }

  // ---------- First-run hints: one callout at a time, each leaves on its action ----------
  const hints = (() => {
    if (!W.hints) return { on() {} };
    const seq = W.hints;
    let i = 0, timer, started = false;
    const box = document.createElement("div");
    box.className = "callout";
    box.setAttribute("role", "status");
    document.body.appendChild(box);
    const leader = document.createElementNS(NS, "svg");
    leader.setAttribute("class", "leader");
    leader.innerHTML = `<path/><circle r="4"/>`;
    document.body.appendChild(leader);
    function show() {
      const h = seq[i];
      if (!h || (h.when && !h.when(st))) return;
      box.innerHTML = h.html;
      const a = h.anchor();
      if (!a) return;
      const at = h.at(a);
      box.style.left = `${at[0]}px`; box.style.top = `${at[1]}px`;
      const b = box.getBoundingClientRect();
      const sx = b.right < a[0] ? b.right : b.left > a[0] ? b.left : b.left + b.width / 2;
      const sy = b.bottom < a[1] ? b.bottom : b.top > a[1] ? b.top : b.top + b.height / 2;
      leader.querySelector("path").setAttribute("d", `M${sx},${sy} C${(sx + a[0]) / 2},${sy} ${a[0]},${(sy + a[1]) / 2} ${a[0]},${a[1]}`);
      leader.querySelector("circle").setAttribute("cx", a[0]); leader.querySelector("circle").setAttribute("cy", a[1]);
      box.classList.add("is-shown");
      leader.querySelectorAll("*").forEach((n) => n.classList.add("is-shown"));
    }
    function hide() { box.classList.remove("is-shown"); leader.querySelectorAll("*").forEach((n) => n.classList.remove("is-shown")); }
    function arm(ms) { clearTimeout(timer); timer = setTimeout(show, HW.instant() ? 0 : ms); }
    return {
      start(n = 0) { i = n; started = true; arm(1500); },
      on(ev) {
        const h = seq[i];
        if (!h) return;
        if (h.done.includes(ev)) { hide(); i++; }
        if (started && !box.classList.contains("is-shown")) arm(1500);
      },
    };
  })();

  // ---------- Input ----------
  svg.addEventListener("pointerover", (e) => {
    const s = e.target.closest?.("[data-slot]");
    if (s) hover(slotById(s.dataset.slot));
    const t = e.target.closest?.("[data-tile]");
    if (t && st.mode === "bulldoze") hoverTile(W.tiles.find((x) => x.id === t.dataset.tile));
  });
  svg.addEventListener("pointerout", (e) => {
    if (e.target.closest?.("[data-tile]") && st.mode === "bulldoze") hoverTile(null);
  });
  svg.addEventListener("click", (e) => {
    const s = e.target.closest?.("[data-slot]");
    if (s && st.mode === "place") { hover(slotById(s.dataset.slot)); place(); }
    const t = e.target.closest?.("[data-tile]");
    if (t && st.mode === "bulldoze") removeTile(W.tiles.find((x) => x.id === t.dataset.tile));
  });
  svg.addEventListener("wheel", (e) => { if (e.ctrlKey && st.slot) { e.preventDefault(); rotate(e.deltaY < 0); } }, { passive: false });
  document.addEventListener("click", (e) => {
    const row = e.target.closest(".row[data-tile]");
    if (row) { select(row.dataset.tile); return; }
    if (e.target.closest(".sheet--tiles .group--off")) { lists.off.scrollIntoView({ block: "nearest", behavior: HW.instant() ? "auto" : "smooth" }); return; }
    const go = e.target.closest("[data-goto]");
    if (go) { hover(slotById(go.dataset.goto)); return; }
    const a = e.target.closest("[data-action]");
    if (!a || a.disabled) return;
    const n = a.dataset.action;
    if (n === "rotate") rotate(e.shiftKey);
    if (n === "undo") undoRedo(false);
    if (n === "redo") undoRedo(true);
    if (n === "bulldoze") setMode("bulldoze");
    if (n === "place") { if (st.mode !== "place") setMode(st.mode); else fire("place"); }
    if (n === "inventory") toggleSheet();
    if (n === "show-ui") document.body.classList.remove("hud-hidden");
  });
  document.addEventListener("keydown", (e) => {
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.key.toLowerCase() === "z") { e.preventDefault(); undoRedo(e.shiftKey); return; }
    if (e.key === "Tab" && !e.altKey && !mod) { e.preventDefault(); toggleSheet(); return; }
    if (e.target.matches("input")) return;
    const k = e.key.toLowerCase();
    if (k === "r" && !mod) rotate(e.shiftKey);
    if (k === "b" && !mod) setMode("bulldoze");
    if (k === "h" && !mod) document.body.classList.toggle("hud-hidden");
    if (e.key === "Escape") { if (st.mode !== "place") setMode(st.mode); else if (document.body.classList.contains("hud-hidden")) document.body.classList.remove("hud-hidden"); }
    if (k === "p" && !mod) location.href = "photo.html";
    const row = e.target.closest?.(".row[data-tile]");
    if (row && (e.key === "ArrowDown" || e.key === "ArrowUp")) {
      e.preventDefault();
      const all = $$(".sheet--tiles .row");
      const i = all.indexOf(row);
      all[Math.max(0, Math.min(all.length - 1, i + (e.key === "ArrowDown" ? 1 : -1)))].focus();
    }
    if (row && (e.key === "Enter" || e.key === " ")) { e.preventDefault(); select(row.dataset.tile); }
    if (/^[1-9]$/.test(e.key) && !mod) { const li = $$("[data-list=fit] .row")[+e.key - 1]; if (li) { select(li.dataset.tile); li.focus(); } }
    if (e.key === "Enter" && !row && st.slot) place();
  });
  addEventListener("resize", () => { if (st.slot) drawGhost(); });

  // ---------- Initial render and review states (play.html#state) ----------
  syncTools();
  renderSheet(false);
  HW.playApi = { st, hover, select, rotate, place, undoRedo, setMode, hoverTile, toggleSheet, slotById, hints, drawGhost, renderSheet, syncTools };
  const apply = window.WORLD_STATES && window.WORLD_STATES[HW.state() || "default"];
  if (apply) apply(HW.playApi);
  else { if (W.hints) hints.start(); drawGhost(); }
})();
