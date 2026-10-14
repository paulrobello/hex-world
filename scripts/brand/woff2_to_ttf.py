# /// script
# dependencies = ["fonttools==4.60.1", "brotli==1.1.0"]
# ///
"""One-time conversion of the self-hosted Manrope WOFF2 to TTF, which resvg can load.

resvg's font database reads TTF/OTF, not WOFF2. The TTF is committed at
design/brand/fonts/ so `make brand` needs no Python. Rerun only when the WOFF2 changes:

    uv run scripts/brand/woff2_to_ttf.py
"""

from pathlib import Path

from fontTools.ttLib import TTFont

ROOT = Path(__file__).resolve().parents[2]
SRC = ROOT / "design/ui/assets/fonts/manrope-latin-wght.woff2"
DST = ROOT / "design/brand/fonts/manrope-latin-wght.ttf"


def main() -> None:
    font = TTFont(SRC, recalcTimestamp=False)  # keep head.modified so reruns are byte-identical
    font.flavor = None
    DST.parent.mkdir(parents=True, exist_ok=True)
    font.save(DST)
    axes = [(a.axisTag, a.minValue, a.maxValue) for a in font["fvar"].axes]
    print(f"wrote {DST.relative_to(ROOT)} axes={axes}")


if __name__ == "__main__":
    main()
