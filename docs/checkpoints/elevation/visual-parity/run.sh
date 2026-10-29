#!/bin/bash
# Elevation visual-parity shots (P4 Task 7, step 4b): visual-parity.json builds a level-1 meadow
# ringed by six hillsides in flat meadow and a two-tile cliff plateau, then shoots the play camera
# and a low camera at the same fixed light, with grass and without. Fixed clocks, as the other play
# shot scripts, so a rerun replays identically. GPU capture: run only inside a booked
# load-sensitive window.
# Usage: docs/checkpoints/elevation/visual-parity/run.sh
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/../../../.." && pwd)
cd "$ROOT"
caffeinate -d -u make shots-piece \
  SCRIPT=docs/checkpoints/elevation/visual-parity/visual-parity.json \
  OUT=docs/checkpoints/elevation/visual-parity \
  ARGS='--scenario play --clock fixed --water_clock fixed'
