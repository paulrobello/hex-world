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
- Remaining cost named: `renderer_extract` — ~20% of frame CPU natively (972/4744 frame-work
  samples), matching the web profile's largest anonymous function (23%, name stripped by
  wasm-bindgen). ~4 ms/frame on web.

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

## Web rendering regression (open)

The instanced path renders **ground-family slots black on WebGPU**; native renders the same
path correctly, and the pre-pivot dist renders the same world colored on web. Silent (no
console or validation error). Not LOD-conditional; foliage slots render. ~2-min repro:
radius-8 world, high + close cameras (see card notes). Root-cause hunting not started.
Criterion 3's parity check needs a redo after the fix.

## Files

- `perf-exactsave-v2.json` — canonical measurement (2 clean rounds)
- `perf-trace-run.json` — traced run (second reproduction)
- `perf-instanced-local.json` — the strict-guard attempt discarded for mid-run desktop
  Chrome activity (kept for the guard-behavior record)
- Raw traces and native sample trees were multi-MB and stay out of git; numbers above are
  the durable record.
