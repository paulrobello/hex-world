"""Pre-script for docs/checkpoints/road_tunnel_portal/eevee_still.py that culls back faces on the portal's slots
as the runtime does (kit_material.rs: StandardMaterial's default back-face culling; only alpha-cutoff slots,
the ferns, are double-sided). The portal still turns culling off, so through the 15 mm voussoir joints it
shows the back side of whatever sits on the X_E plane (the stone back face before, the open build's back
patch after), which the game never draws. Used for the closed-exterior diff (exterior_diff.py):

  "$BLENDER" -b --factory-startup --python-exit-code 1 -P docs/checkpoints/road_tunnel_bore/cull_runtime.py \
      -P docs/checkpoints/road_tunnel_portal/eevee_still.py -- <out_dir> three_quarter,front,eye"""
import bpy

P = "road_tunnel_portal"


@bpy.app.handlers.persistent
def _cull(*_):
    for m in bpy.data.materials:
        if m.name.startswith(P) and "fern" not in m.name:
            m.use_backface_culling = True


bpy.app.handlers.render_pre.append(_cull)
