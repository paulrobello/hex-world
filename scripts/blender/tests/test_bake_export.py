import json
import pathlib
import struct
import tempfile
import unittest

import bpy
import numpy as np

from lib import bake, collider, export, hexgeo, materials, scene, uv
from tests import fixture_piece

LOD_MESHES = {"fixture_lod0", "fixture_lod1", "fixture_lod2"}


def glb_json(path):
    data = pathlib.Path(path).read_bytes()
    n = struct.unpack_from("<I", data, 12)[0]
    return json.loads(data[20:20 + n])


def png_rgba(path):
    img = bpy.data.images.load(str(path), check_existing=False)
    img.colorspace_settings.name = "Non-Color"
    a = np.empty(img.size[0] * img.size[1] * 4, np.float32)
    img.pixels.foreach_get(a)
    return img.size[0], a.reshape(-1, 4)


def inside_hex_mask(size, margin=1.0):
    """Pixels whose planar_cap_uv location lies at least `margin` m inside the hex."""
    out = np.zeros(size * size, bool)
    s = 2.0 * hexgeo.HEX_R
    for row in range(size):
        for col in range(size):
            x = (col + 0.5) / size * s - hexgeo.HEX_R
            y = (row + 0.5) / size * s - hexgeo.HEX_R
            out[row * size + col] = hexgeo.dist_to_border(x, y) >= margin
    return out


