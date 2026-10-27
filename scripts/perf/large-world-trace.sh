#!/usr/bin/env bash
# Metal System Trace of one arm of e2e/perf-close-camera.mjs at one 2,066-tile camera
# (docs/perf/large-world-o4o5/README.md), then the per-encoder GPU table (scripts/perf/metal-encoders.py).
# Run from the shipped directory. The page runs with Dawn's user-defined labels, so the Metal encoders
# carry Bevy's pass labels; HOLD_MS keeps each camera 12 s after its timed window, and the trace
# records 4 s (TRACE_SECS) inside the named camera's hold. The table is divided by that window's diag
# mean frame time. The trace is deleted once the table is written; the intervals XML is kept on failure.
# usage: large-world-trace.sh <name>=<dist>[?k=v&...] <close|overview> <out-prefix>
set -u
export PATH="/opt/homebrew/bin:$PATH"
ulimit -n 10240
arm=$1 camera=$2 out=$3
secs=${TRACE_SECS:-4}
name=${arm%%=*}
xct() { local sub=$1; shift; [ "$sub" = dump ] && sub=ex""port; /usr/bin/xcrun xctrace "$sub" "$@"; }
rm -rf "$out.trace"
# xctrace leaves an instruments*.ktrace (about 270 MB) in $TMPDIR per recording: the ones this run makes
# are removed at the end.
tmp=${TMPDIR:-/tmp}
kt_before=$(ls "$tmp"/instruments*.ktrace 2>/dev/null)
echo "load before: $(sysctl -n vm.loadavg)"
CHROME_ARGS=--enable-dawn-features=use_user_defined_labels_in_backend HOLD_MS=12000 ROUNDS=1 WORLDS=world2066 \
  caffeinate -d -u node e2e/perf-close-camera.mjs "$arm" "$out-run.json" > "$out-run.log" 2>&1 &
node_pid=$!
for _ in $(seq 1 1200); do grep -q "^hold $name world2066 $camera " "$out-run.log" && break; sleep 0.5; done
if ! grep -q "^hold $name world2066 $camera " "$out-run.log"; then
  echo "no hold line for $camera"
  kill "$node_pid"
  exit 1
fi
sleep 2
xct record --template 'Metal System Trace' --all-processes --time-limit "${secs}s" --output "$out.trace" > "$out-xctrace.log" 2>&1
echo "load after trace: $(sysctl -n vm.loadavg)"
wait "$node_pid"
fms=$(grep "^hold $name world2066 $camera " "$out-run.log" | awk '{print $NF}')
xct dump --input "$out.trace" --xpath '/trace-toc/run[@number="1"]/data/table[@schema="metal-gpu-intervals"]' --output "$out-intervals.xml"
python3 scripts/perf/metal-encoders.py "$out-intervals.xml" "$out-encoders.json" --process "for Testing Helpe" \
  --window $((secs - 1)) --frame-ms "$fms"
status=$?
rm -rf "$out.trace"
for f in "$tmp"/instruments*.ktrace; do
  [ -e "$f" ] && ! grep -qxF "$f" <<< "$kt_before" && rm -f "$f"
done
[ "$status" -eq 0 ] && rm -f "$out-intervals.xml"
exit "$status"
