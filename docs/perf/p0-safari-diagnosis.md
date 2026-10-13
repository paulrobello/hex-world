# P0 Safari 27 diagnosis: the frame-3 stall

Date: 2026-10-12. Build: `p0-foundations` at 29016e9 plus this fix. Browser: Safari 27.0.1 (WebKit 605.1.15) on macOS, WebGPU adapter `apple`.

## Result

Safari now renders. Safari never stalled: WebKit rejected one of Bevy 0.20's PBR fragment shaders, and Bevy's render error handler then shut the app down after frame 3.

The cause is the shader construct `let texture = <texture global>;`. Core WGSL only allows a `let` to hold a constructible type or a pointer. Holding a texture or sampler needs the optional language extension `texture_and_sampler_let` (WGSL §4.1.2). Chrome's Tint implements that extension and Safari 27.0.1 does not.

The fix is `web/wgsl-compat.js`, a page-side shim. It is gated on `navigator.gpu.wgslLanguageFeatures`, and it inlines those aliases before WebKit compiles the shader. It makes no change to Rust or Bevy.

**Correction to the Task 10 report.** Task 10 recorded "no console error, warning, exception or rejection". That was wrong. The page logged a validation error and a "Quitting the application" error. Task 10's reporter missed both because it hooked only `console.error` and `console.warn`. Bevy's wasm logger writes every level, ERROR included, through `console.log` with `%c` styling.

## Reproduction (before the fix)

Same method as Task 10, with no Safari setting changed and no Web Inspector:

- **Serving.** A scratch copy of `dist` was served on 127.0.0.1:8094 by a bounded node server, which also accepted POSTs at `/__log` and `/__code`.
- **Tracer.** `tracer.js` was injected as the first script in `index.html`. It does the following:
  - Wraps the `GPUDevice` creation methods, `createRenderPipelineAsync` and `createComputePipelineAsync`, `popErrorScope`, `GPUQueue.submit`, `writeBuffer`, `writeTexture` and `onSubmittedWorkDone`, `GPUBuffer.mapAsync` and `unmap`, and `GPUCanvasContext.configure` and `getCurrentTexture`.
  - Hooks `device.lost` and `uncapturederror`, and wraps the page's `requestAnimationFrame`.
  - Logs each call and each promise settlement with a page rAF counter.
  - Captures every `console.*` level, including `console.log`.
  - In the second pass, wraps each render and compute pipeline creation in its own validation error scope, calls `getCompilationInfo()` on every shader module, and uploads the WGSL source of any module that fails.
- **Driving Safari.** AppleScript opened and sized the window. Screenshots used computer use at read tier.

Default scenario (`/`, single tile), first run, trace excerpt (times in ms since page start):

```
 595  createRenderPipeline#1 label=opaque_mesh_pipeline
 629  queue.submit#1, #2                                   (frame 1)
 765  queue.submit#3, #4                                   (frame 2)
3257  uncapturederror Fragment module is invalid
3257  ERROR bevy_render error_handler.rs:139 Caught rendering error: Fragment module is invalid
3263  ERROR bevy_render error_handler.rs:79  Quitting the application due to Validation RenderError
3268  WARN  bevy_ecs command_queue.rs:220 CommandQueue has un-applied commands being dropped   (x48)
```

After the quit, the app makes no more `requestAnimationFrame` requests. The app's own counter stopped at 10 and `hexWorld.state().frames` stayed at 3, but the page's own rAF kept running. That is the "stall" Task 10 saw. No WebGPU promise was left pending: `pending` was empty, and every `popErrorScope` settled. The hypothesis that a promise never settles is ruled out.

Second run, with per-pipeline error scopes and compilation info:

```
compile        module#7 len=75220 error L51:9 'texture_2d<f32>' cannot be used as the type of a 'let'
pipeline-error createRenderPipeline pbr_opaque_mesh_pipeline vertex=module#6,fragment=module#7 :: Fragment module is invalid
uncapturederror setPipeline: invalid RenderPipeline
```

All 21 other render and compute pipelines validated. Line 51 of the composed fragment WGSL is this:

```wgsl
if ((flags & STANDARD_MATERIAL_FLAGS_BASE_COLOR_TEXTURE_BIT) != 0u) {
    let texture = _1bevy_pbr_render__1pbr_bindings__2base_color_texture;   // line 51
    let sampler_ = _1bevy_pbr_render__1pbr_bindings__2base_color_sampler;
    let coords = uv;
    pbr_input.material.base_color *= textureSampleBias(texture, sampler_, coords, bias.mip_bias);
}
```