class BakeExport(unittest.TestCase):
    def setUp(self):
        self.tmp = pathlib.Path(tempfile.mkdtemp())
        self.root, self.lods = fixture_piece.build()

    def test_export_has_names_sockets_tangents_no_images_float_attrs(self):
        out = self.tmp / "fixture.glb"
        export.export_glb(self.root, str(out))
        j = glb_json(out)
        names = {n.get("name") for n in j["nodes"]}
        self.assertTrue({"fixture", "fixture_lod0", "fixture_lod1", "fixture_lod2",
                         "fixture_collider", "socket-lane-0"} <= names, names)
        self.assertNotIn("images", j)
        self.assertNotIn("textures", j)
        self.assertNotIn("KHR_mesh_quantization", j.get("extensionsUsed", []))
        sock = next(n for n in j["nodes"] if n.get("name") == "socket-lane-0")
        self.assertEqual(sock["extras"]["kind"], "road")
        self.assertAlmostEqual(sock["translation"][0], 13.856406, places=3)  # runtime +x
        # Tangents and float UVs are a LOD contract; colliders are exempt.
        checked = set()
        for mesh in j["meshes"]:
            if mesh.get("name") not in LOD_MESHES:
                continue
            checked.add(mesh["name"])
            for prim in mesh["primitives"]:
                self.assertIn("TANGENT", prim["attributes"])
                for a in ("POSITION", "NORMAL", "TANGENT", "TEXCOORD_0"):
                    self.assertEqual(j["accessors"][prim["attributes"][a]]["componentType"], 5126)
        self.assertEqual(checked, LOD_MESHES)
        # Y-up: runtime y spans skirt bottom (-LEVEL_STEP) to the flat cap (0); fails with export_yup=False.
        lod0 = next(m for m in j["meshes"] if m.get("name") == "fixture_lod0")
        pos = j["accessors"][lod0["primitives"][0]["attributes"]["POSITION"]]
        self.assertAlmostEqual(pos["min"][1], -hexgeo.LEVEL_STEP, places=4)
        self.assertAlmostEqual(pos["max"][1], 0.0, places=4)

    def test_export_rejects_zero_area_face_tangents(self):
        # Collinear vertices: Blender exports zero-length tangents, which gltf-validator rejects in
        # kit-compress (ACCESSOR_VECTOR3_NON_UNIT); export_glb must fail first, naming the mesh.
        import bmesh

        me = self.lods[0].data
        bm = bmesh.new()
        bm.from_mesh(me)
        bm.verts.ensure_lookup_table()
        a, b = bm.verts[0].co.copy(), bm.verts[1].co.copy()
        face = bm.faces.new([bm.verts.new(co) for co in (a, (a + b) / 2, b)])
        uvl = bm.loops.layers.uv.active
        for loop, uv_ in zip(face.loops, ((0.1, 0.1), (0.2, 0.1), (0.1, 0.2))):
            loop[uvl].uv = uv_
        bm.to_mesh(me)
        bm.free()
        with self.assertRaisesRegex(ValueError, r"fixture_lod0 primitive \d+ TANGENT"):
            export.export_glb(self.root, str(self.tmp / "degenerate.glb"))

    def test_export_is_deterministic(self):
        a, b = self.tmp / "a.glb", self.tmp / "b.glb"
        export.export_glb(self.root, str(a))
        self.root, self.lods = fixture_piece.build()
        export.export_glb(self.root, str(b))
        self.assertEqual(bake.sha256_file(a), bake.sha256_file(b))

    def test_ao_bake_masks_and_report(self):
        bake.setup_cycles(samples=4)
        img = bake.new_image("ao", 64, non_color=True)
        bake.bake_to(self.lods[0], "AO", img, margin=4)
        ao = bake.save_png(img, str(self.tmp / "ao.png"))
        bake.make_masks(64, 1.0, 0.5, 0.0, ao, str(self.tmp / "masks.png"))
        sig = bake.receiver_signature([self.lods[0]])
        rep = self.tmp / "bake-report.json"
        bake.write_bake_report(str(rep), receiverSignature=sig, device="CPU")
        bake.check_bake_cache(str(rep), sig)  # same receiver: accepted
        self.assertTrue((self.tmp / "masks.png").exists())

    def test_ao_bake_ignores_non_participating_meshes(self):
        occ = scene.link(bpy.data.objects.new("occluder", bpy.data.meshes.new("occluder")))
        occ.data.from_pydata([(-20, -20, 0.4), (20, -20, 0.4), (20, 20, 0.4), (-20, 20, 0.4)], [], [[0, 1, 2, 3]])
        hidden = scene.link(bpy.data.objects.new("prehidden", bpy.data.meshes.new("prehidden")))
        hidden.data.from_pydata([(-1, -1, 2), (1, -1, 2), (1, 1, 2)], [], [[0, 1, 2]])
        hidden.hide_render = True
        bake.setup_cycles(samples=8)
        img = bake.new_image("ao_m1", 32, non_color=True)
        bake.bake_to(self.lods[0], "AO", img, margin=2)
        size, px = png_rgba(bake.save_png(img, str(self.tmp / "ao_m1.png")))
        mean = float(px[inside_hex_mask(size), 0].mean())
        self.assertGreater(mean, 0.9, "AO darkened by meshes outside the bake")
        self.assertFalse(occ.hide_render)
        self.assertTrue(hidden.hide_render)
        for o in self.lods[1:]:
            self.assertFalse(o.hide_render)

    def test_bake_emit_input_rejects_duplicate_highs(self):
        img = bake.new_image("dup", 8, non_color=False)
        mat = self.lods[0].data.materials[0]
        before = len(mat.node_tree.nodes)
        # Python's own binding error also names 'highs'; the lib's message must say how to fix it.
        with self.assertRaisesRegex(TypeError, "highs is positional"):
            bake.bake_emit_input([], self.lods[0], "Base Color", img, highs=[], margin=4)
        self.assertEqual(len(mat.node_tree.nodes), before)

    def test_stale_bake_refused(self):
        sig = bake.receiver_signature([self.lods[0]])
        rep = self.tmp / "bake-report.json"
        bake.write_bake_report(str(rep), receiverSignature=sig, device="CPU")
        self.lods[0].data.vertices[0].co.x += 0.01
        with self.assertRaises(bake.StaleBakeError):
            bake.check_bake_cache(str(rep), bake.receiver_signature([self.lods[0]]))

    def test_texel_density_measured(self):
        d = uv.measure_texel_density(self.lods[0], 1024)
        self.assertAlmostEqual(d["median"], 1024 / 32.0, delta=1.0)  # planar_cap_uv: 32 m square

    def test_export_report_schema(self):
        rep = export.write_export_report(str(self.tmp / "export-report.json"), self.root, "fixture", __file__)
        self.assertEqual(set(rep), {"pieceId", "nodes", "lodTris", "sockets", "generator", "generatorDeps",
                                    "blender"})
        self.assertEqual(rep["generatorDeps"], [])
        self.assertEqual(set(rep["lodTris"]), LOD_MESHES)
        self.assertTrue(all(k[-1].isdigit() for k in rep["lodTris"]))
        sock = rep["sockets"][0]
        self.assertEqual(sock["name"], "socket-lane-0")
        self.assertAlmostEqual(sock["position"][0], hexgeo.APOTHEM, places=4)
        self.assertEqual(sock["extras"]["kind"], "road")
        self.assertEqual(rep["generator"]["sha256"], bake.sha256_file(__file__))
        self.assertFalse(pathlib.Path(rep["generator"]["path"]).is_absolute(), rep["generator"]["path"])
        self.assertEqual(rep["generator"]["path"], "scripts/blender/tests/test_bake_export.py")

    def test_generator_deps_lists_imported_sibling_helpers(self):
        kit = export.REPO / "scripts/blender/kit"
        road = kit / "_road.py"
        want = [{"path": "scripts/blender/kit/_road.py", "sha256": bake.sha256_file(str(road))}]
        self.assertEqual(export.generator_deps(str(kit / "road_straight_edge.py")), want)
        self.assertEqual(export.generator_deps(str(kit / "road_straight_center.py")), want)
        terrain = kit / "_terrain.py"
        self.assertEqual(export.generator_deps(str(kit / "meadow_base.py")),
                         [{"path": "scripts/blender/kit/_terrain.py", "sha256": bake.sha256_file(str(terrain))}])
        self.assertEqual(export.generator_deps(str(kit / "river_stones.py"))[0]["path"],
                         "scripts/blender/kit/_bank_props.py")
        # Transitive: a helper importing another helper records both, sorted, once each.
        with tempfile.TemporaryDirectory(dir=kit) as d:
            d = pathlib.Path(d)
            (d / "_a.py").write_text("import _b\n")
            (d / "_b.py").write_text("x = 1\n")
            (d / "gen.py").write_text("import os\nimport _a\nfrom _b import x\nimport _missing\n")
            got = [e["path"].rsplit("/", 1)[1] for e in export.generator_deps(str(d / "gen.py"))]
            self.assertEqual(got, ["_a.py", "_b.py"])

    def test_bake_report_with_generator_records_its_deps(self):  # provenance requires them
        path = self.tmp / "bake-report.json"
        bake.write_bake_report(str(path), generator="scripts/blender/kit/road_straight_edge.py")
        deps = json.loads(path.read_text())["generatorDeps"]
        self.assertEqual([d["path"] for d in deps], ["scripts/blender/kit/_road.py"])
        bake.write_bake_report(str(path), receiverSignature="x")
        self.assertNotIn("generatorDeps", json.loads(path.read_text()))

    def test_export_report_budget_class(self):  # owner ruling 2026-10-13: hero pieces mark their class
        path = self.tmp / "export-report.json"
        rep = export.write_export_report(str(path), self.root, "fixture", __file__, budget_class="hero")
        self.assertEqual(rep["budgetClass"], "hero")
        self.assertEqual(json.loads(path.read_text())["budgetClass"], "hero")
        self.assertNotIn("budgetClass", export.write_export_report(str(path), self.root, "fixture", __file__))
        with self.assertRaisesRegex(ValueError, "budget_class"):
            export.write_export_report(str(path), self.root, "fixture", __file__, budget_class="huge")


