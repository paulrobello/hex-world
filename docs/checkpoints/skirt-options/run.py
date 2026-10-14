"""Skirt look options: bake, compress and shoot one column without touching the shipped soil set.

  python3 docs/checkpoints/skirt-options/run.py set <n>      bake + compress soil_alt_<n> into target/skirt-opt/sets/
  python3 docs/checkpoints/skirt-options/run.py shoot <col>  col = current | 1..4

`shoot` clones assets/ (APFS copy-on-write) into target/skirt-opt/<col>/assets, overwrites
kit/p1/materials/soil/soil.{albedo,normal,orm}.{png,bc7.ktx2,astc.ktx2} with the option's files, and runs
the dev binary with that directory as cwd: native asset loading resolves `cwd/assets` (hex-app main.rs
asset_plugin) and screenshots land in `cwd/target/shots`. No Rust, manifest or shipped-asset change.
The option keeps tileM 4 (the manifest is unchanged), so it is a like-for-like swap of the soil set."""
import pathlib
import shutil
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[3]
OPT = ROOT / "target/skirt-opt"
BLENDER = "/Applications/Blender.app/Contents/MacOS/Blender"
SCRIPT = ROOT / "docs/checkpoints/skirt-options/skirt.json"
SHOTS = ("skirt-play", "skirt-close", "skirt-forest")


def run(cmd, cwd=ROOT, log=None):
    with open(log, "w") if log else open("/dev/null", "w") as f:
        r = subprocess.run(cmd, cwd=cwd, stdout=f, stderr=subprocess.STDOUT)
    if r.returncode:
        sys.exit(f"failed ({r.returncode}): {' '.join(map(str, cmd))} (log {log})")


def build_set(n):
    name = f"soil_alt_{n}"
    stage = ROOT / f"target/kit-stage/materials/{name}"
    stage.mkdir(parents=True, exist_ok=True)
    run([BLENDER, "-b", "--factory-startup", "--python-exit-code", "1",
         "-P", "scripts/blender/kit/mat_soil_alt.py", "--", stage, str(n)], log=stage / "blender.log")
    out = OPT / "sets" / name
    out.mkdir(parents=True, exist_ok=True)
    run(["node", "scripts/assets/kit-compress.mjs", "material", name, "--stage", stage, "--out", out,
         "--report", out / "compress-report.json"], log=out / "compress.log")


def shoot(col):
    work = OPT / col
    if work.exists():
        shutil.rmtree(work)
    work.mkdir(parents=True)
    run(["cp", "-cR", ROOT / "assets", work / "assets"])
    if col != "current":
        src = OPT / "sets" / f"soil_alt_{col}"
        dst = work / "assets/kit/p1/materials/soil"
        for ch in ("albedo", "normal", "orm"):
            for ext in ("png", "bc7.ktx2", "astc.ktx2"):
                shutil.copyfile(src / f"soil_alt_{col}.{ch}.{ext}", dst / f"soil.{ch}.{ext}")
    shots = [work / f"target/shots/{s}.png" for s in SHOTS]
    # Under heavy host load (or a Blender bake holding the GPU) whole runs come out as all-black frames,
    # about 26 KB each: retry a few times before giving up.
    for attempt in range(4):
        run([ROOT / "target/debug/hex-world", "--scenario", "play", "--script", SCRIPT, "--size", "1600x1000",
             "--seed", "1", "--fps", "off", "--tilt", "off"], cwd=work, log=work / "run.log")
        if all(p.stat().st_size > 200_000 for p in shots):
            break
        time.sleep(20)
    else:
        sys.exit(f"{col}: shots still blank after 4 runs; re-run `shoot {col}` when the host is quieter")
    (OPT / "shots").mkdir(exist_ok=True)
    for s, p in zip(SHOTS, shots):
        shutil.copyfile(p, OPT / "shots" / f"{col}-{s}.png")
    shutil.rmtree(work / "assets")


if __name__ == "__main__":
    {"set": lambda a: build_set(int(a)), "shoot": shoot}[sys.argv[1]](sys.argv[2])
