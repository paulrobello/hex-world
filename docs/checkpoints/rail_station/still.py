"""Textured headless EEVEE still of rail_station_platform and rail_station_shelter beside
rail_straight_center on a meadow, with a carriage at the platform (evidence only: Blender's AgX is not Bevy's tonemapper). Reuses the train
review's slot binding (docs/checkpoints/train/still.py `place`): tileable slots box-projected at the
set's tileM times the manifest colour, colour slots at their manifest colour, masks.A on UV0.
Usage: Blender -b --factory-startup --python-exit-code 1 -P docs/checkpoints/rail_station/still.py -- <out.png>"""
import math
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
out = sys.argv[sys.argv.index("--") + 1]
# The train still script builds the scene, sun and camera at import from its argv: hand it a view
# and output, then add the station and re-aim the camera.
sys.argv = [sys.argv[0], "--", out, "carriage"]
sys.path.insert(0, str(ROOT / "docs/checkpoints/train"))
import bpy  # noqa: E402
from mathutils import Vector  # noqa: E402

import still as train_still  # noqa: E402

train_still.place("rail_station_platform")
# The shelter at its pivot on the paving (_station.py SHELTER_AT, H), in Blender coordinates.
train_still.place("rail_station_shelter", (-0.3, 5.0, 1.38))
cam = bpy.context.scene.camera
loc, tgt = Vector((11.0, -12.0, 6.5)), Vector((-1.0, 3.5, 2.0))
cam.location = loc
cam.rotation_euler = (tgt - loc).to_track_quat("-Z", "Y").to_euler()
cam.data.lens = 28
bpy.context.scene.render.filepath = out
bpy.ops.render.render(write_still=True)
print("station still written", out, math.pi)
