"""Prints the LOD2 neck fix's measurements from each animal's bake report (written by
`_animals.paint_maps`): the hidden-texel AO fill, and per LOD and part group the AO a viewer sees on
the outside view hits (`lod_visible_ao`) for the raw LOD0 bake, the filled map and the map that was
committed before the rebuild: dark = fraction of hits below 0.25, p05, mean. darkHidden / darkFilled:
the share of the raw bake's dark hits on texels LOD0 buries / that the fill rewrites.
Usage: python3 docs/checkpoints/animal-lod2-neck/ao_stats.py [stage|committed] [piece ...]
(stage: target/kit-stage/<piece>/bake-report.json; committed: assets-source/kit/<piece>/)."""
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
PIECES = ["animal_cow", "animal_cow_b", "animal_sheep", "animal_sheep_b", "animal_sheep_c"]
where = sys.argv[1] if len(sys.argv) > 1 else "committed"
pieces = sys.argv[2:] or PIECES
for p in pieces:
    base = ROOT / ("target/kit-stage" if where == "stage" else "assets-source/kit") / p
    r = json.loads((base / "bake-report.json").read_text())
    s = r["bakeStats"]
    print(f"== {p}  lodTris {r['lodTris']}  aoVisible {s['aoVisible']}")
    print("   fill", json.dumps(s.get("hiddenAoFill")))
    for lv, d in (s.get("lodVisibleAo") or {}).items():
        print(f"   {lv} darkHidden {d.get('darkHidden')} darkFilled {d.get('darkFilled')}")
        for lab in ("committed", "raw", "filled"):
            if lab in d:
                cells = "  ".join(f"{g} {v['dark'] * 100:.2f}%/{v['p05']:.2f}/{v['mean']:.3f}" for g, v in d[lab].items())
                print(f"      {lab:9s} {cells}")
