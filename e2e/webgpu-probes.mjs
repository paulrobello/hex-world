// Playwright init scripts that observe the app's WebGPU usage. Each export is a function passed to
// `page.addInitScript(fn, arg)`, so it runs in the page before any app script.

// Requests the device with `maxSampledTexturesPerShaderStage` set to `lim`. WebGPU raises a request
// below the default (16) to 16, so only values >= 16 clamp anything; read the effective value from
// `window.__deviceSampledLimit`.
export function clampSampledTextures(lim) {
  const rd = GPUAdapter.prototype.requestDevice;
  GPUAdapter.prototype.requestDevice = function (d = {}) {
    d = { ...d, requiredLimits: { ...(d.requiredLimits ?? {}), maxSampledTexturesPerShaderStage: lim } };
    return rd.call(this, d).then((dev) => {
      window.__deviceSampledLimit = dev.limits.maxSampledTexturesPerShaderStage;
      return dev;
    });
  };
}

// Counts sampled textures visible to the fragment stage in every pipeline layout the app creates
// (the number WebGPU checks against maxSampledTexturesPerShaderStage). `window.__sampledTextures`
// holds `{ max, layouts, kit }`: `layouts` lists each distinct per-layout count, and `kit` lists
// `{ pass, sampled, ibl, transmission, groups }` for each render pipeline whose fragment module is a kit shader (it declares
// the `tile_inv_m` uniform field): `pass` is "forward" when the module also declares
// `masks_texture` (kit_material.wesl), else "prepass" (kit_prepass.wesl); `groups` lists, per bind
// group, the binding numbers of those fragment-visible textures (the breakdown behind `sampled`), and
// `transmission` is true when the layout binds the view transmission texture (group 0, binding 24). `tilt` lists `{ sampled }`
// for each render pipeline whose fragment module is the tilt-shift shader (it declares `focus_uv_y`). `water` lists
// `{ pass, sampled, ibl, transmission }` for the water scenario's pipelines, `pass` being "water"
// (water.wesl) or "terrain" (water_terrain.wesl). `boid` lists `{ sampled, ibl, transmission }` for each
// render pipeline whose fragment module is the boid material (it declares the `boid_verts` field,
// assets/shaders/boid_material.wesl).
export function countSampledTextures() {
  const FRAGMENT = 2;
  const perBgl = new WeakMap();
  const perBglBindings = new WeakMap();
  const perLayout = new WeakMap();
  const perLayoutGroups = new WeakMap();
  const kitModules = new WeakMap();
  const tiltModules = new WeakSet();
  const waterModules = new WeakMap();
  const boidModules = new WeakMap();
  const stats = { max: 0, layouts: [], kit: [], tilt: [], water: [], boid: [] };
  window.__sampledTextures = stats;
  const csm = GPUDevice.prototype.createShaderModule;
  GPUDevice.prototype.createShaderModule = function (desc) {
    const mod = csm.apply(this, arguments);
    const code = typeof desc?.code === "string" ? desc.code : "";
    // `ibl`: the module was composed with ENVIRONMENT_MAP (Bevy declares the env-map bindings only then),
    // so a pipeline specialized before the IBL cubemaps loaded is distinguishable from the final one.
    if (/\btile_inv_m\b/.test(code))
      kitModules.set(mod, { pass: /\bmasks_texture\b/.test(code) ? "forward" : "prepass", ibl: /diffuse_environment_map/.test(code) });
    // Water scenario (crates/hex-render/src/water.rs): the water surface declares `wave_count`, the
    // terrain `terrain_time`; `transmission` marks a module composed with the view transmission texture
    // (no \b: the composer prefixes imported names with their module path).
    else if (/\bwave_count\b|\bterrain_time\b/.test(code))
      waterModules.set(mod, { pass: /\bwave_count\b/.test(code) ? "water" : "terrain", ibl: /diffuse_environment_map/.test(code),
        transmission: /view_transmission_texture/.test(code) });
    if (/\bfocus_uv_y\b/.test(code)) tiltModules.add(mod);
    if (/\bboid_verts\b/.test(code))
      boidModules.set(mod, { ibl: /diffuse_environment_map/.test(code), transmission: /view_transmission_texture/.test(code) });
    return mod;
  };
  const cbgl = GPUDevice.prototype.createBindGroupLayout;
  GPUDevice.prototype.createBindGroupLayout = function (desc) {
    const bgl = cbgl.apply(this, arguments);
    const tex = (desc?.entries ?? []).filter((e) => (e.texture || e.externalTexture) && (e.visibility & FRAGMENT));
    perBgl.set(bgl, tex.length);
    perBglBindings.set(bgl, tex.map((e) => e.binding));
    return bgl;
  };
  const cpl = GPUDevice.prototype.createPipelineLayout;
  GPUDevice.prototype.createPipelineLayout = function (desc) {
    const n = (desc?.bindGroupLayouts ?? []).reduce((s, b) => s + (b ? perBgl.get(b) ?? 0 : 0), 0);
    stats.max = Math.max(stats.max, n);
    if (!stats.layouts.includes(n)) stats.layouts.push(n);
    const layout = cpl.apply(this, arguments);
    perLayout.set(layout, n);
    perLayoutGroups.set(layout, (desc?.bindGroupLayouts ?? []).map((b) => (b ? perBglBindings.get(b) ?? [] : [])));
    return layout;
  };
  const record = (desc) => {
    if (desc?.fragment?.module && tiltModules.has(desc.fragment.module)) {
      const sampled = perLayout.get(desc.layout) ?? null;
      if (!stats.tilt.some((t) => t.sampled === sampled)) stats.tilt.push({ sampled });
    }
    const w = desc?.fragment?.module && waterModules.get(desc.fragment.module);
    if (w) {
      const sampled = perLayout.get(desc.layout) ?? null;
      if (!stats.water.some((k) => k.pass === w.pass && k.sampled === sampled && k.ibl === w.ibl && k.transmission === w.transmission))
        stats.water.push({ ...w, sampled });
    }
    const bm = desc?.fragment?.module && boidModules.get(desc.fragment.module);
    if (bm) {
      const sampled = perLayout.get(desc.layout) ?? null;
      // As for the kit: the layout binds the view transmission texture at group 0 binding 24.
      const transmission = ((perLayoutGroups.get(desc.layout) ?? [])[0] ?? []).includes(24);
      if (!stats.boid.some((k) => k.sampled === sampled && k.ibl === bm.ibl && k.transmission === transmission))
        stats.boid.push({ sampled, ibl: bm.ibl, transmission });
    }
    const mod = desc?.fragment?.module && kitModules.get(desc.fragment.module);
    if (!mod) return;
    const { pass, ibl } = mod;
    const sampled = perLayout.get(desc.layout) ?? null;
    const groups = perLayoutGroups.get(desc.layout) ?? null;
    // Bevy binds the view transmission texture at group 0 binding 24 (mesh_view_bindings.rs) in every
    // mesh pipeline of a view with ScreenSpaceTransmission, whether or not the shader reads it.
    const transmission = (groups?.[0] ?? []).includes(24);
    if (!stats.kit.some((k) => k.pass === pass && k.sampled === sampled && k.ibl === ibl && k.transmission === transmission))
      stats.kit.push({ pass, sampled, ibl, transmission, groups });
  };
  for (const name of ["createRenderPipeline", "createRenderPipelineAsync"]) {
    const orig = GPUDevice.prototype[name];
    GPUDevice.prototype[name] = function (desc) {
      record(desc);
      return orig.apply(this, arguments);
    };
  }
}

