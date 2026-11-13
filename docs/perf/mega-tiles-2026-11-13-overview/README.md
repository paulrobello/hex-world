# Mega tiles flat-world overview residual: root cause and fix (2026-11-13)

Follow-up to `../mega-tiles-2026-11-13-parity/` (on branch `worktree-agent-ae7322c2c6b46f949`). That run left one deterministic difference between MAIN+FIX and the branch in a world with no massif: at the `overview` camera, 245-322 pixels differed (61 over 2/255, max 17-20) in two clusters, small grey marks on the NE beach sand and small brown marks on the shallow seabed near the east river mouth.

## Cause

**The marks are the boids, not scatter.** They are the paused bird flock over the beach and a fish school in the shallows. The flat-parity script turns on photo mode with `pause_agents`, so the flock holds still and looks like static props. Commit `6942098` ("lift the boid band over massifs alone") added the peak lift to `assets/shaders/boids_compute.wgsl`: a `lift_at(p.xz)` call, with a loop over `params.peaks`, inside `fly()`. In a world with no massif the loop runs zero times, so the arithmetic is the same on paper, but the compiled compute pass is not. Its flight step differs in the last bits, and the flock integrates that over about 1,400 frames into slightly different poses at the same places.

The brief's leading hypothesis was wrong in two places:

- **The onset is not `151b3a1`.** The 2026-10-29 debug report already measured the overview at `151b3a1` as matching main. That bisect was of the play-camera speckle, which the hold fix (`a09e359`) resolved. Here `151b3a1` matches MAIN+FIX again: 4 / 0 / 1 at overview-late, and 0 / 0 / 0 against the second MAIN+FIX run.
- **It is not draw order or asset ids.** The pixels belong to boids, and a shader file swap alone (below) removes the difference.

## Evidence

Probe: `scripts/flat-parity-late.json`, the 2026-10-30 flat-parity script with two extra overview shots after the last camera. Its `overview-late` shot is byte-identical run to run within an arm (151b3a1 against itself: 0 / 0 / 0). Every bisect binary runs with `a09e359`'s `capture.rs` grafted on (`scripts/build.sh`) and with its own commit's `assets/` as the working directory. The full output is in `bisect-and-variants.txt`. Figures are differing pixels / pixels over 2/255 / largest channel delta, out of 1,600,000.

**Bisect** (first-parent commits, overview-late against `151b3a1`):

| Commit | overview-late |
|---|---|
| `151b3a1` (second run), MAIN+FIX `6707194` | 0-4 / 0 / 0-1 |
| `3c0855c` | 2 / 0 / 1 |
| **`6942098`** | **321 / 61 / 18** |
| `11437e3`, `165f491`, `897c216`, `63b2064`, `e351e56`, `a00c825`, `cf2c2c9`, `b06ac09`, head `7d896ec` | 321-326 / 61 / 18 |

**Intervention.** All of these use the head binary `7d896ec`. Only `assets/shaders/boids_compute.wgsl` in its run tree is swapped, because the compute shader loads from `assets/` at run time. Each is compared to MAIN+FIX at overview-late. The shader sources are in `shader-variants/`.

| Shader | Result |
|---|---|
| main's (`old`) | 4-6 / 0 / 1 (two runs): parity |
| `v3`: main's code plus the new `Params` fields (`peak_count`, `peaks`) | 4 / 0 / 1: the uniform layout is harmless |
| `v1`: `lift_at` behind a runtime `if (params.peak_count > 0u)` | 325 / 61 / 18 |
| `v2`: main's two band lines, with `lift_at` behind the same runtime `if` | 325 / 61 / 18 |
| `fix` (`#ifdef PEAKS`) | does not parse: this shader is loaded as plain WGSL with no preprocessor, so the boids did not run |
| `va`: two entry points, `fly` byte-for-byte main's, a duplicated `fly_peaks` | 10 / 0 / 1: parity |
| `vb`: two entry points over a shared `advance(i, peaks: bool)` with a literal argument, `fly` taking `lift` | 4 / 0 / 1: parity (shipped) |

Any `lift_at` reachable from the flat world's pass changes the flock, even behind a uniform-driven branch. Keeping it out of that entry point does not.

## Fix

`assets/shaders/boids_compute.wgsl`: `fly(i, me, c, lift)` takes the band lift as an argument. `update` calls `advance(id.x, false)`, which passes `params.lift`. The new `update_peaks` entry point calls `advance(id.x, true)`, which passes `lift_at(me.pos.xz)`.

