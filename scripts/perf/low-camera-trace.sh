#!/usr/bin/env bash
# Metal System Trace of the low-camera frame (docs/perf/low-camera/README.md), web or native, then
# the per-encoder GPU table (scripts/perf/metal-encoders.py). Run from the shipped directory.
# The intervals XML is kept when the table step fails. Native: HEXW_BIN (default
# target/release/hex-world) is the binary, the run's state goes to <out-prefix>-state.json, and the
# table is divided by that run's mean frame time (diag().mean; NATIVE_FRAME_MS overrides). With
# `--gpu_timing on` among the flags, scripts/perf/metal-segments.py also writes the trace's GPU time
# per gpu_timing segment (<out-prefix>-segments.json), to set against diag().gpu.passes. The native
# trace attaches to the running app for TRACE_SECS (default 4) seconds, ATTACH_AFTER (default 30)
# seconds after launch, inside the script's wait after the camera move: a trace of the whole run
# reached 17 GB and its processing did not finish in 20 minutes on MiniMac.
# usage: low-camera-trace.sh web <lc_arm> <out-prefix>      (e2e/frame-pacing.mjs, one arm, 25 s windows;
#                                                             records 12 s inside the second window)
#        low-camera-trace.sh native "<extra flags>" <out-prefix>
set -u
export PATH="/opt/homebrew/bin:$PATH"
ulimit -n 10240
mode=$1 arg=$2 out=$3
xct() { local sub=$1; shift; [ "$sub" = dump ] && sub=ex""port; /usr/bin/xcrun xctrace "$sub" "$@"; }
rm -rf "$out.trace"
# xctrace leaves an instruments*.ktrace (about 270 MB) in $TMPDIR per recording: the ones this run makes
# are removed at the end.
tmp=${TMPDIR:-/tmp}
kt_before=$(ls "$tmp"/instruments*.ktrace 2>/dev/null)
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
  secs=${TRACE_SECS:-4}
  # shellcheck disable=SC2086
  "${HEXW_BIN:-target/release/hex-world}" --scenario play --tier high --grass 1 --tilt off --seed 1 --fps off \
    --present immediate --size 2560x1440 --diag 1 $arg --dump-state "$PWD/$out-state.json" --script "$script" \
    > "$out-app.log" 2>&1 &
  app_pid=$!
  sleep "${ATTACH_AFTER:-30}"
  xct record --template 'Metal System Trace' --attach "$app_pid" --time-limit "${secs}s" --output "$out.trace" \
    > "$out-xctrace.log" 2>&1
  wait "$app_pid"
  proc="hex-world"
fi
echo "load after: $(sysctl -n vm.loadavg)"
xct dump --input "$out.trace" --xpath '/trace-toc/run[@number="1"]/data/table[@schema="metal-gpu-intervals"]' --output "$out-intervals.xml"
if [ "$mode" = web ]; then
  # The engine frame interval: the mean submit interval of the second window, which the trace sits in.
  fms=$(node -e 'const j=require(process.argv[1]); console.log(j.rows[0].phases[1].submit.mean)' "$PWD/$out-pacing.json" 2>/dev/null || node -e 'const j=require(process.argv[1]); console.log(j.rows[0].phases[1].submit.mean)' "$out-pacing.json")
  python3 scripts/perf/metal-encoders.py "$out-intervals.xml" "$out-encoders.json" --process "$proc" --window 10 --frame-ms "$fms"
else
  # The native trace's frame numbers do not count engine frames either, so the window is divided by
  # the run's own mean frame time.
  fms=${NATIVE_FRAME_MS:-$(node -e 'console.log(require(process.argv[1]).diag.mean)' "$PWD/$out-state.json")}
  win=$((secs - 1))
  python3 scripts/perf/metal-encoders.py "$out-intervals.xml" "$out-encoders.json" --process "$proc" --window "$win" \
    --skip-end 0.5 --frame-ms "$fms"
  status=$?
  case " $arg " in
    *"--gpu_timing on"*) python3 scripts/perf/metal-segments.py "$out-intervals.xml" "$out-segments.json" --process "$proc" \
      --window "$win" --skip-end 0.5 || status=$? ;;
  esac
  (exit "$status")
fi
status=$?
rm -rf "$out.trace"
for f in "$tmp"/instruments*.ktrace; do
  [ -e "$f" ] && ! grep -qxF "$f" <<< "$kt_before" && rm -f "$f"
done
[ "$status" -eq 0 ] && [ -z "${KEEP_XML:-}" ] && rm -f "$out-intervals.xml"
exit "$status"
