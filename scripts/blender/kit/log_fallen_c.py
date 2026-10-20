"""log_fallen_c: an old hollow spruce log, about 3.2 m long and 0.55 m thick, crooked in plan, its top
heavily mossed, and its tip broken open into a wide, deep rotten mouth whose upper lip has fallen
away, so the dark hollow shows from above and from the side; a small flare with two snapped roots at
the butt and one short branch stub (refs: assets-source/refs/forest, "fallen log" and ref-1's "fallen
mossy log"). Same bark set and tint, moss and end-grain colours, atlas density and LOD scheme as
log_fallen; the pipeline is `_log.build`. Scripted route, prop kind (LOD0 <= 1,500 tris):
make kit-build PIECE=log_fallen_c.

It reads apart from log_fallen (long and straight, branch stubs along its top, a small hollow) and
log_fallen_b (short butt with a splintered solid break) by its bowed, C-shaped crook, a top mostly green
with moss, and the large dark opening at one end. Its radius is _log's atlas ceiling: the two
half-length body islands (2 x 2 pi r0 x 1.06 plus three gaps) fill the 3.71 m atlas at r0 = 0.265.

The rot inside the hollow: `_log.wood_high_material` ramps rot by object x from half_l inward, which
fits log_fallen's square-on mouth; this mouth's lower lip juts 0.12 m past half_l, so its sunlit
lower wall baked pale (friction.md). The wrapper below keeps that material and replaces only its rot
factor with a radial one: dark wherever the surface lies inside the hollow's radius (about 0.2 m
from the bent axis) near the tip, pale on the outer lip ring.

The sink is log_fallen's rule at this length: the forest floor relief drops at most about 0.11 m from
a 1.6 m half-length log's centre to its ends (log_fallen: 0.127 m at 1.8 m), so 0.13 m keeps both
ends in the litter. The collider reach stays under log_fallen's 1.9 m footprint, so the log group's
scatter footprint (its widest member) does not change."""
import dataclasses
import inspect
import math
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
import _log  # noqa: E402

D = math.radians
SPEC = _log.Spec(
    # bend: one C-shaped crook, 0.2 m off straight at each end, its slope 0 at both ends so the end
    # caps' planar islands (centred on the axis at +-half_l) hold the rings.
    half_l=1.5, r0=0.26, bury=0.13, taper=0.016, flare=(0.06, 0.4), lump=0.08, bend=(0.2, math.pi / 3.0),
    seed_off=(7.4, 3.3, -9.6),
    # Two snapped roots low on the butt, one short broken branch on the upper flank.
    stubs=[(-1.38, D(15), 0.5, 0.36, 0.08, 0.04), (-1.4, D(150), 0.45, 0.3, 0.075, 0.038),
           (-0.2, D(115), -0.5, 0.2, 0.05, 0.03)],
    lod1_stubs=[0, 1],
    # The broken tip: the upper lip has fallen away (the rim recedes 0.12 m on top, th = pi/2, and
    # juts 0.12 m below), plus a little jaggedness; a wide (0.76 R), deep (0.38 m) rotten hollow.
    rim=[(0.12, 1, math.pi), (0.035, 4, 0.7)], hollow=(0.76, 0.38),
    rings=(18, 8, 4, 140),
    # Moss over most of the top and down both flanks, in broad patches (log_fallen: blotches on the
    # top, log_fallen_b: streaks down one flank), and cushions on top.
    moss=_log.Moss(toward=(0.0, 0.0, 1.0), lo_hi=(-0.1, 0.35), scale=1.6, stretch=(0.6, 1.0, 1.0),
                   cut=(0.4, 0.55), top=(0.4, 0.8), cushion_scale=5.0, cushion_cut=(0.5, 0.62),
                   offset=(-4.2, 2.9, 6.1)))

