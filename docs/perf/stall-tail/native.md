# Native stall tail: cause (ADR 0001 carry-over)

Measured 2026-10-14 on `d803233` (main: `bf64195` plus the Settings sheet, tier-change and wet-band
commits), Apple M4 Max, with the built-in Liquid Retina XDR panel (ProMotion, 120 Hz) as the only
display. Workload: the perf-passes native arm (`gate2000`, strategic view, instanced, mid600 shadows,
`tier=high`, `post=full`, `fps=off`, 2560x1440, `--present immediate`, 25 s runs). Every run sat inside
the host-load guard (1-minute load at most 15 before and after; `native.json` records both for every
run, 6.3–14.9, none discarded). 27 runs, 86,147 probed frames, 54 frames over 25 ms.

## Summary

50 of the 54 native frames over 25 ms are waits for the window's next drawable. The other 4 (two
events) are CPU-side stalls that stretched every phase at once. None comes from our main-thread work,
the 20 Hz sim loop, asset or pipeline work, or the boids dispatch, and the earlier `0187620` build
shows the same tail at a similar rate.

- **The native frame is paced by the drawable, not by the CPU.** The main thread's schedules are busy
  0.84–0.98 ms a frame (p50 per run) and the render thread 2.1–2.5 ms outside the drawable wait. The
  render thread then waits in `get_current_texture` (`prepare_windows`) for 5.2–5.9 ms at the median
  (p95 8.3–13.6 ms) for one of the surface's three drawables. Frame intervals peak at 8.3 ms, the
  panel's refresh interval. The earlier notes called these frames "CPU-bound at about 7 ms"; that
  was wrong and is corrected in `README.md` and ADR 0001.
- **The tail is the long end of that wait.** 48 of the 50 frames spent 22–37 ms in
  `get_current_texture` with every other phase at its normal value. The other two split a 14 ms wait
  with 10–11 ms of render queue/prepare.
- **Our own GPU load and present mode do not drive it.** The same waits appear at the same rate with
  `--present fifo` (vsync on) and with `--post none` (about half the GPU work and frame time).
- **About a third follow a GPU spike.** Of the 19 drawable waits in GPU-timed runs, 6 come one or
  two frames after a GPU frame of 15–32 ms and 13 have every GPU frame around them at 4.0–5.3 ms. The
  spikes land in whichever pass is running, and in runs with four or more of them they fall on a
  wall-clock grid of about 1 s. That fits another process's periodic GPU work stretching our
  timeline. This is the best-supported hypothesis, not a proven one: the host had Activity Monitor,
  Chrome, the Codex and Claude desktop renderers, an iOS Simulator runtime and Time Machine running.
- **Two events (four frames) were CPU-side.** Render queue/prepare took 11–48 ms, and in one event
  the main thread's `PostUpdate` took 18.6 ms and then 43.6 ms in the same window. Every phase
  stretched at once, which points at the OS (preemption or a system-wide stall) rather than one
  system. Both were in the first batch; none appeared in the 21 runs after it.

No local fix: the waits sit in macOS's drawable and compositor path and in other processes' GPU
work. Recommendations are at the end.

## Method

A scratch probe (uncommitted) timestamped, every frame, the main world's schedule boundaries (before
`First`, after `First`, `PreUpdate`, `RunFixedMainLoop`, `Update`, `PostUpdate` and `Last`, plus the
fixed-tick count) and the render world's phases (`ExtractSchedule`, `ExtractCommands`, before and
after `prepare_windows`, the end of `PrepareBindGroups`, before and after `render_system`,
`PostCleanup`). The second and third batches also recorded process `getrusage` deltas (CPU time,
context switches, faults). With pipelined rendering, Bevy's `Time` delta is the interval between
two render-thread `send_time` calls. So each frame over 25 ms maps to one render frame whose phases
add up to it. With `--gpu_timing on` the probe also kept every GPU frame from
`hex_render::gpu_timing`, joined to the stall at ±3 frames.

