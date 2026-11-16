#!/bin/bash
# Run every ui-e2e script against the dist, reporting each result and failing at the
# end if any script failed, so one early failure no longer hides the later scripts.
# (kanban 01a1de316b32 criterion 2)
set -u
DIST="${1:-dist}"
cd "$(dirname "$0")/.." || exit 1
scripts="ui ui-motion photo-fx persist saves-ui audio debug version"
fail=0
for s in $scripts; do
  echo "=== e2e/$s.mjs START"
  if node "e2e/$s.mjs" "$DIST"; then
    echo "=== e2e/$s.mjs PASS"
  else
    echo "=== e2e/$s.mjs FAIL"
    fail=1
  fi
done
exit "$fail"
