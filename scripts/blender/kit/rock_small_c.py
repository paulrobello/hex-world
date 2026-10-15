"""rock_small_c: a low, flat cluster of four granite stones, the largest about 1 m across, all
sitting about 0.1-0.22 m proud of the turf, about 2 m across overall, in rock_small's weathered granite
with moss and lichen (refs: assets-source/refs/meadow_props). The pipeline is `_rock.build`.
Scripted route, prop kind (LOD0 <= 1,500 tris): make kit-build PIECE=rock_small_c.

Each stone is deep (0.2-0.3 m semi-axis) and centred below the pivot, so only a flat, cleaved top
shows and the outer stones stay embedded where the relief dips under them. The four stones carry
about 1.3x rock_small's surface, so the albedo is 1024² and normal and ORM ship at 512²."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _rock  # noqa: E402

FLAT = dict(top_el=(1.25, 1.45), keep=(0.64, 0.80), tilt=(0.03, 0.02))
_rock.build("rock_small_c", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
            stones=[_rock.Stone(semi=(0.52, 0.40, 0.30), centre=(0.05, 0.03, -0.04), yaw=0.3, **FLAT),
                    _rock.Stone(semi=(0.32, 0.26, 0.26), centre=(-0.64, 0.32, -0.05), yaw=0.7, **FLAT),
                    _rock.Stone(semi=(0.28, 0.22, 0.24), centre=(0.46, -0.58, -0.06), yaw=1.9, **FLAT),
                    _rock.Stone(semi=(0.20, 0.16, 0.20), centre=(-0.30, -0.56, -0.05), yaw=2.6, **FLAT)],
            bury=-0.30, lod0_tris=1400, atlas=1024, normal_size=512, orm_size=512, mask_size=256)
