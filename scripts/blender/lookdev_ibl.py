"""make lookdev-ibl: HDRI -> assets/kit/p1/ibl/noon.{specular,diffuse}.ktx2 (+ hashes to lookdev.json)."""
import json
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from lib import bake, ibl, ktx2  # noqa: E402

hdri = pathlib.Path(sys.argv[sys.argv.index("--") + 1])
out = pathlib.Path("assets/kit/p1/ibl")
out.mkdir(parents=True, exist_ok=True)
eq = ibl.load_equirect(hdri)
counts = {
    "noon.specular.ktx2": ktx2.write_cubemap_rgba16f(str(out / "noon.specular.ktx2"),
                                                     ibl.prefilter(eq, base=256, samples=128)),
    "noon.diffuse.ktx2": ktx2.write_cubemap_rgba16f(str(out / "noon.diffuse.ktx2"), ibl.irradiance(eq, size=32)),
}
for name, c in counts.items():
    print(f"lookdev-ibl: {name}: {c['clamped']} channel values clamped to 65504, {c['nonFinite']} non-finite")
meta = pathlib.Path("assets-source/lookdev/lookdev.json")
doc = json.loads(meta.read_text()) if meta.exists() else {}
doc["ibl"] = {"source": str(hdri), "sourceSha256": bake.sha256_file(hdri), "specularBase": 256, "samples": 128,
              "files": {p.name: bake.sha256_file(p) for p in sorted(out.glob("*.ktx2"))},
              "float16": counts}
meta.parent.mkdir(parents=True, exist_ok=True)
meta.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
