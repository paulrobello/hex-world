"""Open-tunnel extension of the road portal (P4 open tunnels, owner 2026-11-14; spec
docs/superpowers/specs/2026-11-14-open-tunnels-design.md): the lining runs from the back plane X_B to the
cell edge X_E, where road_tunnel_bore's lining starts on the same section, and the stone back face at X_E is
cut round the opening so the bore shows through it. The back wall stays its own slot (`<piece>_back`) at
X_B, plus a patch of it behind the opening at X_E, so a lone portal looks as it did from the front and stays
closed from behind; the runtime hides the `_back` slot while the tunnel is complete.

The approved helper _tunnel_portal.py is never edited: this file wraps two of its module-level names
(`build_lod`, `compose_maps`), so `_tunnel_portal.build_piece` runs unchanged. The exterior is untouched:
face, ring, keystone, jambs, coping, turf, ferns and lanterns come out of the helper as before, and the
lining keeps its UVs over the part in front of the back wall (its X_B end moves to X_E with v extrapolated
along the same line).

Tris: the X_E stone face drops from panel cells over the whole wall end (56 at LOD0) to three cut regions
mapped at OPEN_SCALE (the opening's N arch segments + 1, plus 3 a side) and a two-triangle back patch."""
from __future__ import annotations

import bmesh
from mathutils import Vector

import _tunnel_portal as tp

# The cut back face maps the panel island at 1 / OPEN_SCALE (the opening's 7.8 m span fills the panel's
# 2.4 m width at 3.25), so each region is one island cell: the face is hidden behind the cover cell or
# the bore and seen only from behind a lone portal on open ground.
OPEN_SCALE = 3.25
# The back patch: a back-slot quad just behind the X_E face, a little larger than the opening.
PATCH_S, PATCH_H0, PATCH_H1, PATCH_DX = tp.A + 0.05, -tp.BURY - 0.05, tp.CROWN + 0.05, 0.01
# Masks rows past the lining island's top edge filled with its last row: the extended lining samples up to
# that edge (v = 1.25 at LOD0/1), so mip and bilinear taps there read lining values, not the atlas ground.
LINING_EXT_PX = 16

_base_build_lod = tp.build_lod
_base_compose_maps = tp.compose_maps


def _free(isl, box):
    x0, y0, x1, y1 = box
    for i in isl.values():
        a0, b0, a1, b1 = i.px_box()
        if a0 < x1 and x0 < a1 and b0 < y1 and y0 < b1:
            return False
    return True


def compose_maps(isl, baked, spec):
    alb, nrm, orm, msk = _base_compose_maps(isl, baked, spec)
    x0, _, x1, y1 = isl["lining"].px_box()
    box = (x0, y1, x1, y1 + LINING_EXT_PX)
    assert _free(isl, box) and y1 + LINING_EXT_PX <= tp.ATLAS, f"the lining's mask extension {box} overlaps an island"
    msk[y1:y1 + LINING_EXT_PX, x0:x1] = msk[y1 - 1:y1, x0:x1]
    return alb, nrm, orm, msk


def _face(bm, uvl, pts, uvs, slot):
    f = bm.faces.new([bm.verts.new(p) for p in pts])
    f.material_index = tp.SLOTS.index(slot)
    for loop, uv in zip(f.loops, uvs):
        loop[uvl].uv = uv
    f.normal_update()
    if f.normal.dot(Vector((1.0, 0.0, 0.0))) < 0:
        f.normal_flip()
    f.smooth = False
    return f


