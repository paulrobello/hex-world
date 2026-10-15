"""rock_small_b: a large granite boulder, about 1.9 x 1.6 m and 0.8 m proud of the turf, with one
shoulder higher, in rock_small's weathered granite with moss and lichen (refs:
assets-source/refs/meadow_props). The pipeline is `_rock.build`. Scripted route, prop kind (LOD0
<= 1,500 tris): make kit-build PIECE=rock_small_b.

About 4x rock_small's surface, so the albedo is 1024² to hold 256 px/m; normal and ORM ship at
512² (box-filtered from the 1024² bake) to keep the KTX2 family under the piece budget."""
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _rock  # noqa: E402

_rock.build("rock_small_b", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__,
            stones=[_rock.Stone(semi=(0.95, 0.78, 0.68), centre=(0.0, 0.0, 0.10), planes=12, keep=(0.72, 0.86),
                                tilt=(0.16, 0.06))],
            bury=-0.27, lod0_tris=1400, atlas=1024, normal_size=512, orm_size=512, mask_size=256)
