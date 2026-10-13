# P0 engine gate diagnosis

**Answer.** The instanced shipping layout passes the 16.6 ms uncapped p95 budget on Bevy WebGPU only when shadows are reduced. With full shadows it fails. The shadow setting is a visual trade-off, not a free win. Figures are from the fix-round re-measure (rows M*, FP*), with SSAO now sampled by the instanced pieces.

| Shadow setting | What casts | Uncapped p95, strategic / closeup | Mean, strategic / closeup | Uncapped frames over 25 ms per 600 | Budget |
|---|---|---|---|---|---|
| Tuned: `shadow_casters=large`, 3 cascades, 2,000 m (M3, M4) | No static shadows at strategic zoom (every chunk is LOD2, and `large` drops LOD2). In the closeup, only LOD0/LOD1 bases, edges and centers cast, which means near pieces only | 3.6 [3.6–3.7] / 3.7 [3.6–3.8] | 3.44 / 3.58 | 5–11 | pass, 12.9 ms headroom |
| Middle: all casters, 3 cascades, `shadow_dist` 600 m (M5, M6) | Every piece within 600 m of the camera. At strategic zoom that is 13,100 LOD2 instances in the last cascade; in the closeup, all 3 cascades | 3.7 [3.7–3.7] / 3.8 [3.7–3.8] | 3.50 / 4.33 | 4–21 | pass, 12.8 ms headroom |
| Middle: all casters, 3 cascades, `shadow_dist` 1,200 m (M7, M8) | Every piece within 1,200 m. At strategic zoom that is all 40,000 pieces | 4.7 [4.4–4.9] / 4.2 [4.0–4.3] | 4.18 / 5.03 | 14–30 | pass, 11.9 ms headroom |
| Full: all casters, 4 cascades, 2,000 m (M1, M2) | Everything | 20.4 [13.3–24.0] / 45.7 [24.6–45.9] | 4.92 / 6.12 | 20–43 | **fail** |

- **Every uncapped instanced configuration has a stall tail.** Between 4 and 43 of the 600 frames take longer than 25 ms (max 35–223 ms). The middle and full settings have the most. p95 hides this for every setting except full.
- **Paced at 60 Hz (FP1–FP4), every row fails the criterion set before the runs (p95 ≤ 17.0 ms).** Full shadows medians are 17.3 / 17.3 ms; tuned shadows medians are 17.5 / 17.3 ms.
- **Dropped frames when paced.** Full shadows dropped 0 frames (over 25 ms) in 6 runs. Tuned shadows dropped 1 frame (39.4 ms) in 6 runs.
- **What the paced p95 means.** The means of 16.66–16.69 ms are consistent with the jitter interpretation: frames are presented on time, but the presentation interval jitters. That is an interpretation, not a pass.
- **Middle setting, where to start.** The 600 m middle setting keeps shadows on every near piece and costs about the same p95 as tuned. It does lengthen the closeup tail: 20 frames over 25 ms per run, against 8–11 for tuned.

Bevy's per-entity cost, not the GPU, made the earlier layouts miss the budget. The shipping architecture removes that cost while still rendering inside Bevy's passes: p50 frame time drops from 24–28 ms to about 3 ms.

## Fix round 1 re-measure (supersedes W3–W6 and P3–P6 below)

- **Why the re-measure.** The first-round instanced rows below (W3–W6, P3–P6) were measured before the instanced fragment shader sampled SSAO. Bevy samples SSAO in `pbr_input_from_standard_material`, which the instanced shader does not call. They are kept for history.
- **How it ran.** All rows here are web, under the host-load guard (load ≤ 15 before and after every run; 7 uncapped and 2 paced samples were discarded and re-run). There are 3 round-robin runs per row, each in a fresh browser.
- **Data files.** `docs/perf/p0-gate-matrix-fix1.json` and `docs/perf/p0-gate-matrix-paced-fix1.json` (`MATRIX_ONLY=web MATRIX_CONFIGS=instanced-full,instanced-tuned,instanced-mid600,instanced-mid1200`, and for the paced file `MATRIX_PACED=1 MATRIX_ONLY=web MATRIX_CONFIGS=instanced-full,instanced-tuned`).
- **SSAO check.** Debug-rendering the instanced pieces' `diffuse_occlusion` shows contact darkening in creases and at piece bases. The effect on the final image is small, because ambient light (400) is about 1% of the 30,000 lx sun, so the entities and instanced closeups still match.

**Uncapped.** Median of 3 runs with min–max in brackets. "Over 25 ms" is per run.