# Radial rot (see the module docstring). Pinned: build() looks wood_high_material up as a module
# global and passes (name, spec); with no hollow it leaves the rot factor of its one RGBA mix into Base
# Color unlinked.
ROT_R = (0.225, 0.195)  # m from the axis: pale lip at and beyond the first, full rot inside the second
ROT_X = (SPEC.half_l - 0.9, SPEC.half_l - 0.8)  # only toward the tip: the butt cap and stubs stay pale
_wood = _log.wood_high_material
assert list(inspect.signature(_wood).parameters) == ["name", "spec"]
assert "wood_high_material(f\"{piece}_wood_high\", spec)" in inspect.getsource(_log.build)


def _wood_radial_rot(name, spec):
    m = _wood(name, dataclasses.replace(spec, hollow=None))
    nt = m.node_tree
    N, L = nt.nodes, nt.links
    bsdf = next(n for n in N if n.type == "BSDF_PRINCIPLED")
    mixes = [lk.from_node for lk in bsdf.inputs["Base Color"].links]
    assert len(mixes) == 1 and mixes[0].type == "MIX" and mixes[0].data_type == "RGBA", mixes
    fac = next(s for s in mixes[0].inputs if s.identifier == "Factor_Float")
    assert not fac.is_linked and fac.default_value == 0.0

    def op(kind, a, b):
        n = N.new("ShaderNodeMath")
        n.operation = kind
        for i, x in enumerate((a, b)):
            if isinstance(x, float):
                n.inputs[i].default_value = x
            else:
                L.new(x, n.inputs[i])
        return n.outputs[0]

    def ramp(src, a, b):
        r = N.new("ShaderNodeMapRange")
        r.inputs["From Min"].default_value, r.inputs["From Max"].default_value = a, b
        r.clamp = True
        L.new(src, r.inputs["Value"])
        return r.outputs["Result"]

    tc = N.new("ShaderNodeTexCoord")
    sep = N.new("ShaderNodeSeparateXYZ")
    L.new(tc.outputs["Object"], sep.inputs[0])
    x, y, z = sep.outputs["X"], sep.outputs["Y"], sep.outputs["Z"]
    # The bent axis, as _log.Log.axis: (x, bend_a sin(bend_f x), r0 - bury).
    ay = op("MULTIPLY", op("SINE", op("MULTIPLY", x, spec.bend[1]), 0.0), spec.bend[0])
    dy = op("SUBTRACT", y, ay)
    dz = op("SUBTRACT", z, spec.r0 - spec.bury)
    rad = op("SQRT", op("ADD", op("MULTIPLY", dy, dy), op("MULTIPLY", dz, dz)), 0.0)
    L.new(op("MULTIPLY", ramp(rad, *ROT_R), ramp(x, *ROT_X)), fac)
    return m


_log.wood_high_material = _wood_radial_rot

# The hollow's floor island. _log lays it out 0.3 m square, enough for log_fallen's 0.6 R mouth; LOD2
# has no wall segments, so its floor is the mouth ring itself, here 0.76 R (about 0.42 m across with
# the lumps). Widen the island in place (row B has room to its right) and re-check every gap. Pinned:
# build() constructs `Log(spec)` through the module global, and the floor island is the rect that
# follows the wall in `rects`.
FLOOR_M = 0.46
assert "log = Log(spec)" in inspect.getsource(_log.build)
_Log = _log.Log


class _WideFloorLog(_Log):
    def __init__(self, s):
        super().__init__(s)
        i = self.rects.index(self.inner_floor)
        assert self.rects[i - 1] is self.inner_wall
        u0, v0 = self.inner_floor[0] / _log.K, self.inner_floor[1] / _log.K
        self.inner_floor = self.rect(u0, v0, FLOOR_M, FLOOR_M)
        self.rects[i] = self.inner_floor
        g = _log.GAP * _log.K * 0.99
        for j, a in enumerate(self.rects):
            for b in self.rects[j + 1:]:
                assert (a[2] + g <= b[0] or b[2] + g <= a[0] or a[3] + g <= b[1] or b[3] + g <= a[1]), \
                    ("uv rects closer than GAP", a, b)


_log.Log = _WideFloorLog

_log.build("log_fallen_c", pathlib.Path(sys.argv[sys.argv.index("--") + 1]), __file__, SPEC,
           length=(2.9, 3.6), top=(0.4, 1.0))