class LibSmoke(unittest.TestCase):
    """Functions no fixture path exercises; Tasks 8-10 depend on them."""

    def setUp(self):
        self.tmp = pathlib.Path(tempfile.mkdtemp())
        scene.reset_scene("smoke")

    def test_colliders_hull_and_box(self):
        bpy.ops.mesh.primitive_uv_sphere_add(radius=1.0, location=(0.0, 0.0, 2.0))
        src = bpy.context.active_object
        box = collider.collider_from(src, "box", "box_collider")
        hull = collider.collider_from(src, "hull", "hull_collider")
        self.assertEqual(len(box.data.polygons), 12)
        self.assertEqual(box["collider"], "box")
        self.assertEqual(hull["collider"], "hull")
        self.assertGreater(len(hull.data.polygons), 12)
        zs = [v.co.z for v in hull.data.vertices]
        self.assertAlmostEqual(min(zs), 1.0, places=4)  # world space: source location applied
        self.assertAlmostEqual(max(zs), 3.0, places=4)
        with self.assertRaises(ValueError):
            collider.collider_from(src, "sphere", "bad")

    def test_atlas_unwrap_in_unit_square(self):
        bpy.ops.mesh.primitive_cube_add(location=(0, 0, 0))
        a = bpy.context.active_object
        bpy.ops.mesh.primitive_cube_add(location=(3, 0, 0))
        b = bpy.context.active_object
        uv.atlas_unwrap([a, b])
        for o in (a, b):
            uvs = np.empty(len(o.data.loops) * 2, np.float64)
            o.data.uv_layers["UVMap"].data.foreach_get("uv", uvs)
            self.assertTrue(((uvs >= -1e-6) & (uvs <= 1 + 1e-6)).all(), o.name)
            self.assertGreater(np.ptp(uvs), 0.1)

    def test_tileable_set_and_pack_orm(self):
        bake.setup_cycles(samples=1)
        mat = bpy.data.materials.new("tile")
        nt = materials.ensure_nodes(mat)
        vec, w = materials.torus_coords(nt, 4.0)
        noise = nt.nodes.new("ShaderNodeTexNoise")
        noise.noise_dimensions = "4D"
        nt.links.new(vec, noise.inputs["Vector"])
        nt.links.new(w, noise.inputs["W"])
        p = next(n for n in nt.nodes if n.type == "BSDF_PRINCIPLED")
        nt.links.new(noise.outputs["Color"], p.inputs["Base Color"])
        files = bake.bake_tileable_set(mat, 16, str(self.tmp), "tset", metallic=0.25)
        self.assertEqual(set(files), {"albedo", "normal", "orm"})
        for f in files.values():
            self.assertTrue(pathlib.Path(f).exists(), f)
        self.assertFalse((self.tmp / "tset.rough.tmp.png").exists())
        size, orm = png_rgba(files["orm"])
        self.assertEqual(size, 16)
        self.assertAlmostEqual(float(orm[:, 0].mean()), 1.0, places=2)
        self.assertAlmostEqual(float(orm[:, 2].mean()), 0.25, delta=0.01)
        _, alb = png_rgba(files["albedo"])
        self.assertGreater(float(alb[:, 0].std()), 0.0)  # noise baked, not a flat colour
        out = bake.pack_orm(files["albedo"], files["orm"], files["orm"], str(self.tmp / "packed.orm.png"))
        _, packed = png_rgba(out)
        np.testing.assert_allclose(packed[:, 0], alb[:, 0], atol=1 / 255)
        np.testing.assert_allclose(packed[:, 1], orm[:, 0], atol=1 / 255)
        np.testing.assert_allclose(packed[:, 2], orm[:, 0], atol=1 / 255)