| Row | Config | View | p50 | p95 | p99 (run 1) | Mean | Max | Over 25 ms |
|---|---|---|---|---|---|---|---|---|
| M1 | full: all casters, 4 cascades, 2,000 m | strategic | 3.0 | 20.4 [13.3–24.0] | 38.2 | 4.92 [4.89–5.00] | 60.0 | 21, 21, 20 |
| M2 | full | closeup | 3.1 | 45.7 [24.6–45.9] | 49.2 | 6.12 [6.01–6.25] | 76.9 | 40, 43, 28 |
| M3 | tuned: `large`, 3 cascades, 2,000 m | strategic | 2.9 | 3.6 [3.6–3.7] | 22.7 | 3.44 [3.37–3.49] | 53.7 | 5, 7, 11 |
| M4 | tuned | closeup | 3.0 | 3.7 [3.6–3.8] | 32.3 | 3.58 [3.56–3.67] | 56.1 | 8, 8, 11 |
| M5 | middle: all, 3 cascades, 600 m | strategic | 2.9 | 3.7 [3.7–3.7] | 22.4 | 3.50 [3.50–3.52] | 35.4 | 4, 9, 12 |
| M6 | middle 600 | closeup | 2.8 | 3.8 [3.7–3.8] | 50.0 | 4.33 [4.27–4.67] | 223.1 | 20, 21, 20 |
| M7 | middle: all, 3 cascades, 1,200 m | strategic | 2.9 | 4.7 [4.4–4.9] | 29.5 | 4.18 [4.15–4.42] | 178.6 | 20, 24, 20 |
| M8 | middle 1,200 | closeup | 2.9 | 4.2 [4.0–4.3] | 60.5 | 5.03 [4.80–5.04] | 77.6 | 29, 30, 14 |

**Shadow draws per cascade (run 1), as draws / instances.**

| Row | Cascade 0 | Cascade 1 | Cascade 2 | Cascade 3 |
|---|---|---|---|---|
| M1 | 0 | 0 | 5 / 31,800 | 5 / 40,000 |
| M2 | 10 / 2,800 | 15 / 12,600 | 15 / 33,420 | 15 / 40,000 |
| M3 | 0 | 0 | 0 | n/a |
| M4 | 6 / 1,120 | 6 / 4,704 | 6 / 5,936 | n/a |
| M5 | 0 | 0 | 5 / 13,100 | n/a |
| M6 | 10 / 2,800 | 15 / 10,080 | 15 / 29,180 | n/a |
| M7 | 0 | 0 | 5 / 40,000 | n/a |
| M8 | 10 / 2,800 | 15 / 18,620 | 15 / 37,240 | n/a |

**Paced at 60 Hz.** Criterion set before the runs: p95 ≤ 17.0 ms and p99 < 25 ms.

| Row | Config | View | p95 | p99 per run | Mean | Max | Over 25 ms per run | Criterion |
|---|---|---|---|---|---|---|---|---|
| FP1 | full | strategic | 17.3 [17.0–17.4] | 17.7, 17.1, 17.6 | 16.67 | 17.9 | 0, 0, 0 | **fail** (p95) |
| FP2 | full | closeup | 17.3 [17.3–17.4] | 17.6, 17.7, 17.5 | 16.66–16.67 | 18.0 | 0, 0, 0 | **fail** (p95) |
| FP3 | tuned | strategic | 17.5 [17.4–17.9] | 17.6, 18.0, 17.8 | 16.66–16.67 | 18.1 | 0, 0, 0 | **fail** (p95) |
| FP4 | tuned | closeup | 17.3 [17.1–17.5] | 17.7, 17.5, 17.8 | 16.66–16.69 | 39.4 | 1, 0, 0 | **fail** (p95) |

Of the first-round paced rows below, P4–P6 also fail the p95 ≤ 17.0 criterion (17.1–17.9 ms medians); P3 sits exactly at 17.0 and passes.

**Shipping visual gap (F4, F7).** The instanced layout picks LOD per chunk centroid with a hard cut at 150 and 400 m. It has none of the ±5 m crossfade band that the entities layout's `VisibilityRange` gives. As a result, closeup union triangles are about 5% lower (8.66 M vs 9.08 M), and LOD changes pop a whole chunk at a time. A shipping build needs per-piece or dithered LOD selection, for example in the vertex shader or with a per-chunk crossfade. That is a visual gap, not a performance claim.

## Sources and method

