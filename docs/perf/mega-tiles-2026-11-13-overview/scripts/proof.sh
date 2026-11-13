#!/bin/bash
python3 - <<'PY'
import json
s=json.load(open('/Users/probello/Repos/hex-world/.claude/worktrees/agent-a1afa85caad816d27/docs/checkpoints/massif_granite/massif-shots.json'))
i=next(k for k,c in enumerate(s) if c.get('name')=='massif-world-state')
out=s[:i]+[{'cmd':'dump_state','name':'flat-massif-state'},{'cmd':'camera','pos':[10,300,270],'look_at':[10,0,-6]},{'cmd':'wait','frames':60},{'cmd':'screenshot','name':'flat-massif-overview'},{'cmd':'exit'}]
json.dump(out,open('/tmp/ovb/massif-smoke.json','w'),indent=1)
PY
for i in 1 2 3; do
  /tmp/ovb/run.sh 6707194 p-mf-$i
  /tmp/ovb/run.sh bfix p-bfix-$i
  /tmp/ovb/run.sh 7d896ec p-head-$i
done
/tmp/ovb/run.sh bfix p-bfix-massif /tmp/ovb/massif-smoke.json
/tmp/ovb/run.sh 7d896ec p-head-massif /tmp/ovb/massif-smoke.json
echo "proof done $(date -u +%H:%M:%SZ)"
