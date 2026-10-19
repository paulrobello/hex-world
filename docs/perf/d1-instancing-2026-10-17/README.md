# D1 instancing pivot — measurement record (2026-10-17)

Branch `perf/play-rendering` @ `76c14f8`. Card: `01a146751eef778f87c4cbd2f38281e3`
("Chunked/instanced play rendering to break the per-entity frame-cost floor").

## What shipped

GPU instancing of settled kit parts (`crates/hex-render/src/kit_instances.rs` + the
`KIT_INSTANCED` shader entry pairs): one 48-byte record per settled part, per-chunk slot
ranges, per-chunk LOD at the 150/400 m bands, per-view frustum culling including shadow
cascades; bevy's material bind group is reused at group 3. `make checkall` and
`make checkall-ci` green at HEAD. The web build now completes the full 2,066-tile world
(~900 MB wasm heap) where the chunk-merge path died in `rust_oom`.

## Criterion 2 — perf-grass grass=off, canonical M4 Max (not met yet)

- Exact-save world (`docs/perf/post-arc001-2026-10-16/local-world.hexworld`), 2,066 tiles,
  guard `exact-save-retry-v2-five-consecutive`, GPU-exclusive, load 9.1→11.9:
  **round 0 p50 17.0 / p95 17.7 / max 31.1; round 1 p50 17.4 / p95 18.3 / max 19.8**
  (`perf-exactsave-v2.json`). Budget 16.6 ms: **not met** — but 2.8× down from the
  pre-pivot 50.8/48.8 on the identical world, with a tight distribution.
- Reproduced twice more (18.0/17.9). `over_25ms`: 4 then 0 frames.
- Correction (2026-10-18): the earlier "renderer_extract is the remaining cost" read was
  wrong — the native samples under `renderer_extract` are 100% `pthread_cond_wait`
  (frame-pacing idle), not compute, and the web block it was rank-matched to inherits
  the error. Post-fix native on the identical world (`--load` the exact save, uncapped,
  13,674 frames): **p50 14.5 / p95 16.64** — the render path is at budget natively; the
  web p95 19.1 is ~2.5 ms of wasm/browser overhead on top. Native CPU is spread thin:
  bevy's schedule executor ~20%, wgpu command encoding ~13%, no app system above 5%.

## Criterion 1 — per-entity systems (verified, checked on the card)

Chrome traces recorded on the exact-save world (`perf-trace-run.json`; the raw
trace + cpuprofile stay out of git, multi-MB). The web profile is index-anonymous
(wasm-bindgen strips the wasm name section — same limitation as the post-arc001 record's
`name_section_bytes: []`), so per-system shares were verified on the **native symbol
profile** of the identical settled world (release `hex-world` binary, 20 s `sample` at
t+180 s and t+220 s, state dump `rising=0`, 13,124 frames):

| system | share |
|---|---|
| check_visibility | 0.13% |
| calculate_bounds | 4.41% (the largest) |
| update_frusta | 0.02% |
| occlusion_culling | 0.19% |
| visibility_propagate | 0.06% |
| reset_view_visibility | 0.11% |
| chunks::settle | 1.38% |
| queue_kit | 0.16% |

All far below the 5% bar (pre-pivot: 14.85 / 10.20 / 9.64%).

## Web rendering regression (resolved 2026-10-18)

Two bugs, both fixed at `3db37c7` + `bb64148`:

1. **Bevy's `SetMaterialBindGroup` aborted every kit draw.** Bevy 0.20's RenderCommand
   tuple impl short-circuits on `Skip`; the stock `SetMaterialBindGroup` reads
   `RenderMaterialInstances`, which extraction only fills for `Mesh3d` entities, so it
   Skipped for the mesh-less kit draw slots and silently dropped every kit-instanced
   draw on **both backends, in every layout** (the "web-only black grounds" was a
   misread — native never rendered the pivot tree's instanced path either; yesterday's
   native evidence pre-dated the pivot). Fixed with `SetKitMaterialBindGroup`, which
   binds the slot's material via the kit scene + `MaterialBindGroupAllocators`. The
   records buffer also gained a 256-byte floor (wgpu validates the bound size against
   the shader's runtime-array minimum, and this app treats validation errors as fatal).
2. **Chunk ranges were within-slot, not flat.** `rebuild()` accumulated per-chunk
   ranges as within-slot instance positions while the draw path indexes the flat
   slot-major record array, so every slot after the first rendered at the first
   records' positions and later-placed tiles went visually missing (the "black hex").
   Fixed by lifting each slot's offset before the chunk set is published.

Verified 2026-10-18: native parity shots (start world matches the committed reference;
five-tile placed world renders fully with shadows); the full 2,066-tile world builds,
settles and renders on WebGPU (river, forest, roads, cars); `make checkall` and
`make checkall-ci` green at `bb64148`.

## Criterion 2 after the fixes (canonical M4 Max, 2026-10-18)

Exact-save world, 2 clean rounds, guard `exact-save-retry-v2-five-consecutive`,
GPU-exclusive (one earlier attempt discarded by the guard for desktop Chrome activity):
**grass=off p95 median 19.1 ms vs the 16.6 ms budget — not met** (`perf-exactsave-v2-postfix.json`).
A third clean run (owner-requested, peak desktop Chrome 0.2-0.5%, load 10-11) reproduced
19.1 exactly (round p50 17.1/17.7) — the ~2.5 ms gap is stable, not noise.
Down from the pre-pivot 50.8/48.8 on the identical world; up slightly from the
pre-fix measurement (18.3) within run-to-run noise on a busier host (load 10-14 vs
9-12). Post-fix native runs the identical world at p50 14.5 / p95 16.64 uncapped:
the render path is at budget, and the web gap is wasm/browser overhead (see the
correction above).

## Criterion 2 amendment (owner decision D1, 2026-10-18)

The owner accepted the measured web number and amended the criterion: the 16.6 ms
budget is held **natively** — met at p50 14.47 / p95 16.64 uncapped on the identical
exact-save world — and the web p95 19.1 is accepted as wasm/browser platform
overhead (three clean GPU-exclusive reproductions of 19.1). The residual levers were
evaluated and rejected with evidence: shadow/post GPU tuning cannot move a CPU-bound
frame (native p50 14.5 vs ≤11 ms serialized GPU; shadow cascades 4.48 ms of it), and
the wasm build already ships `opt-level=3` + thin LTO with the tuned `wasm-release`
profile dead config. The branch also carries `618e875`, the owner-reported wet-band
crawl on cliff walls (the animated dark blotches on exposed coast sides), gated to
flat shores in `kit_material.wesl`.

## Files

- `perf-exactsave-v2.json` — canonical measurement (2 clean rounds, pre-fix)
- `perf-exactsave-v2-postfix.json` — post-fix measurement (2 clean rounds, 2026-10-18)
- `perf-trace-run.json` — traced run (second reproduction)
- `perf-instanced-local.json` — the strict-guard attempt discarded for mid-run desktop
  Chrome activity (kept for the guard-behavior record)
- Raw traces and native sample trees were multi-MB and stay out of git; numbers above are
  the durable record.