- **Data.** `docs/perf/p0-gate-matrix.json` (`make perf-matrix`) and `docs/perf/p0-gate-matrix-paced.json` (`MATRIX_PACED=1 MATRIX_ONLY=web MATRIX_CONFIGS=entities-full,instanced-full,instanced-tuned`). Both ran on 2026-10-12 on an Apple M4 Max.
- **Scene.** gate2000 at 2560x1440: 2,000 tiles, 40,000 static pieces, 300 vehicles and 1,000 GPU boids. Every run echoed back the config it was asked for (`echo_mismatch` is empty for all 66 recorded runs).
- **Repetition.** Each configuration ran 3 times. The repeats were round-robin, so drift does not land on one configuration. Every web run used a fresh headless Chromium 153 (`apple | metal-3`). This task ran no other job during measurement. Other sessions shared the host, and the load guard handled them.
- **Uncapped web.** Uses `--disable-gpu-vsync --disable-frame-rate-limit`, like `perf-gate.mjs`.
- **Paced web.** Keeps Chromium's vsync and its 60 Hz frame-rate limit, so that run behaves like a shipping 60 Hz app.
- **Native.** The release binary with `--present immediate`, run for 20 s. It is launched by the runner, so window focus is not guaranteed (finding F8).
- **Timing.** p50, p95, p99 and mean cover the 600 frames after READY. Tables give the median of 3 runs with the min–max in brackets. The mean is new in this task (`diag.rs`): when submission runs ahead of the GPU, p50 shows the CPU submit time and the mean shows the throughput actually sustained.
- **Instanced layout (`crates/hex-render/src/instanced.rs`).** One 48-byte record per piece: rotation, translation, scale and palette index. The records are grouped by kit mesh, then by 7-hex chunk. For each view (the camera and each cascade), the CPU frustum-culls chunks and picks each chunk's LOD from the camera distance to the chunk centroid (150/400 m). It then writes the surviving ranges into a per-frame id list. Each view issues at most one draw per (kit mesh, LOD), so at most 15. The draws are `NonMesh` items in Bevy's `Opaque3d`, `Opaque3dPrepass` and `Shadow` phases. The pieces therefore write depth, normals and motion vectors for SSAO and TAA, cast into every cascade, and are lit by `apply_pbr_lighting` with shadows received. SSAO is sampled in the instanced fragment shader the way `pbr_input_from_standard_material` does. That holds from fix round 1 on: the first-round W3–W6 and P3–P6 rows below were measured without it. Triangles and pieces per LOD equal the entities layout (`instances_match_the_entities_layout_at_every_lod`).

## First round: web, uncapped (budget: p95 ≤ 16.6 ms; W3–W6 superseded by M1–M4)

| Row | Config | View | p50 ms | p95 ms | Mean ms | Pass |
|---|---|---|---|---|---|---|
| W1 | entities, full shadows, full post | strategic | 24.2 [24.2–24.3] | 25.0 [24.9–25.0] | 24.24 [24.19–24.31] | no |
| W2 | entities, full shadows, full post | closeup | 28.0 [27.8–28.0] | 28.6 [28.5–28.7] | 27.99 [27.77–28.02] | no |
| W3 | instanced, full shadows, full post | strategic | 3.3 [3.1–3.3] | 21.8 [19.1–25.7] | 4.86 [4.85–4.87] | no |
| W4 | instanced, full shadows, full post | closeup | 3.2 [3.0–3.2] | 43.8 [31.3–45.9] | 6.19 [6.07–6.24] | no |
| W5 | instanced, `large` + 3 cascades, full post | strategic | 2.9 [2.7–3.0] | 3.7 [3.6–3.7] | 3.51 [3.38–3.53] | yes |
| W6 | instanced, `large` + 3 cascades, full post | closeup | 2.9 [2.8–3.1] | 3.6 [3.6–3.7] | 3.55 [3.50–3.71] | yes |
| W7 | instanced, shadows off, post none | strategic | 1.9 [1.9–2.0] | 2.6 [2.5–2.7] | 1.94 [1.92–1.98] | yes |
| W8 | instanced, shadows off, post none | closeup | 1.7 [1.5–1.9] | 2.4 [2.3–2.5] | 1.76 [1.51–1.96] | yes |

**Memory and load.**

- **wasm memory.** The entities layout peaks at 0.39–0.40 GB (W1, W2). The instanced layout peaks at 0.07 GB (W3–W8).
- **Time to READY.** The entities layout reaches READY in 4.6–5.0 s. The instanced layout with full post takes 1.1–1.4 s.

