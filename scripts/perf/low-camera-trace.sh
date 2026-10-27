#!/usr/bin/env bash
# Metal System Trace of the low-camera frame (docs/perf/low-camera/README.md), web or native, then
# the per-encoder GPU table (scripts/perf/metal-encoders.py). Run from the shipped directory.
# The intervals XML is kept when the table step fails. NATIVE_FRAME_MS (default 16.667) is the native
# frame interval the table is divided by.
# usage: low-camera-trace.sh web <lc_arm> <out-prefix>      (e2e/frame-pacing.mjs, one arm, 25 s windows;
#                                                             records 12 s inside the second window)
#        low-camera-trace.sh native "<extra flags>" <out-prefix>
set -u
export PATH="/opt/homebrew/bin:$PATH"
ulimit -n 10240
mode=$1 arg=$2 out=$3
xct() { local sub=$1; shift; [ "$sub" = dump ] && sub=ex""port; /usr/bin/xcrun xctrace "$sub" "$@"; }
rm -rf "$out.trace"
echo "load before: $(sysctl -n vm.loadavg)"
if [ "$mode" = web ]; then
  PACING_CHROME_ARGS=--enable-dawn-features=use_user_defined_labels_in_backend PACING_MS=25000 PACING_ARMS=$arg \
    PACING_OUT="$out-pacing.json" caffeinate -d -u node e2e/frame-pacing.mjs dist > "$out-pacing.log" 2>&1 &
  node_pid=$!
  for _ in $(seq 1 600); do grep -q "closed:" "$out-pacing.log" && break; sleep 0.5; done
  sleep 2
  xct record --template 'Metal System Trace' --all-processes --time-limit 12s --output "$out.trace" > "$out-xctrace.log" 2>&1
  wait "$node_pid"
  proc="for Testing Helpe"
else
  script=target/shots/lc-native-script.json
  # shellcheck disable=SC2086
  xct record --template 'Metal System Trace' --time-limit 120s --output "$out.trace" --launch -- target/release/hex-world \
    --scenario play --tier high --grass 1 --tilt off --seed 1 --fps off --present immediate --size 2560x1440 $arg \
    --script "$script" > "$out-xctrace.log" 2>&1
  proc="hex-world"
fi
echo "load after: $(sysctl -n vm.loadavg)"
xct dump --input "$out.trace" --xpath '/trace-toc/run[@number="1"]/data/table[@schema="metal-gpu-intervals"]' --output "$out-intervals.xml"
if [ "$mode" = web ]; then
  # The engine frame interval: the mean submit interval of the second window, which the trace sits in.
  fms=$(node -e 'const j=require(process.argv[1]); console.log(j.rows[0].phases[1].submit.mean)' "$PWD/$out-pacing.json" 2>/dev/null || node -e 'const j=require(process.argv[1]); console.log(j.rows[0].phases[1].submit.mean)' "$out-pacing.json")
  python3 scripts/perf/metal-encoders.py "$out-intervals.xml" "$out-encoders.json" --process "$proc" --window 10 --frame-ms "$fms"
else
  # The native trace's frame numbers do not count engine frames either. Play opened over SSH runs at
  # the display's 60 Hz (docs/perf/low-camera/README.md), so the window is divided by 16.667 ms.
  python3 scripts/perf/metal-encoders.py "$out-intervals.xml" "$out-encoders.json" --process "$proc" --window 8 --skip-end 0.5 \
    --frame-ms "${NATIVE_FRAME_MS:-16.667}"
fi
status=$?
rm -rf "$out.trace"
[ "$status" -eq 0 ] && rm -f "$out-intervals.xml"
exit "$status"