Counts here cover the whole run (frame 121 to the end, about 3,000 per run). `diag.stall` counts
only the last 600 frames; `native.json` carries both (`over_25ms_whole_run`,
`diag_over_25ms_last600`). The probe's frame numbers run one ahead of `diag.stall.tail`, because it
reads `FrameCount` after `Update` increments it.

## Classification

| Class | Frames | Evidence |
|---|---|---|
| Drawable wait, GPU timing off | 31 | 22–37 ms in `get_current_texture`; main schedules and every other render phase normal |
| Drawable wait, GPU frames normal | 13 | the same (two of them a 14 ms wait plus 10–11 ms of queue/prepare), and every GPU frame within 3 frames at 4.0–5.3 ms |
| Drawable wait after a GPU spike | 6 | the wait comes one or two frames after a 15–32 ms GPU frame |
| CPU-side, render queue/prepare | 2 | one event: 25.6 ms in queue/prepare; another: 11.6 ms queue/prepare plus 11.3 ms `render_system`; drawable wait under 5 ms |
| CPU-side, both threads | 2 | one event, frames of 47.2 and 65.5 ms: render queue/prepare 11.3 then 48.1 ms, `render_system` 18.0 ms, main `PostUpdate` 18.6 then 43.6 ms |

Ruled out:

| Suspect | Evidence | Verdict |
|---|---|---|
| Sim ticks catching up | Across 50,426 frames the 20 Hz fixed loop ran 0 ticks in 43,279 and 1 in 7,147, never 2 or more. `tick` also returns early outside `play` | Ruled out |
| Main-thread work (systems, diag census, state publishing) | Main schedules p50 0.84–0.98 ms per run (p99 1.4–1.8 ms in the first batch). At every drawable-wait stall the render thread waited on the main thread for 0.2–0.4 ms | Ruled out |
| Asset or pipeline work | Pipeline queue processing at most 0.03 ms at p99 and `pipelines_waiting` 0 at every stall | Ruled out |
| Boids compute dispatch | Its CPU encoding sits inside `render_system`, which stayed at 1.1–1.7 ms at every drawable-wait stall. Its GPU time is not timed, but halving the GPU work (`--post none`) left the tail unchanged | Ruled out |
| Scatter, photo mode, cars | `scatter` and the car sim run only in `play`, and photo mode only while open. gate2000 runs none of them | Not on this path |
| Allocator, page faults | 40–72 minor faults in stall frames (about 200 in two frames early in a run) against 40 at the median; no major fault in any frame | Ruled out |
| Process starved of CPU during drawable waits | Process CPU of 7–24 ms over a 25–34 ms stall, and context switches below their normal rate: the process is waiting, not preempted | Ruled out |

## A/B: present mode, GPU load and the earlier build

Interleaved, tilt-shift off, every run inside the load guard (loads in `native.json`). Every run's
dumped state reported tier High and the arm's tilt-shift state, on both builds.

| Arm | Frames over 25 ms per run (whole run) | Class | p95 ms (diag) | mean ms (diag) |
|---|---|---|---|---|
| `--present immediate` (perf-passes) | 4, 0, 0 | all drawable waits | 12.0–13.0 | 7.03–7.25 |
| `--present fifo` (vsync) | 1, 1, 1 | all drawable waits | 8.75–8.96 | 8.37–8.40 |
| `--post none` | 3, 1, 0 | all drawable waits | 9.1–9.5 | 4.97–5.16 |
| `d803233`, GPU timing off | 4, 1, 2 | all drawable waits | 16.0–16.7 | 7.62–7.97 |
| `0187620` (the earlier p2-passes build), GPU timing off | 0, 0, 2 | all drawable waits | 12.0–16.7 | 7.19–9.27 |
| `d803233`, GPU timing on | 2, 2, 0 | 2 after a GPU spike, 2 with normal GPU frames | 10.3–12.2 | 7.83–8.26 |
| `0187620`, GPU timing on | 2, 3, 1 | 2 after a GPU spike, 4 with normal GPU frames | 12.2–16.5 | 8.06–8.69 |