## First round: web, paced at 60 Hz (P3–P6 superseded by FP1–FP4)

| Row | Config | View | p95 ms | p99 ms (per run) | Mean ms | Dropped frames (inferred from mean and p99; not counted) |
|---|---|---|---|---|---|---|
| P1 | entities, full | strategic | 25.0 [24.9–25.1] | 25.7, 25.2, 25.5 | 24.21 [24.21–24.37] | every frame misses 60 Hz |
| P2 | entities, full | closeup | 28.7 [28.6–28.9] | 29.2, 49.5, 29.0 | 27.91 [27.78–28.23] | every frame misses 60 Hz |
| P3 | instanced, full | strategic | 17.0 [17.0–17.1] | 17.5, 17.3, 17.3 | 16.67 in all 3 runs | none |
| P4 | instanced, full | closeup | 17.1 [17.0–18.1] | 18.6, 17.3, 17.2 | 16.66–16.67 | none |
| P5 | instanced, `large` + 3 cascades | strategic | 17.9 [17.0–18.3] | 17.1, 18.2, 18.6 | 16.67 in all 3 runs | none |
| P6 | instanced, `large` + 3 cascades | closeup | 17.1 [16.9–18.1] | 17.2, 18.4, 17.5 | 16.66–16.67 | none |

**How to read the paced rows.**

- **A paced run cannot show headroom**, and its p95 is pinned at about 16.67 ms or above. This is why the gate's p95 ≤ 16.6 ms is measured uncapped.
- **What marks a dropped frame.** A dropped frame shows up as an interval of 33 ms or more, and it lifts the mean above 16.67 ms. A p95 of 17–18 ms with a mean of 16.67 ms is jitter in the presentation interval, not a missed frame.
- **Criterion set before the paced runs:** p95 ≤ 17.0 ms and p99 < 25 ms.
  - Every instanced row meets p99 < 25 ms in every run.
  - P4–P6 **fail** the p95 criterion (17.1, 17.9 and 17.1 ms). That their means sit at 16.67 ms is consistent with presentation jitter rather than drops; that is an interpretation, not a pass.
- **The load guard changed the result.** An earlier paced run of P4, taken without the guard while the host's 15-minute load average was above 30, dropped frames in 2 of 3 runs (p99 46.6 and 32.1 ms). Under the guard it dropped none. That earlier run is discarded.

## Per-pass draw census (I4)

Each cell is the first web run of that configuration, sampled outside the measured window.

- **Bevy cells** are bins in that view's phase: one instanced draw per distinct mesh+material. On web, `PreprocessingOnly` turns multi-draw batch sets into plain bins.
- **Instanced cells** are the non-empty (kit mesh, LOD) draws that view issued, then the number of instances and triangles.

In the instanced rows, each view's Bevy bins are the 300 vehicles plus the flock. The 15 instanced slots appear as `non_mesh` in `passes`, and empty slots skip their draw.

| View / phase | W1 entities strategic | W2 entities closeup | W3 instanced full strategic | W4 instanced full closeup | W6 instanced tuned closeup |
|---|---|---|---|---|---|
| main opaque | 241 bins | 595 bins | 41 bins + 5 draws, 38,380 inst, 3.75 M tris | 39 bins + 15 draws, 13,160 inst, 2.71 M tris | 39 bins + 15 draws, 13,160 inst, 2.71 M tris |
| main prepass | 240 bins | 594 bins | 40 bins + 5 draws (same instances as main) | 38 bins + 15 draws | 38 bins + 15 draws |
| cascade 0 shadow | 0 | 336 bins | 0 bins + 0 draws | 8 bins + 10 draws, 2,800 inst, 1.75 M tris | 8 bins + 6 draws, 1,120 inst, 0.72 M tris |
| cascade 1 shadow | 0 | 631 bins | 0 bins + 0 draws | 40 bins + 15 draws, 12,600 inst, 4.38 M tris | 40 bins + 6 draws, 4,704 inst, 2.08 M tris |
| cascade 2 shadow | 240 bins | 640 bins | 40 bins + 5 draws, 31,800 inst, 3.11 M tris | 40 bins + 15 draws, 33,420 inst, 7.53 M tris | 40 bins + 6 draws, 5,936 inst, 2.54 M tris |
| cascade 3 shadow | 240 bins | 640 bins | 40 bins + 5 draws, 40,000 inst, 3.91 M tris | 40 bins + 15 draws, 40,000 inst, 8.66 M tris | (3 cascades) |
| union over views | 3.92 M tris, LOD 0/0/40,000 pieces | 9.08 M tris, LOD 2,181/13,530/25,199 | 3.91 M tris, LOD 0/0/40,000 | 8.66 M tris, LOD 1,960/12,880/25,160 | 7.09 M tris |

