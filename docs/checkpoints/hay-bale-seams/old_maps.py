"""Writes a git revision's round-bale maps (albedo, normal, orm in PNG, BC7 and ASTC) over a copy of
dist/, so the web shots of the old look run on the same build as the new. Usage (repo root):
  python3 docs/checkpoints/hay-bale-seams/old_maps.py <rev> <dist copy>"""
import pathlib
import subprocess
import sys

rev, dist = sys.argv[1], pathlib.Path(sys.argv[2])
for piece in ("hay_bale_round", "hay_bale_round_b"):
    for ch in ("albedo", "normal", "orm"):
        for ext in ("png", "bc7.ktx2", "astc.ktx2"):
            rel = f"assets/kit/p1/{piece}/{piece}.{ch}.{ext}"
            data = subprocess.run(["git", "show", f"{rev}:{rel}"], check=True, capture_output=True).stdout
            dst = dist / rel
            assert dst.exists(), dst
            dst.write_bytes(data)
            print("wrote", dst, len(data))
