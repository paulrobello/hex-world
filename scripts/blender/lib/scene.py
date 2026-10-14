import bpy


def reset_scene(name):
    """Empty factory scene in metric units; no .blend1 backups (the-cube lib.py:19-26)."""
    bpy.ops.wm.read_factory_settings(use_empty=True)
    s = bpy.context.scene
    s.name = name
    s.unit_settings.system = "METRIC"
    s.unit_settings.scale_length = 1.0
    bpy.context.preferences.filepaths.save_version = 0
    return s


def link(obj, collection=None):
    (collection or bpy.context.scene.collection).objects.link(obj)
    return obj


def empty(name, loc=(0.0, 0.0, 0.0), parent=None):
    e = bpy.data.objects.new(name, None)
    e.location = loc
    e.empty_display_size = 0.5
    link(e)
    if parent is not None:
        e.parent = parent
    return e


def select_only(objs, active=None):
    for o in bpy.context.view_layer.objects:
        o.select_set(False)
    for o in objs:
        o.select_set(True)
    bpy.context.view_layer.objects.active = active or (objs[0] if objs else None)


def apply_transforms(obj):
    """Apply before joining (vault Patterns/blender-transform-identity-in-automated-glb-builds.md)."""
    select_only([obj])
    bpy.ops.object.transform_apply(location=True, rotation=True, scale=True)