## Findings

**F1. The entities layout's cost is CPU-side per-entity work, and the instanced layout removes it.**
- The two layouts draw the same scene. In the strategic view the union over views is 40,000 LOD2 pieces and 3.9 M triangles in both (W1, W3).
- p50 falls from 24.2 to 3.3 ms in the strategic view (W1 to W3) and from 28.0 to 3.2 ms in the closeup (W2 to W4).
- The entities layout spends its time on 120,301 mesh entities across 6 views, not on draw submission (241–640 bins per view). The instanced layout replaces them with at most 15 draws per view over a CPU chunk cull of 309 chunks.

**F2. The instanced layout with full shadows has the throughput but misses the uncapped budget on frame stalls.**
- Mean frame time is 4.9 and 6.2 ms (W3, W4), but uncapped p95 is 21.8 and 43.8 ms, with a 3.2–3.3 ms p50 underneath.
- With full shadows, these stalls reach p95. In W5–W8, p95 stays within 1 ms of p50, but the fix-round rows show every instanced setting has a stall tail: 4–43 frames over 25 ms per 600 (M1–M8). The tail grows with shadow load: 8.66 M cascade-3 triangles, and 21.3 M shadow triangles per frame in W4.
- Paced, full shadows show 0 frames over 25 ms in 6 runs and a mean of 16.66–16.67 ms (FP1, FP2). Their p95 of 17.3 ms fails the 17.0 ms criterion, so paced holding of 60 Hz is consistent with the data but not certified.
- **Not measured: the cause of the stalls.** There are no GPU timestamps. Two candidates remain open:
  - backpressure in Chrome's GPU process under heavy shadow passes;
  - the per-frame upload of the id list, about 400 KB with full shadows and about 100 KB tuned.

**F3. Shadow tuning is the lever that makes the budget.**
- `large` + 3 cascades cuts W4's 33,420 + 40,000 instances in the far cascades to 5,936 in the last one (W6).
- p95 drops from 43.8 to 3.6 ms in the closeup and from 21.8 to 3.7 ms in the strategic view (W4 to W6, W3 to W5).
- The spread across 3 runs is 0.1 ms (W5, W6). The 2 ms (strategic) and 6 ms (closeup) spread seen in earlier tasks did not recur, so the 12.9 ms margin is many times the spread.

**F4. The post stack and shadows together cost about 1.1–1.8 ms on top of bare rendering in the tuned instanced layout.**
- p95 goes from 2.6 to 3.7 ms (W7 to W5) and from 2.4 to 3.6 ms (W8 to W6). Mean goes from 1.94 to 3.51 ms and from 1.76 to 3.55 ms.
- The prepass is real work here: the census shows the same 15 draws in the prepass as in the main pass (W4, W6).

**F5. Memory and load improve as well.**
- wasm memory peaks at 0.07 GB instead of 0.40 GB, and time to READY is 1.1–1.4 s instead of 4.6–5.0 s (W3–W6 vs W1, W2).
- Native peak footprint is 1.44–1.65 GB with shadows, versus 1.92–2.20 GB for entities (N3–N6 vs N1, N2).

**F6. The tuned setting removes static shadows entirely at strategic zoom.**
- The chunk LOD is chosen from the camera distance. In the strategic view (camera about 1,100 m out) every chunk is LOD2, and `large` excludes LOD2, so cascades 0–2 issue 0 instanced draws (W5 census: `cascade0..2` 0 draws). Only vehicles cast shadows there.
- In the closeup, LOD0 and LOD1 bases, edges and centers cast (W6).
- Whether flat strategic lighting is acceptable is a product call. Fix round 1 measured middle settings (all casters, 3 cascades, `shadow_dist` 600 m and 1,200 m; M5–M8), which restore strategic shadows and still pass.

**F7. Triangle counts are close to the entities layout, not identical, because LOD is chosen per chunk.**
- In the closeup, union triangles are 8.66 M instanced versus 9.08 M entities (W4 vs W2). Pieces per LOD are 1,960/12,880/25,160 versus 2,181/13,530/25,199.
- Entities choose LOD per piece and draw both LODs inside the ±5 m crossfade band. Instanced chooses LOD per chunk centroid with a hard cut.
- Per-LOD instance counts and triangle totals are equal by construction (unit test). Only what is visible at each LOD differs.