`crates/hex-play/src/boids.rs`: `BoidsPipeline` queues both entry points, and `dispatch_boids` runs `update_peaks` only while `Params::peak_count > 0` (`BoidsBindGroup::peaks`). The shader test pins both entry points' calls. A world without a massif therefore runs a pass that compiles to the same flight arithmetic as main.

The fix does not belong on main. Main has no peak code, and this is a branch regression. The general lesson does apply to main: any edit to `boids_compute.wgsl`, even dead or zero-trip code, can move a fixed-clock flock and show up in a parity diff.

## Proof (booked GPU window 11:45-13:45Z)

`scripts/proof.sh` interleaves three runs each of MAIN+FIX (`6707194` = main `c9688ea` + `a09e359`. The branch has since absorbed main `af7c4be`, which adds only a lint fix and a web CSP fix, and main is now further on. Binary `be1ce440`, the same binary as the 2026-11-13 parity run's MAIN+FIX), BRANCH+FIX (this commit's tree, binary `ea83dd1b`) and the unfixed branch head (`7d896ec`), 12:01-12:07Z. All exited 0. Host 1-minute load was 25-42 during the proof runs (`runs-load.txt`). These are fixed-clock pixel captures, not timings. The full table is in `parity-table.md`:

| Camera | MAIN+FIX vs itself (3) | BRANCH+FIX vs itself (3) | **MAIN+FIX vs BRANCH+FIX (9)** | MAIN+FIX vs unfixed branch (9) |
|---|---|---|---|---|
| overview | 216-225 / 5-6 / 4-5 | 241-470 / 5-11 / 5-6 | **165-448 / 2-9 / 3-6** | 425-517 / 55-63 / 20 |
| play | 203-254 / 4-6 / 5-7 | 206-249 / 5-7 / 5-7 | **172-266 / 4-7 / 3-7** | 145-245 / 1-8 / 3-9 |
| play-road-farm | 42-54 / 2-5 / 4-5 | 49-55 / 2-5 / 3-5 | **9-53 / 0-5 / 1-5** | 20-63 / 0-5 / 2-5 |
| close | 53-103 / 6-8 / 4-7 | 35-97 / 5-9 / 4-7 | **2-101 / 0-9 / 1-7** | 3-102 / 0-9 / 1-7 |
| low-road-cars | 14-483 / 0-1 / 1-3 | 0-480 / 0-1 / 0-3 | **2-485 / 0-1 / 1-3** | 329-813 / 7-8 / 3 |
| low-level1 | 0-21 / 0 / 0-2 | 0-21 / 0 / 0-2 | **0-21 / 0 / 0-2** | 0-21 / 0 / 0-2 |
| low-coast | 2-117 / 0 / 1-2 | 0-117 / 0 / 0-2 | **0-117 / 0 / 0-2** | 0-117 / 0 / 0-2 |
| low-farm | 9-441 / 0-39 / 1-23 | 0-2 / 0 / 0-1 | **3-444 / 0-39 / 1-23** | 0-444 / 0-39 / 0-23 |
| overview-late | 0-1 / 0 / 0-1 | 0 / 0 / 0 | **0-1 / 0 / 0-1** | 321-322 / 61 / 18 |
| overview-later | 0-1 / 0 / 0-1 | 0 / 0 / 0 | **0-1 / 0 / 0-1** | 245-251 / 61 / 17 |

Every camera's MAIN+FIX vs BRANCH+FIX range sits inside the arms' own run-to-run ranges. The low-road-cars residual of 7-8 pixels on the pine branch, left open by the 2026-11-13 parity run, is gone too: it was a bird of the same flock. The PNGs are not committed. They stay in `/tmp/ovb/runs/<label>/`, scratch on the capture host. `overview-late-mainfix-branch-fixed-diffx10.jpg` shows the two clusters, MAIN+FIX, the unfixed branch, BRANCH+FIX, then each against MAIN+FIX ×10.

**The massif path still runs (indirect evidence).** `massif-shots.json`'s world, which has two massifs, was run up to its state dump with BRANCH+FIX. It dispatches 1,000 boids with the pieces ready and no pipeline error, and the overview renders both peaks. `dispatched` alone does not show which entry point ran, since the unfixed binary reports the same count. What does show it is the 484 / 44 / 17 difference against the unfixed branch, where the peak lift is compiled into `update`. Had the fixed binary run `update` without the lift over two peaks, the birds near the massifs would have diverged far more. No dedicated check that the birds clear the peaks ran here. The plan-side test `flocks_clear_the_massif_peak` and the shader test cover the arithmetic.