// Observes which shader modules contain `marker` (a global only the kit material declares) and
// whether the WGSL compat shim (web/wgsl-compat.js) rewrote them. A wrapper installed now sits
// under the shim and sees the code it passes on; a second one, installed once the shim has
// published `window.__wgslCompatRewrite`, sits above it and sees the original. `window.__markedModules`
// lists `{ before, after }` handle-alias counts per marked module: `before > 0 && after === 0`
// means the shim rewrote that module completely.
export function markedModuleProbe(marker) {
  const re = new RegExp("\\b" + marker + "\\b");
  const mods = [];
  window.__markedModules = mods;
  let pending = null;
  const count = (code) => (window.__wgslCompatRewrite ? window.__wgslCompatRewrite(code).aliases : null);
  const below = GPUDevice.prototype.createShaderModule;
  GPUDevice.prototype.createShaderModule = function (desc) {
    if (pending && typeof desc?.code === "string" && re.test(desc.code)) pending.after = count(desc.code);
    pending = null;
    return below.apply(this, arguments);
  };
  let rewrite;
  Object.defineProperty(window, "__wgslCompatRewrite", {
    configurable: true,
    get: () => rewrite,
    set(v) {
      rewrite = v;
      // The shim wraps createShaderModule right after publishing the rewriter, in the same task.
      queueMicrotask(() => {
        const shim = GPUDevice.prototype.createShaderModule;
        GPUDevice.prototype.createShaderModule = function (desc) {
          if (typeof desc?.code === "string" && re.test(desc.code)) {
            pending = { before: count(desc.code), after: null };
            mods.push(pending);
          }
          return shim.apply(this, arguments);
        };
      });
    },
  });
}
