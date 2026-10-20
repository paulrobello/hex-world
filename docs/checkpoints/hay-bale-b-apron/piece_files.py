"""Writes hay_bale_round_b's shipped files (the GLB and every map the dist copy carries) over a copy
of dist/, from git revision <rev> or, with "tree", from the working tree, so before and after shots run
on one web build. Usage (repo root): python3 docs/checkpoints/hay-bale-b-apron/piece_files.py <rev|tree> <dist copy>"""
import pathlib
import subprocess
import sys

PIECE = "hay_bale_round_b"
rev, dist = sys.argv[1], pathlib.Path(sys.argv[2])
rel_dir = f"assets/kit/p1/{PIECE}"
names = sorted(p.name for p in (dist / rel_dir).iterdir())
assert f"{PIECE}.glb" in names, names
for name in names:
    rel = f"{rel_dir}/{name}"
    if rev == "tree":
        data = pathlib.Path(rel).read_bytes()
    else:
        data = subprocess.run(["git", "show", f"{rev}:{rel}"], check=True, capture_output=True).stdout
    (dist / rel).write_bytes(data)
    print("wrote", dist / rel, len(data))