Fifo narrows the distribution (p95 8.9 ms against 12–13 ms) but keeps the long waits. Removing TAA,
SSAO, bloom and DOF cuts the median drawable wait from about 5.7 ms to 2.0–2.9 ms and the mean frame
to about 5 ms, and the long waits stay. The earlier build shows the same classes at a similar rate
(8 frames in 6 runs, against 11 in 6 for `d803233`). So the difference between the p2-passes files
(1 frame in 12 runs on `0187620`, 19 in 12 on `bf64195`, both per 600) is host state and sampling,
not the merged boids, scatter, photo or tier code. Three runs per arm cannot rule out a small change
in rate.

## GPU spikes: which pass

GPU frames over 10 ms in the timing-on runs had their slow segment in `post`, `prepass`, `shadow`,
`ssao` and `opaque`, far above its median (for example `prepass` 14.3 ms against 1.0 ms, `post` 7.7 ms
against 0.4 ms), while the other segments stayed normal. One of our passes getting expensive would show
the same segment every time. In three runs with four to nine spikes, the spike times fit a periodic
grid of 0.998–1.008 s (phase coherence 0.99–1.00), for example 6.03, 7.05, 8.06, 10.07, 11.08, 14.11,
17.12 and 22.15 s in one run. The spikes keep wall-clock time, not frame count: in the two runs with
nine spikes each, their frame numbers drift by 20–27 frames against a 128-frame period (modulo 128: 96,
90, 84, 81, 81, 86, 87, 82, 76 in one run, 34 down to 7 in the other), while their times fit a
1.0074–1.0075 s grid with coherence 1.00. A frame-counted event would keep a constant residue. That
pattern fits a timer in another process that does GPU work about once a second. Nothing in our gate2000
path runs on a wall-clock timer: the FPS overlay is hidden with `fps=off`, and the diag, census and
GPU-timing debug log count frames. The p2-passes runs, which had no probe, show the same kind of
spikes. One run of the earlier build had a different burst, 50 consecutive GPU frames of 10–13 ms in
`opaque` around 7 s, which again points at the host. In the two nine-spike runs, 8 of the 30 drawable
waits over 18 ms fall within 50 ms of the grid (10% would by chance). Most long waits have no GPU spike
near them.

## Recommendations

1. **Accept the native tail as host noise with this evidence, or re-measure on a quiet host.** The
   test that would settle it is the owner's to run: `make perf-passes` with `PASSES_ONLY=native`, with
   Activity Monitor, Chrome, the Electron apps and the iOS Simulator closed and Time Machine idle. If
   the 1 s GPU-spike grid and the long drawable waits go away, the tail belongs to the host.
2. **Do not read the native 25 ms count as a budget signal.** On a 120 Hz panel, 25 ms is exactly three
   refresh intervals. The drawable waits form a continuous distribution from 14 to 37 ms (57–89
   frame intervals over 16.7 ms per run in the first batch), so the count swings between 0 and 6
   per run with no code change. If the count must be tracked, track the `diag.stall.histogram`
   buckets instead.
3. **There is no in-app lever for the drawable waits.** The surface already holds three drawables.
   wgpu-hal sets `maximumDrawableCount` to `desired_maximum_frame_latency + 1`, and Bevy's default
   latency of 2 already gives 3, the CAMetalLayer maximum. That leaves no headroom to absorb a late
   release.
4. **The CPU-side events need a span profile to attribute.** There were two in 27 runs. A Tracy
   capture with Bevy's `trace` feature over a long run would name the systems involved. At this rate
   it is worth doing only if the owner sees hitches in play.

## Reproduce

The probe is not committed. To recreate it:

1. Add exclusive marker systems at the schedule and set boundaries listed under Method. Use
   `MainScheduleOrder::insert_before` and `insert_after` for the main world, and
   `.before`/`.after(prepare_windows)` and `.before`/`.after(render_system)` in `Render`.
2. Record `Instant::now()` (and `getrusage`) per frame, and write the records on `AppExit`.
3. Run the perf-passes native arguments with `--dump-state`, wrapped in `e2e/perf-load.mjs`'s
   `guarded`.
