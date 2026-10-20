"""Per LOD and part group, the AO a viewer sees on the shipped maps (`bakeStats.lodVisibleAo`, label
`filled`), before and after the LOD2 head fix: the committed bake report at a git revision against the
report in the working tree. `dark` is the fraction of outside view hits below AO 0.25.

Usage: python3 docs/checkpoints/animal-lod2-head/ao_stats.py <rev> <piece>...
(from the repo root; e.g. HEAD before the rebuilt reports are committed)."""
import json
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
rev = sys.argv[1]
for piece in sys.argv[2:]:
    rel = f"assets-source/kit/{piece}/bake-report.json"
    old = json.loads(subprocess.run(["git", "show", f"{rev}:{rel}"], cwd=ROOT, capture_output=True,
                                    check=True, text=True).stdout)
    new = json.loads((ROOT / rel).read_text())
    print(piece, "lodTris", old["lodTris"], "->", new["lodTris"])
    for lv in ("lod0", "lod1", "lod2"):
        a = old["bakeStats"]["lodVisibleAo"][lv]["filled"]
        b = new["bakeStats"]["lodVisibleAo"][lv]["filled"]
        for g in ("all", "head", "legs", "neck", "body"):
            if g in a or g in b:
                fa, fb = a.get(g, {}), b.get(g, {})
                print(f"  {lv} {g:5s} dark {fa.get('dark')} -> {fb.get('dark')}, p05 {fa.get('p05')} -> "
                      f"{fb.get('p05')}, mean {fa.get('mean')} -> {fb.get('mean')}, hits {fa.get('hits')} -> "
                      f"{fb.get('hits')}")