def build_lod(spec, lod, isl, mats):
    o = _base_build_lod(spec, lod, isl, mats)
    d = tp.LOD[lod]
    X_B, X_E, A, J, W, TOP, BURY = tp.X_B, tp.X_E, tp.A, tp.J, tp.W, tp.TOP, tp.BURY
    xl = tp.X_F + tp.SOFFIT + (tp.BARREL if lod < 2 else 0.0)
    v_end = (X_E - xl) / (X_B - xl)
    li, pi_ = isl["lining"], isl["panel"]
    i_lin, i_stone = tp.SLOTS.index("lining"), tp.SLOTS.index("stone")
    bm = bmesh.new()
    bm.from_mesh(o.data)
    uvl = bm.loops.layers.uv["UVMap"]
    tris0 = len(bm.faces)

    # 1. The lining's X_B end to the cell edge, v extrapolated on its line.
    v1, ve = li.uv(0.0, 1.0)[1], li.uv(0.0, v_end)[1]
    ends = set()
    for f in bm.faces:
        if f.material_index != i_lin:
            continue
        for loop in f.loops:
            if abs(loop.vert.co.x - X_B) < 1e-5:
                assert abs(loop[uvl].uv.y - v1) < 1e-6, (lod, loop[uvl].uv.y, v1)
                loop[uvl].uv.y = ve
                ends.add(loop.vert)
    assert ends, "no lining vertex on the back plane"
    for v in ends:
        v.co.x = X_E

    # 2. The X_E stone face (panel cells over the whole wall end) out.
    back = [f for f in bm.faces if f.material_index == i_stone and all(abs(v.co.x - X_E) < 1e-5 for v in f.verts)]
    assert back, "no stone face on X_E"
    bmesh.ops.delete(bm, geom=back, context="FACES")

    # 3. The cut face: a side region each side of the opening and the spandrel over it, seen from +x
    # (u to the viewer's right is +s), each one panel cell at OPEN_SCALE from its region's corner.
    sc = OPEN_SCALE

    def stone(pts, s0):
        uvs = [pi_.uv((s - s0) / sc, (h + BURY) / sc) for s, h in pts]
        assert all(0.0 <= (s - s0) / sc <= tp.PW + 1e-6 and 0.0 <= (h + BURY) / sc <= tp.PH + 1e-6 for s, h in pts)
        return _face(bm, uvl, [(X_E, s, h) for s, h in pts], uvs, "stone")

    for side in (1, -1):
        a0, a1 = (A, W) if side > 0 else (-W, -A)
        foot, top, edge = (side * W, -BURY), (side * W, TOP), (side * A, TOP)
        spring, low = (side * A, J), (side * A, -BURY)
        for tri in ((low, foot, spring), (foot, top, spring), (top, edge, spring)):
            stone(list(tri), a0)
    pts = [tp.intrados(t) for t in tp.ellipse_divisions(d["segs"] * d["sub"])]
    right = [p for p in pts if p[0] > 1e-9]
    mid = [(0.0, p[1]) for p in pts if abs(p[0]) <= 1e-9]
    left = [p for p in pts if p[0] < -1e-9]
    tr, tl = (A, TOP), (-A, TOP)
    for chain, corner in ((right + mid, tr), (mid + left, tl)):
        for p, q in zip(chain, chain[1:]):
            stone([corner, p, q], -A)
    if mid:
        stone([tr, mid[0], tl], -A)
    else:
        stone([tr, right[-1], left[0]], -A)
        stone([tr, left[0], tl], -A)

    # 4. The back patch behind the opening (back slot, hidden with the X_B back wall when open).
    cuv = isl["back"].uv(0.5, 0.5)
    x = X_E - PATCH_DX
    _face(bm, uvl, [(x, -PATCH_S, PATCH_H0), (x, PATCH_S, PATCH_H0), (x, PATCH_S, PATCH_H1), (x, -PATCH_S, PATCH_H1)],
          [cuv] * 4, "back")

    bmesh.ops.triangulate(bm, faces=[f for f in bm.faces if len(f.verts) > 3])
    tris1 = len(bm.faces)
    bm.to_mesh(o.data)
    bm.free()
    for p in o.data.polygons:
        p.use_smooth = False
    o.data.update()
    print(f"{spec['piece']}: open lod{lod}: lining to x {X_E:.4f} (v {v_end:.4f}), X_E face {len(back)} tris out, "
          f"tris {tris0} -> {tris1} ({tris1 - tris0:+d})")
    return o


tp.build_lod = build_lod
tp.compose_maps = compose_maps
