// Hex World, Legend full pass: shared prototype plumbing (design mockups only, not game code).
// Classic script on purpose: every page must open by double-click from file://.
(() => {
  const HW = (window.HW = {});

  // ---------- Shared icon set: 24 px grid, 1.8 px stroke, round caps ----------
  const ICONS = {
    place: '<path d="M12 2.8 20 7.4v9.2L12 21.2 4 16.6V7.4Z"/><path d="M12 8.6v6.8M8.6 12h6.8"/>',
    rotate: '<path d="M19.5 12a7.5 7.5 0 1 1-2.2-5.3"/><path d="M19.6 3.8v4.6H15"/>',
    undo: '<path d="M9 14 4 9l5-5"/><path d="M4 9h10.5a5.5 5.5 0 0 1 0 11H11"/>',
    redo: '<path d="m15 14 5-5-5-5"/><path d="M20 9H9.5a5.5 5.5 0 0 0 0 11H13"/>',
    bulldoze: '<path d="M12 2.8 20 7.4v9.2L12 21.2 4 16.6V7.4Z"/><path d="m9 9 6 6M15 9l-6 6"/>',
    photo: '<path d="M3.5 8h3.2l1.9-2.8h6.8L17.3 8h3.2v11h-17Z"/><circle cx="12" cy="13.2" r="3.4"/>',
    settings: '<path d="M4 7h8.5M17.5 7H20M4 17h2.5M11.5 17H20"/><circle cx="15" cy="7" r="2.4"/><circle cx="9" cy="17" r="2.4"/>',
    close: '<path d="m15 6-6 6 6 6"/>',
    inventory: '<path d="M7.5 3.5 11 5.5v4L7.5 11.5 4 9.5v-4ZM16.5 3.5 20 5.5v4l-3.5 2L13 9.5v-4ZM12 12.5l3.5 2v4L12 20.5l-3.5-2v-4Z"/>',
    worlds: '<path d="M4 6.5 12 3l8 3.5-8 3.5Z"/><path d="m4 11.5 8 3.5 8-3.5M4 16.5 12 20l8-3.5"/>',
    export: '<path d="M12 15V4M7.5 8.5 12 4l4.5 4.5"/><path d="M4.5 14v5.5h15V14"/>',
    import: '<path d="M12 4v11M7.5 10.5 12 15l4.5-4.5"/><path d="M4.5 14v5.5h15V14"/>',
    eyeoff: '<path d="M3.5 12s3.2-6 8.5-6c1.5 0 2.8.5 3.9 1.1M20.5 12s-3.2 6-8.5 6c-1.5 0-2.8-.5-3.9-1.1"/><path d="M9.9 14.1a3 3 0 0 1 4.2-4.2M4 20 20 4"/>',
    eye: '<path d="M3.5 12S6.7 6 12 6s8.5 6 8.5 6-3.2 6-8.5 6-8.5-6-8.5-6Z"/><circle cx="12" cy="12" r="3"/>',
    pause: '<path d="M9 5.5v13M15 5.5v13"/>',
    trash: '<path d="M4.5 7h15M9.5 7V4.5h5V7M6.5 7l1 12.5h9l1-12.5"/>',
    copy: '<path d="M8.5 8.5h11v11h-11Z"/><path d="M15.5 8.5V4.5h-11v11h4"/>',
    pencil: '<path d="m14.5 5.5 4 4L9 19H5v-4Z"/><path d="m12.5 7.5 4 4"/>',
    plus: '<path d="M12 5v14M5 12h14"/>',
    alert: '<path d="M12 4 21 19.5H3Z"/><path d="M12 10v4.2M12 17v.01"/>',
    info: '<circle cx="12" cy="12" r="8.5"/><path d="M12 11v5.5M12 7.8v.01"/>',
    check: '<path d="m5 12.5 4.5 4.5L19 7.5"/>',
    x: '<path d="m6.5 6.5 11 11M17.5 6.5l-11 11"/>',
    sun: '<circle cx="12" cy="12" r="3.8"/><path d="M12 3v2M12 19v2M3 12h2M19 12h2M5.6 5.6 7 7M17 17l1.4 1.4M5.6 18.4 7 17M17 7l1.4-1.4"/>',
    volume: '<path d="M4 9.5h3.5L12 5.5v13l-4.5-4H4Z"/><path d="M15.5 9a4.5 4.5 0 0 1 0 6M18 6.5a8 8 0 0 1 0 11"/>',
    keyboard: '<path d="M3 6.5h18v11H3Z"/><path d="M6.5 10h.01M10 10h.01M14 10h.01M17.5 10h.01M8 14h8"/>',
    more: '<path d="M6 12h.01M12 12h.01M18 12h.01"/>',
    down: '<path d="m6.5 9.5 5.5 5.5 5.5-5.5"/>',
    retry: '<path d="M4.5 12a7.5 7.5 0 0 0 13.3 4.7M19.5 12A7.5 7.5 0 0 0 6.2 7.3"/><path d="M6 3.5v4h4M18 20.5v-4h-4"/>',
    file: '<path d="M6 3.5h8l4 4v13H6Z"/><path d="M14 3.5v4h4"/>',
    gpu: '<path d="M4 7h16v10H4Z"/><path d="M8 7V4.5M12 7V4.5M16 7V4.5M8 19.5V17M12 19.5V17M16 19.5V17"/><path d="M9 10h6v4H9Z"/>',
    exit: '<path d="M14.5 4.5h5v15h-5"/><path d="M10 8 6 12l4 4M6 12h9.5"/>',
    focus: '<path d="M4 8.5V4h4.5M15.5 4H20v4.5M20 15.5V20h-4.5M8.5 20H4v-4.5"/><circle cx="12" cy="12" r="2.5"/>',
    browser: '<path d="M3.5 5h17v14h-17Z"/><path d="M3.5 9h17M6.5 7h.01M9 7h.01"/>',
    hand: '<path d="M8.5 12V5.5a1.5 1.5 0 0 1 3 0V11M11.5 10.5V4a1.5 1.5 0 0 1 3 0v6.5M14.5 10.5V6a1.5 1.5 0 0 1 3 0v7.5a6 6 0 0 1-6 6h-.6a6 6 0 0 1-4.6-2.2L4 14.6a1.5 1.5 0 0 1 2.3-1.9L8.5 15"/>',
  };
  const sprite = `<svg width="0" height="0" style="position:absolute" aria-hidden="true"><defs>${Object.entries(ICONS)
    .map(([id, d]) => `<symbol id="i-${id}" viewBox="0 0 24 24">${d}</symbol>`)
    .join("")}<pattern id="hatch" width="10" height="10" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><rect width="10" height="10" fill="rgb(18 39 58 / .35)"/><line x1="0" y1="0" x2="0" y2="10" stroke="rgb(238 243 246 / .75)" stroke-width="2.5"/></pattern></defs></svg>`;
  document.body.insertAdjacentHTML("afterbegin", sprite);
  HW.icon = (id, cls = "ic") => `<svg class="${cls}" aria-hidden="true"><use href="#i-${id}"/></svg>`;
  document.querySelectorAll("[data-icon]").forEach((el) => el.insertAdjacentHTML("afterbegin", HW.icon(el.dataset.icon)));

  // ---------- The tool bar, shared by every screen that shows the play HUD ----------
  HW.bar = ({ world = "Riverside", tiles = 14, save = "saved", active = "place" } = {}) => {
    const t = (a, icon, label, key, tip, aria) =>
      `<button class="tool${active === a ? " is-active" : ""}" type="button" data-action="${a}"${tip ? ` data-tip="${tip}"` : ""}${aria ? ` aria-label="${aria}"` : ""}>${HW.icon(icon)}${label ? `<span>${label}</span>` : ""}${key ? `<kbd>${key}</kbd>` : ""}</button>`;
    return `<nav class="bar" data-region aria-label="Tools">
      <a class="bar__world" href="saves.html" data-tip="Worlds|" aria-label="Worlds. ${world}, ${tiles} tiles, ${save === "off" ? "not saved" : "saved"}"><b>${world}</b><span><span data-tiles data-count="${tiles}">${tiles} tile${tiles === 1 ? "" : "s"}</span> · <span class="save-state" data-save data-state="${save}"><span data-s="saved"><i class="save-dot"></i>Saved</span><span data-s="saving"><i class="save-dot"></i>Saving…</span><span data-s="off">${HW.icon("alert", "ic ic--xs")}Not saved</span></span></span></a>
      <span class="bar__rule" aria-hidden="true"></span>
      ${t("place", "place", "Place", "", "Place|Click")}
      ${t("rotate", "rotate", "Rotate", "R", "Rotate|R|Shift+R turns back")}
      ${t("undo", "undo", "Undo", "⌘Z", "Undo|⌘Z")}
      ${t("redo", "redo", "", "", "Redo|⇧⌘Z", "Redo, Shift Command Z")}
      ${t("bulldoze", "bulldoze", "Bulldoze", "B", "Bulldoze|B")}
      <span class="bar__rule" aria-hidden="true"></span>
      ${t("inventory", "inventory", "Tiles", "Tab", "Tiles|Tab", "Tiles, Tab").replace('class="tool', 'class="tool tool--inv')}
      <span class="bar__rule" aria-hidden="true"></span>
      ${t("photo", "photo", "", "", "Photo mode|P", "Photo mode, P").replace("<button", "<button data-href=\"photo.html\"")}
      ${t("settings", "settings", "", "", "Settings", "Settings").replace("<button", "<button data-href=\"settings.html\"")}
    </nav>`;
  };

  // ---------- Review plumbing: ?still pins motion for captures; ` shows the state strip ----------
  const params = new URLSearchParams(location.search);
  HW.still = params.has("still");
  if (HW.still) document.documentElement.classList.add("is-still");
  HW.reduce = matchMedia("(prefers-reduced-motion: reduce)");
  HW.instant = () => HW.still || HW.reduce.matches;
  HW.state = () => decodeURIComponent(location.hash.slice(1));
  document.addEventListener("keydown", (e) => {
    if (e.key === "`") document.documentElement.classList.toggle("show-review");
  });
  HW.after = (ms, fn) => (HW.instant() ? fn() : setTimeout(fn, ms));

  // ---------- Tiles, sockets and the legality rule (data/sockets.ron, data/tiles/*.ron) ----------
  HW.KIND = { G: "Grass", F: "Forest", R: "Road", V: "River", B: "Beach", O: "Ocean" };
  HW.COMPAT = { G: "GFB", F: "GF", R: "R", V: "V", B: "BGO", O: "OB" };
  HW.TILES = [
    { id: "meadow", name: "Meadow", s: "GGGGGG" },
    { id: "forest", name: "Forest", s: "FFFFFF" },
    { id: "house_cluster", name: "House cluster", s: "RGGGGG" },
    { id: "road_straight", name: "Road straight", s: "RGGRGG" },
    { id: "road_bend_60", name: "Road bend 60°", s: "RRGGGG" },
    { id: "road_bend_120", name: "Road bend 120°", s: "RGRGGG" },
    { id: "road_junction", name: "Road junction", s: "RGRGRG" },
    { id: "river_straight", name: "River straight", s: "VGGVGG" },
    { id: "river_bend", name: "River bend", s: "VGVGGG" },
    { id: "river_road_bridge", name: "River bridge", s: "VRGVRG" },
    { id: "beach", name: "Beach", s: "OOBGGB" },
    { id: "ocean", name: "Ocean", s: "OOOOOO" },
  ];
  HW.tile = (id) => HW.TILES.find((t) => t.id === id);
  // Rotation k maps tile edge i to (i + k) mod 6 (PRD §5.1), so edge e now carries s[e - k].
  HW.rot = (s, k) => [0, 1, 2, 3, 4, 5].map((e) => s[(e - k + 6) % 6]).join("");
  const lower = (k) => HW.KIND[k].toLowerCase();
  const MISSING = {
    V: "Needs a river edge to meet the river.",
    R: "Needs a road edge to meet the road.",
    O: "Needs ocean or beach to meet the sea.",
    B: "Needs grass, beach or ocean to meet the beach.",
    G: "Needs grass, forest or beach to meet the grass.",
    F: "Needs grass or forest to meet the forest.",
  };
  function clashPhrase(t, n) {
    if (t === "R") return `Its road would dead-end into the ${lower(n)}.`;
    if (t === "V") return `Its river would run into the ${lower(n)}.`;
    if (n === "R") return `The road here would dead-end into its ${lower(t)}.`;
    if (n === "V") return `The river here would run into its ${lower(t)}.`;
    if (t === "O") return `Ocean can't meet ${lower(n)}. Coasts need a beach.`;
    if (n === "O") return `${HW.KIND[t]} can't meet the ocean. Coasts need a beach.`;
    return `${HW.KIND[t]} can't meet ${lower(n)}.`;
  }
  // needs: { edgeIndex: kind } for the placed neighbours of a slot.
  HW.fit = (tile, needs) => {
    const edges = Object.keys(needs).map(Number);
    const rots = [];
    const seen = new Set();
    let best = null;
    for (let k = 0; k < 6; k++) {
      const r = HW.rot(tile.s, k);
      const clashes = edges.filter((e) => !HW.COMPAT[needs[e]].includes(r[e]));
      if (!best || clashes.length < best.clashes.length) best = { k, clashes };
      if (!clashes.length && !seen.has(r)) { seen.add(r); rots.push(k); }
    }
    if (rots.length) return { ok: true, rots, meta: rots.length === 1 ? (new Set([...tile.s]).size === 1 ? "Fits any way round" : "1 rotation fits") : `${rots.length} rotations fit` };
    const kinds = [...new Set(edges.map((e) => needs[e]))];
    const lacking = kinds.find((n) => ![...tile.s].some((c) => HW.COMPAT[n].includes(c)));
    const e = best.clashes[0];
    const why = lacking ? MISSING[lacking] : clashPhrase(HW.rot(tile.s, best.k)[e], needs[e]);
    return { ok: false, rots: [], why, bestK: best.k };
  };
  HW.edgeState = (tile, k, needs) => {
    const r = HW.rot(tile.s, k);
    return [0, 1, 2, 3, 4, 5].map((e) => (needs[e] == null ? "free" : HW.COMPAT[needs[e]].includes(r[e]) ? "ok" : "bad"));
  };
  // What the slot key prints: lanes first ("River on one edge"), then what may sit beside them.
  HW.needLines = (needs) => {
    const kinds = Object.values(needs);
    const order = "VROBFG";
    const uniq = [...new Set(kinds)].sort((a, b) => order.indexOf(a) - order.indexOf(b));
    const hasLane = uniq.some((n) => n === "R" || n === "V");
    return uniq.map((n) => {
      const count = kinds.filter((x) => x === n).length;
      const accepts = HW.COMPAT[n].split("").map(lower);
      const list = accepts.length === 1 ? accepts[0] : `${accepts.slice(0, -1).join(", ")} or ${accepts.at(-1)}`;
      const where = count === 1 ? "on one edge" : `on ${count} edges`;
      const text = n === "R" || n === "V" ? `${HW.KIND[n]} ${where}` : `${list[0].toUpperCase()}${list.slice(1)} ${hasLane ? "beside it" : where}`;
      return { kind: n, text };
    });
  };
  HW.summary = (s) => {
    const order = "VROBFG";
    const ks = [...new Set(s)].sort((a, b) => order.indexOf(a) - order.indexOf(b)).map(lower);
    if (ks.length === 1) return `${ks[0][0].toUpperCase()}${ks[0].slice(1)} all round`;
    const txt = ks.length === 2 ? `${ks[0]} and ${ks[1]}` : `${ks.slice(0, -1).join(", ")} and ${ks.at(-1)}`;
    return `${txt[0].toUpperCase()}${txt.slice(1)} edges`;
  };

  // ---------- Glyphs: stand-ins drawn from the socket string until thumbnails exist (F4) ----------
  const GR = 40, GC = 50;
  const gv = (i) => { const a = ((-30 + 60 * i) * Math.PI) / 180; return [GC + GR * Math.cos(a), GC + GR * Math.sin(a)]; };
  HW.hexPts = (r = GR, c = GC) => [0, 1, 2, 3, 4, 5].map((i) => { const a = ((-30 + 60 * i) * Math.PI) / 180; return `${(c + r * Math.cos(a)).toFixed(1)},${(c + r * Math.sin(a)).toFixed(1)}`; }).join(" ");
  const fillKind = (s) => (s === "OOOOOO" ? "O" : s.includes("F") ? "F" : "G");
  HW.glyph = (t) => {
    let edges = "", lanes = "", extra = "";
    for (let e = 0; e < 6; e++) {
      const [x1, y1] = gv(e), [x2, y2] = gv(e + 1);
      edges += `<line class="g-edge" x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" style="stroke:var(--k-${t.s[e]})"/>`;
      if (t.s[e] === "R" || t.s[e] === "V") lanes += `<line class="g-lane g-lane-${t.s[e]}" x1="${(x1 + x2) / 2}" y1="${(y1 + y2) / 2}" x2="${GC}" y2="${GC}"/>`;
    }
    if (t.id === "beach") extra = `<path class="g-sand" d="M${gv(0)} L${gv(1)} L${gv(2)} Q${GC},${GC - 6} ${gv(5)} Z"/>`;
    if (t.id === "forest") extra = [[38, 40], [60, 36], [50, 56], [34, 62], [64, 60]].map(([x, y]) => `<circle class="g-tree" cx="${x}" cy="${y}" r="7"/>`).join("");
    if (t.id === "house_cluster") extra = [[44, 44], [58, 52], [42, 60]].map(([x, y]) => `<rect class="g-house" x="${x - 5}" y="${y - 5}" width="10" height="10" rx="1.5"/>`).join("");
    if (t.s.includes("R") && t.s.includes("V")) extra = `<rect class="g-bridge" x="${GC - 7}" y="${GC - 7}" width="14" height="14" rx="2"/>`;
    return `<svg class="glyph" viewBox="0 0 100 100" aria-hidden="true"><g class="g-rot"><polygon class="g-fill" points="${HW.hexPts()}" style="fill:var(--k-${fillKind(t.s)})"/>${extra}${lanes}${edges}</g></svg>`;
  };
  HW.code = (s) => [...s].map((k) => `<i style="background:var(--k-${k})" title="${HW.KIND[k]}"></i>`).join("");

  // The slot key: a hex that draws exactly which edges are touched, and by what.
  HW.keyHex = (needs, opts = {}) => {
    let out = `<polygon class="key__slot" points="${HW.hexPts(34)}"/>`;
    const v = (i) => { const a = ((-30 + 60 * i) * Math.PI) / 180; return [GC + 34 * Math.cos(a), GC + 34 * Math.sin(a)]; };
    for (const [e, k] of Object.entries(needs)) {
      const [x1, y1] = v(+e), [x2, y2] = v(+e + 1);
      out += `<line class="key__edge" x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" style="stroke:var(--k-${k})"/>`;
    }
    if (opts.none) out += `<path class="key__x" d="M42 42l16 16M58 42 42 58"/>`;
    return `<svg class="key__hex" viewBox="0 0 100 100" aria-hidden="true">${out}</svg>`;
  };

  // ---------- Toast: one at a time, updates in place, pauses while hovered ----------
  HW.toast = (opts) => {
    const el = document.querySelector("[data-toast]");
    if (!el) return;
    const { text, sub, action, onAction, tone, icon, count, sticky } = opts;
    el.className = `toast ${tone ? "toast--" + tone : ""}`;
    el.innerHTML = `${icon ? HW.icon(icon) : ""}<p>${count > 1 ? `<span><span class="toast__count">${count}</span>${text}</span>` : `<span>${text}</span>`}${sub ? `<small>${sub}</small>` : ""}</p>${action ? `<button class="toast__action" type="button">${action}</button>` : ""}`;
    if (action && onAction) el.querySelector(".toast__action").addEventListener("click", onAction);
    void el.offsetWidth;
    el.classList.add("is-shown");
    clearTimeout(HW.toast.t);
    if (sticky || HW.still) return;
    const arm = () => (HW.toast.t = setTimeout(() => el.classList.remove("is-shown"), 4000));
    el.onmouseenter = () => clearTimeout(HW.toast.t);
    el.onmouseleave = arm;
    arm();
  };
  HW.hideToast = () => document.querySelector("[data-toast]")?.classList.remove("is-shown");

  // ---------- Tooltips: [data-tip] carries "Name|Key|Note" ----------
  const tip = document.createElement("div");
  tip.className = "tip";
  tip.setAttribute("role", "tooltip");
  document.body.appendChild(tip);
  HW.showTip = (el) => {
    const [name, key, note] = (el.dataset.tip || "").split("|");
    if (!name) return;
    tip.innerHTML = `${name}${key ? ` <kbd>${key}</kbd>` : ""}${note ? ` <small>${note}</small>` : ""}`;
    const r = el.getBoundingClientRect();
    tip.style.left = `${Math.round(r.left)}px`;
    tip.style.top = `${Math.round(r.bottom + 8)}px`;
    tip.classList.add("is-shown");
  };
  HW.hideTip = () => tip.classList.remove("is-shown");
  let tipTimer;
  const tipTarget = (e) => e.target.closest?.("[data-tip]");
  document.addEventListener("pointerover", (e) => { const t = tipTarget(e); if (!t) return; clearTimeout(tipTimer); tipTimer = setTimeout(() => HW.showTip(t), 350); });
  document.addEventListener("pointerout", (e) => { if (tipTarget(e)) { clearTimeout(tipTimer); HW.hideTip(); } });
  document.addEventListener("focusin", (e) => { const t = tipTarget(e); if (t && t.matches(":focus-visible")) HW.showTip(t); });
  document.addEventListener("focusout", HW.hideTip);

  // ---------- Range inputs paint their fill; switches and segmented groups toggle ----------
  HW.paintRange = (r) => r.style.setProperty("--pct", `${((r.value - r.min) / (r.max - r.min)) * 100}%`);
  document.querySelectorAll("input[type=range]").forEach((r) => { HW.paintRange(r); r.addEventListener("input", () => HW.paintRange(r)); });
  document.addEventListener("click", (e) => {
    const sw = e.target.closest(".switch");
    if (sw && !sw.disabled) { sw.setAttribute("aria-checked", String(sw.getAttribute("aria-checked") !== "true")); sw.dispatchEvent(new Event("change", { bubbles: true })); }
    const seg = e.target.closest(".seg button");
    if (seg && !seg.disabled) {
      seg.parentElement.querySelectorAll("button").forEach((b) => b.setAttribute("aria-checked", String(b === seg)));
      seg.parentElement.dispatchEvent(new CustomEvent("pick", { bubbles: true, detail: seg.dataset.value }));
    }
    const tab = e.target.closest(".tabs button");
    if (tab) HW.selectTab(tab);
    const nav = e.target.closest("[data-href]");
    if (nav && !nav.disabled) location.href = nav.dataset.href;
  });
  HW.selectTab = (tab) => {
    const list = tab.parentElement;
    list.querySelectorAll("button").forEach((b) => {
      b.setAttribute("aria-selected", String(b === tab));
      const p = document.getElementById(b.getAttribute("aria-controls"));
      if (p) p.hidden = b !== tab;
    });
  };

  // ---------- Keyboard: Tab belongs to the sheet, so F6 is the focus escape between regions ----------
  document.addEventListener("keydown", (e) => {
    if (e.key !== "F6") return;
    e.preventDefault();
    const regions = [...document.querySelectorAll("[data-region]")].filter((r) => r.offsetParent && !r.classList.contains("is-closed"));
    if (!regions.length) return;
    const i = regions.findIndex((r) => r.contains(document.activeElement));
    const next = regions[(i + (e.shiftKey ? -1 : 1) + regions.length) % regions.length];
    (next.querySelector("button:not(:disabled), [tabindex='0'], input") || next).focus();
  });
})();
