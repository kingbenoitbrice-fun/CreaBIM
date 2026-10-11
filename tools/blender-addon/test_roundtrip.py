# SPDX-License-Identifier: GPL-3.0-or-later
"""Round trip through Blender, run with Blender's Python (or the bpy wheel):

    python test_roundtrip.py HOUSE.glb OUT.blend

Loads the CreaBIM export with the add-on, makes the first wall 1 m longer,
adds a box marked as a slab, and saves OUT.blend for `creabim link`.
"""
import os
import sys

import bpy

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import creabim_link  # noqa: E402

glb, out = sys.argv[-2], sys.argv[-1]
bpy.ops.wm.read_factory_settings(use_empty=True)
creabim_link.register()
bpy.context.scene.creabim_glb = glb
added, updated, removed = creabim_link.load_model(bpy.context, glb)
print("loaded", added, updated, removed)
assert added > 10 and updated == 0
# loading again updates in place
added2, updated2, _ = creabim_link.load_model(bpy.context, glb)
assert added2 == 0 and updated2 == added, (added2, updated2)

walls = [o for o in bpy.data.objects if o.get("creabim_kind") == "wall"]
assert walls, [dict(o.items()) for o in bpy.data.objects][:3]
wall = sorted(walls, key=lambda o: o.name)[0]
print("stretching", wall.name, len(wall.data.polygons), "faces")
# move the wall's +x end out by a metre
xs = [v.co.x for v in wall.data.vertices]
hi = max(xs)
for v in wall.data.vertices:
    if abs(v.co.x - hi) < 1e-4:
        v.co.x += 1.0

bpy.ops.mesh.primitive_cube_add(size=1.0, location=(5.0, -4.0, -0.1))
deck = bpy.context.active_object
deck.name = "Deck"
deck.scale = (4.0, 2.0, 0.2)
bpy.ops.object.transform_apply(scale=True)
deck["creabim_kind"] = "slab"
bpy.ops.wm.save_as_mainfile(filepath=out)
print("wrote", out, "stretched", wall.name)
