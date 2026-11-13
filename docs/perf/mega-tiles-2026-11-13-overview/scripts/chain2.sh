#!/bin/bash
until grep -q 'chain done' /tmp/ovb/bisect.log; do sleep 3; done
for spec in 7d896ec-oldboids:head-oldboids-1 7d896ec-v1:head-v1-1 7d896ec-v2:head-v2-1 7d896ec-oldboids:head-oldboids-2; do
  sha=${spec%%:*}; label=${spec#*:}
  [ "$(date -u +%H%M)" -ge 1338 ] && break
  /tmp/ovb/run.sh "$sha" "$label"
done
echo "chain2 done $(date -u +%H:%M:%SZ)"