**F8. The native rows are upper bounds and do not decide anything.**
- `--present immediate` still produced 120 Hz-like p50s (about 8.33 ms) in N1 (3 of 3 runs), N2 (3/3), N4 (3/3) and N6 (1/3). The runner's window may not be focused, and macOS may pace the window anyway.
- Native means are also above web for the instanced layout (N8 4.19 ms vs W8 1.76 ms), which points at presentation pacing rather than rendering cost.
- Native confirms the visual result: `make shots-instanced` matches the entities gallery, and cast shadows are present. A shadows on/off diff of the instanced closeup has 11,639 pixels darker by more than 20 levels.

## Native rows (upper bounds, F8)

| Row | Config | View | p50 ms | p95 ms | Mean ms | Vsync-like runs |
|---|---|---|---|---|---|---|
| N1 | entities, full | strategic | 8.41 [8.33–8.42] | 13.34 [13.30–14.61] | 8.66 [8.26–9.00] | 3/3 |
| N2 | entities, full | closeup | 8.28 [8.26–8.29] | 13.54 [13.37–13.57] | 8.21 [8.19–8.32] | 3/3 |
| N3 | instanced, full | strategic | 7.42 [6.95–7.58] | 9.10 [8.92–12.07] | 6.47 [6.33–6.71] | 0/3 |
| N4 | instanced, full | closeup | 8.11 [8.10–8.13] | 14.21 [13.84–14.25] | 7.61 [7.29–7.78] | 3/3 |
| N5 | instanced, `large` + 3 cascades | strategic | 5.83 [5.81–6.19] | 8.35 [8.35–10.32] | 5.64 [5.59–6.03] | 0/3 |
| N6 | instanced, `large` + 3 cascades | closeup | 6.01 [5.33–16.47] | 9.52 [8.56–17.08] | 5.82 [5.70–12.84] | 1/3 |
| N7 | instanced, shadows off, post none | strategic | 3.94 [2.89–4.02] | 7.29 [6.82–10.09] | 4.25 [4.18–4.95] | 0/3 |
| N8 | instanced, shadows off, post none | closeup | 4.05 [3.24–5.54] | 7.10 [6.73–7.57] | 4.19 [4.19–4.61] | 0/3 |

## What this means for Bevy versus a custom wgpu renderer

- **Bevy carries the shipping static layout on web WebGPU with tuned shadows and 12.9 ms of headroom (W5, W6).**
- **With full shadows its uncapped tail fails the gate (W3, W4), and paced at 60 Hz it misses p95 ≤ 17.0 ms (FP1, FP2: 17.3 ms) with 0 frames over 25 ms.**
- **The gap to the budget was Bevy's per-entity ECS path, not WebGPU or the GPU.** A custom render path inside Bevy's own passes removes it (F1), and keeps Bevy's PBR lighting, shadows, SSAO (sampled in the instanced shader since fix round 1), TAA and prepasses.
- **The shadow trade-off on Bevy WebGPU, uncapped p95, strategic / closeup.**
  - Tuned: 3.6 / 3.7 ms, with no static shadows at strategic zoom and near-only casters in the closeup (M3, M4).
  - Middle, all casters, 3 cascades, 600 m: 3.7 / 3.8 ms (M5, M6).
  - Middle at 1,200 m: 4.7 / 4.2 ms (M7, M8).
  - Full: fails, at 20.4 / 45.7 ms (M1, M2).
  - Every instanced setting has an uncapped stall tail of 4–43 frames over 25 ms per 600 frames, and no paced row meets p95 ≤ 17.0 ms (FP1–FP4).
- **What a custom wgpu renderer would still have to beat is GPU shadow cost (F2).** That cost is set by how many instances each cascade draws, so it applies to any renderer.

## Not measured

- **GPU time per pass.** There are no GPU timestamps, so the cause of the full-shadow stalls (F2) is open.
- **GPU-side culling and compaction.** It would remove the per-frame id upload and is the obvious next experiment if full shadows are required.
- **Chunk size 19 in the instanced layout.** Every row used chunk 7.
- **`shadow_dist` values other than 600 m and 1,200 m (M5–M8).**
- **Native rows with a guaranteed focused window and no presentation pacing.**
- **The merged `chunks` layout.** It was not re-run. The Task 12 numbers stand, and that layout is not the shipping architecture.
