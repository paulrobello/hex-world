"""Checks the photo-effects checkpoint (make shots-photo-fx) and writes its measurements.

Reads target/shots/photo-fx-*.png and the state dumps, asserts each effect did what it says, and
writes docs/checkpoints/photo/photo-fx-measure.json. Needs ImageMagick (`magick`).
"""

import json
import subprocess
import sys

SHOTS = "target/shots"
OUT = "docs/checkpoints/photo/photo-fx-measure.json"
FX_KEYS = ["filter", "filter_strength", "vignette", "vignette_softness", "grain", "aberration"]


def magick(name: str, expr: str, *ops: str) -> str:
    path = f"{SHOTS}/{name}.png"
    return subprocess.run(["magick", path, *ops, "-format", expr, "info:"], capture_output=True, text=True,
                          check=True).stdout.strip()


def mean(name: str, *ops: str) -> float:
    return float(magick(name, "%[fx:mean]", *ops))


def measure(name: str) -> dict:
    corner = mean(name, "-gravity", "NorthWest", "-crop", "12%x12%+0+0", "+repage")
    centre = mean(name, "-gravity", "Center", "-crop", "12%x12%+0+0", "+repage")
    return {
        "size": magick(name, "%wx%h"),
        "mean": round(mean(name), 4),
        "saturation": round(mean(name, "-colorspace", "HSL", "-channel", "G", "-separate"), 4),
        "corner_over_centre": round(corner / centre, 4),
    }


def photo(name: str) -> dict:
    with open(f"{SHOTS}/{name}.json") as f:
        return json.load(f)["photo"]


def main() -> int:
    names = ["photo-fx-00-none", "photo-fx-01-mono", "photo-fx-02-sepia", "photo-fx-03-vignette",
             "photo-fx-04-grain", "photo-fx-05-aberration", "photo-fx-06-combined", "photo-fx-07-export-2x",
             "photo-fx-08-exit-orbit"]
    m = {n: measure(n) for n in names}
    none, mono, sepia = m["photo-fx-00-none"], m["photo-fx-01-mono"], m["photo-fx-02-sepia"]
    vig, combined, export = m["photo-fx-03-vignette"], m["photo-fx-06-combined"], m["photo-fx-07-export-2x"]
    rmse = {}
    for n in ["photo-fx-04-grain", "photo-fx-05-aberration"]:
        out = subprocess.run(["magick", "compare", "-metric", "RMSE", f"{SHOTS}/photo-fx-00-none.png",
                              f"{SHOTS}/{n}.png", "null:"], capture_output=True, text=True).stderr
        rmse[n] = float(out.split("(")[1].split(")")[0])
    states = {n: photo(n) for n in ["photo-fx-06-combined", "photo-fx-08-exit", "photo-fx-09-reenter"]}
    checks = [
        (all(v["mean"] > 0.02 for v in m.values()), "no shot is black"),
        (mono["saturation"] < 0.01, f"black and white has no colour (saturation {mono['saturation']})"),
        (sepia["saturation"] > 0.05, f"sepia is tinted (saturation {sepia['saturation']})"),
        (vig["corner_over_centre"] < 0.5 * none["corner_over_centre"],
         f"vignette darkens the corner against the centre ({vig['corner_over_centre']} vs "
         f"{none['corner_over_centre']} without)"),
        (rmse["photo-fx-04-grain"] > 0.004, f"grain changes the image (RMSE {rmse['photo-fx-04-grain']})"),
        (rmse["photo-fx-05-aberration"] > 0.004,
         f"chromatic aberration changes the image (RMSE {rmse['photo-fx-05-aberration']})"),
        (export["size"] == "2560x1600", f"the 2x export is 2560x1600 (got {export['size']})"),
        (abs(export["corner_over_centre"] - combined["corner_over_centre"])
         <= 0.25 * combined["corner_over_centre"] + 0.01,
         f"the 2x export keeps the viewfinder's vignette ({export['corner_over_centre']} vs "
         f"{combined['corner_over_centre']})"),
        (abs(export["saturation"] - combined["saturation"]) < 0.03,
         f"the 2x export keeps the viewfinder's colour ({export['saturation']} vs {combined['saturation']})"),
        (states["photo-fx-06-combined"]["fx_active"] is True, "the camera carries the pass in photo mode"),
        (states["photo-fx-08-exit"]["fx_active"] is False and states["photo-fx-08-exit"]["on"] is False,
         "leaving photo mode takes the pass off the game camera"),
        (states["photo-fx-09-reenter"]["fx_active"] is True
         and all(states["photo-fx-09-reenter"][k] == states["photo-fx-06-combined"][k] for k in FX_KEYS),
         "re-entering keeps the effect settings"),
    ]
    failed = 0
    for ok, what in checks:
        print(("ok: " if ok else "FAIL: ") + what)
        failed += not ok
    with open(OUT, "w") as f:
        json.dump({"shots": m, "rmse_vs_none": rmse,
                   "state": {n: {k: s[k] for k in ["on", *FX_KEYS, "fx_active"]} for n, s in states.items()}},
                  f, indent=2)
        f.write("\n")
    print(f"wrote {OUT}")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