The same pattern repeats for the emissive, metallic-roughness and occlusion textures, which makes 8 aliases in this module.

## Root cause

1. **Bevy's shader source.** Bevy 0.20 writes these aliases in `crates/bevy_pbr/src/render/pbr_fragment.wesl`: 52 lines of `let texture = …` and `let sampler_ = …`, under `@if(VERTEX_UVS)`. The same pattern appears in `pbr_prepass.wesl`, `pbr_prepass_functions.wesl` and `light_probe/irradiance_volume.wesl`. Bevy `main` at 12a94e0 (2026-10-12) still has the same 52 lines.
2. **No translation step on the web.** Bevy passes the composed WGSL text straight to `wgpu::ShaderSource::Wgsl`. That happens in `bevy_render/src/render_resource/pipeline_cache.rs` (`load_module`). wgpu 30's WebGPU backend then hands the text unchanged to `GPUDevice.createShaderModule` (`wgpu-30.0.1/src/backend/webgpu.rs`, around line 1972), so nothing on the way rewrites the aliases.
3. **naga accepts it.** Native builds and Bevy's own validation never complain, because naga 30 accepts a `let` of handle type with no extension check. That is upstream gfx-rs/wgpu#7393, "naga should restrict the allowed types of `let` bindings", which is still open. The shaders were written and tested against a validator more permissive than the spec.
4. **Chrome and Safari differ on the extension.** Core WGSL requires a `let` initializer to be constructible or a pointer (WGSL §6.2.1). The language extension `texture_and_sampler_let` "allows the effective-value-type of a let-declaration to be a texture or sampler type" (WGSL §4.1.2, https://www.w3.org/TR/WGSL/). Their advertised `wgslLanguageFeatures` were:

   | Browser | `texture_and_sampler_let` | Full list |
   |---|---|---|
   | Chromium 153 (headless, Metal) | yes | packed_4x8_integer_dot_product, subgroup_uniformity, immediate_address_space, subgroup_id, linear_indexing, readonly_and_readwrite_storage_textures, swizzle_assignment, unrestricted_pointer_parameters, buffer_view, pointer_composite_access, texture_formats_tier1, texture_and_sampler_let, uniform_buffer_standard_layout |
   | Safari 27.0.1 | **no** | packed_4x8_integer_dot_product, pointer_composite_access, readonly_and_readwrite_storage_textures, texture_formats_tier1, unrestricted_pointer_parameters |

   So WebKit is spec-conformant here: Bevy relies on an optional extension without checking for it.
5. **Bevy quits on the error.** A validation error is fatal in Bevy's default render error policy, so the whole app stops at the first invalid pipeline. On wasm, `load_module` cannot catch shader errors early (see the comment at `pipeline_cache.rs:170`). The error arrives asynchronously through `uncapturederror`, which explains the 2.5 s gap between frame 2 and the quit.

Every hex-world scene uses `StandardMaterial` on meshes with UVs (the hex test tile, the kit pieces), so `VERTEX_UVS` is always set and every scenario hit the error. That explains why Task 10 saw the identical failure in every config, including post none with shadows off. hex-world's own shaders (`assets/shaders/*.wesl`, `boids_compute.wgsl`) contain no handle-typed `let`.

## Bisect

The tracer named the failing module directly, so the plugin-by-plugin bisect from the brief was not needed. The minimal case is a standalone page with no engine. The table below runs from smallest to largest.

| Case | Safari 27.0.1 | Chromium 153 |
|---|---|---|
| Standalone repro `docs/perf/p0-safari-wgsl-let-repro.html`: fragment shader with `let t = tex; let s = samp;` | compile error 5:3 `'texture_2d<f32>' cannot be used as the type of a 'let'`, pipeline error "Fragment module is invalid" | compiles, pipeline ok |
| Same repro, handles used directly | compiles, pipeline ok | compiles, pipeline ok |
| hex-world default single scenario, no shim | quits after frame 3 (pbr_opaque_mesh_pipeline) | renders |
| hex-world single, shim active | READY, renders (screenshot checked), 0 errors | inert by default (native support); forced with `?wgsl_compat=1`: READY, 0 errors |
| gate2000 strategic, instanced, tuned shadows, shim | READY, 600 samples, 0 errors | forced shim: READY, 0 errors |
| gate2000 close-up, instanced, tuned shadows, shim | READY, 600 samples, 0 errors | forced shim (entities layout): READY, 0 errors, frame checked visually |
| gate2000 strategic, instanced, mid1200 shadows, shim | READY, 600 samples, 0 errors | not re-run |
| gate2000 strategic, entities layout, shim | READY, 600 samples, 0 errors | forced shim: READY, 0 errors |

**Upstream Bevy example.** An upstream Bevy example such as `3d_scene` was not built. Its plane and cuboid meshes have UVs and use `StandardMaterial`, so the same `pbr_fragment` lines would be compiled. That is an inference from the shader source, not a run.

**Shared warning.** One warning appears in both browsers, so it is not Safari-specific: on the entities layout, `shaders/boid_material.wesl has an unresolved import: bevy_pbr::render::mesh_bindings`. It comes from Bevy's shader cache resolving the material before the `bevy_pbr` library is registered. Chrome logs it on the same scene and renders normally, so this is not part of the Safari failure.

## Fix

`web/wgsl-compat.js` loads from `index.html` before the WebGPU gate and the wasm. Here is what it does:

- **When it acts.** It wraps `GPUDevice.prototype.createShaderModule` only when the browser does not list `texture_and_sampler_let`. `?wgsl_compat=1` forces it on (used to verify the rewrite under Tint) and `?wgsl_compat=0` turns it off.
- **The rewrite.**
  - It collects the module-scope handle globals, meaning every `var NAME: texture_*`, `sampler` or `sampler_comparison`.
  - It walks the source with a block-scope stack.
  - Each `let N = G;` where G resolves to a handle global is removed, and `N` is replaced by `G` until the enclosing `{}` block closes. It never touches `.N` member accesses, and inner declarations shadow correctly.
  - Sibling blocks reuse `texture` and `sampler_` for different globals, which is why a module-wide replace would be wrong.
- **What it leaves alone.** A handle `let` in another form, such as the bindless `let texture = bindless_textures_2d[i];`, is left as is, counted as unhandled and logged with `console.warn`. `binding_array<texture_*|sampler>` globals are recognized as handles. The indexed form is not inlined, because moving the index expression to each use site could change when it is evaluated. A `for (...)` header declaration is registered in the enclosing scope, which can only leave an alias un-inlined (a loud compile error), never substitute wrongly. Bindless is off on WebGPU in this build, and no such case occurred.
- **Stats.** Rewrite counts are exposed on `window.__wgslCompat`. In Safari: 46–47 modules seen, 1–2 rewritten (the PBR fragment shader, plus a second module on the entities layout), 8–16 aliases inlined, 0 unhandled.

Verification of the rewrite:

- **naga 30.0.1.** `naga` validates both the original and the rewritten 75,220-byte fragment module.
- **Tint.** Chromium with `?wgsl_compat=1` compiles every rewritten module and reaches READY in the single, gate2000 strategic instanced and gate2000 close-up scenarios with 0 console errors.
- **Safari.** All configs reach READY with 0 compile errors, 0 pipeline errors and 0 `uncapturederror`.
- **Chrome smoke.** `make web-smoke` passes. The shim is inert by default, and smoke case 4 forces it (`?wgsl_compat=1`) on the single scenario and on gate2000 close-up with the instanced layout. In each it asserts READY, exactly 8 rewritten aliases, 0 unhandled and 0 console errors. Every smoke case now fails on Bevy's `%cERROR%c` marker, on text containing "uncaptured" or "invalid", and on any WebGPU `uncapturederror`, not just on `console.error`.

## Workaround options considered

| Option | Verdict |
|---|---|
| A. Page-side WGSL rewrite at the WebGPU boundary, feature-gated (`web/wgsl-compat.js`) | **Chosen.** It acts on the final composed WGSL, so `@if` branches are already resolved. It needs no Bevy fork, is inert where the extension exists, and can be deleted once Bevy or WebKit fixes this. Its cost is one rewrite pass at startup, on Safari only: 12 ms for the 75 KB PBR module (Safari, 1 ms resolution) and 11.3 ms in Chromium when forced. |
| B. Override Bevy's embedded WESL from Rust (rewrite the `EmbeddedAssetRegistry` bytes for the four files) | Rejected. It races the embedded-asset loader, and it means maintaining patched copies of four large Bevy files, each with `@if` alternatives. |
| C. `[patch.crates-io]` a forked `bevy_pbr` that drops the aliases | Viable but heavy. It forks a 0.20 crate for a one-line-per-site change. Keep it as the fallback if the shim ever misses a form. |
| D. Wait for Bevy, or for WebKit to ship `texture_and_sampler_let` | Not a P0 option. Track it upstream and delete the shim when either lands. |

## Draft upstream issue (Bevy; not filed)

**Title:** PBR shaders use `let` with texture and sampler types, which needs the `texture_and_sampler_let` WGSL extension: Safari 27 WebGPU fails "Fragment module is invalid" and the app quits

**Body:**

> Bevy 0.20.0 (also present on main at 12a94e0), WebGPU backend on wasm32, Safari 27.0.1 on macOS.
>
> Any 3D scene using `StandardMaterial` on a mesh with UVs quits after the first frames in Safari:
>
> ```
> ERROR bevy_render::error_handler: Caught rendering error: Fragment module is invalid
> ERROR bevy_render::error_handler: Quitting the application due to Validation RenderError
> ```
>
> `getCompilationInfo()` on the composed `pbr_fragment` module reports:
> `error L51:9 'texture_2d<f32>' cannot be used as the type of a 'let'`.
>
> `pbr_fragment.wesl` (and `pbr_prepass.wesl`, `pbr_prepass_functions.wesl`, `irradiance_volume.wesl`) bind handles with `let texture = pbr_bindings::base_color_texture;` and `let sampler_ = …`. Core WGSL requires a `let` to be constructible or a pointer, and texture and sampler types are only allowed under the optional language extension `texture_and_sampler_let` (WGSL §4.1.2). Chrome advertises it in `navigator.gpu.wgslLanguageFeatures`. Safari 27.0.1 does not, and rejects the shader. naga accepts the pattern without the extension (gfx-rs/wgpu#7393), so this is invisible natively and in Chrome.
>
> Minimal standalone repro (no Bevy): a fragment shader with `let t = tex; let s = samp; return textureSample(t, s, uv);` fails in Safari 27 and passes in Chrome. The same shader with `textureSample(tex, samp, uv)` passes in both.
>
> Suggested fix: use the bindings directly at each sample site, or select the binding through a WESL `@if` at the call site instead of a `let` alias. A shader-validation CI step that rejects handle-typed `let` without the extension (or naga#7393) would prevent regressions.
>
> Workaround we ship: a page script that wraps `GPUDevice.createShaderModule` and inlines these aliases when `wgslLanguageFeatures` lacks `texture_and_sampler_let`.

Secondary targets:

- **gfx-rs/wgpu#7393.** Add this case as evidence: naga should reject handle-typed `let` unless the extension is enabled.
- **WebKit.** File a feature request to implement `texture_and_sampler_let`.

## Safari timings

Configuration: gate2000, instanced layout, tuned shadows (`shadow_casters=large`, 3 cascades, 2,000 m), TAA, SSAO, bloom and DOF on. Each figure is the median of 3 runs over the 600 frames after READY, with host 1-minute load ≤ 15 before and after every run (`e2e/perf-load.mjs`, `PERF_LOAD_WAIT_MIN=40`; the guard discarded and re-ran 3 samples). The light tracer (`trace=0`) was active and did no per-call WebGPU wrapping.

| View | p50 | p95 [range] | mean | frames over 25 ms per run | console errors |
|---|---|---|---|---|---|
| strategic | 17 ms | 17 ms [17–18] | 16.67 ms | 0, 15, 0 | 0 |
| close-up | 17 ms | 18 ms [17–18] | 16.69 ms | 0, 1, 3 | 0 |

SSAO was active (`ssao: true`) and GPU preprocessing was `PreprocessingOnly`. Per-run data is in the `"safari"` block of `docs/perf/p0-safari-gate.json`, and the Safari column of `docs/decisions/0001-engine.md` carries the same figures. Against the pre-set paced criterion (p95 ≤ 17.0 ms), Safari strategic meets it at 1 ms resolution, and close-up misses by one timer tick. Chrome's paced rows are 17.3–17.5 ms. Strategic run 3 was re-run, because the first attempt read a load-discarded page (see the JSON caveats).

The Safari rows are paced, not uncapped:

- **Pacing.** Safari has no uncapped mode and paces rAF at 60 Hz on this display.
- **Timer resolution.** Safari coarsens `performance.now()` to 1 ms without cross-origin isolation, so per-frame times are whole milliseconds. p50 and p95 read as 17.00 at a true 16.67 ms cadence, and the mean (16.67 ms) is the precise figure.
- **Display.** The only attached display was the built-in Retina panel (1496×967 pt, DPR 2). The canvas was 1496×887 CSS, which is 2992×1774 device pixels and more pixels than the Chrome rows' 2560×1440.
