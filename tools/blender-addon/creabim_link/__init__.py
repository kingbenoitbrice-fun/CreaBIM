# SPDX-License-Identifier: GPL-3.0-or-later
"""CreaBIM Link: work on a CreaBIM project from Blender.

CreaBIM reads .blend files directly and reloads them when they are saved,
so the round trip is:

1. In CreaBIM: File > Export glTF (.glb) for Blender.
2. Here: CreaBIM panel (N sidebar) > pick that .glb > Load CreaBIM model.
   Every element comes in as an object that remembers its CreaBIM id. With
   "Follow changes" on, saving the .glb again in CreaBIM updates them here.
3. Model anything. Mark objects as Wall, Slab, Door, Window or any IFC class
   with the buttons, or leave them as free shapes.
4. Save the .blend (Ctrl+S) and link it in CreaBIM once
   (File > Import and link). From then on every save updates CreaBIM:
   edited walls stay walls, new objects come in, deleted ones go.
"""

bl_info = {
    "name": "CreaBIM Link",
    "author": "CreaBIM contributors",
    "version": (0, 1, 0),
    "blender": (4, 2, 0),
    "location": "3D Viewport > Sidebar > CreaBIM",
    "description": "Load CreaBIM models, follow their changes, and mark objects as walls, slabs, doors and windows for CreaBIM",
    "category": "Import-Export",
}

import os

import bmesh
import bpy
from bpy.props import BoolProperty, StringProperty

COLLECTION = "CreaBIM"
KINDS = [
    ("wall", "Wall"),
    ("slab", "Slab"),
    ("door", "Door"),
    ("window", "Window"),
    ("IfcColumn", "Column"),
    ("IfcBeam", "Beam"),
    ("IfcRoof", "Roof"),
    ("IfcStair", "Stair"),
    ("IfcFurniture", "Furniture"),
]


def _collection(scene):
    c = bpy.data.collections.get(COLLECTION)
    if c is None:
        c = bpy.data.collections.new(COLLECTION)
    if c.name not in scene.collection.children:
        scene.collection.children.link(c)
    return c


def _quads(mesh):
    """glTF stores triangles; join them back into the faces they came from."""
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bmesh.ops.join_triangles(
        bm, faces=bm.faces, angle_face_threshold=0.01, angle_shape_threshold=3.15
    )
    bm.to_mesh(mesh)
    bm.free()


def load_model(context, path):
    """Load or refresh the CreaBIM model in `path`. Returns (added, updated, removed)."""
    scene = context.scene
    target = _collection(scene)
    existing = {
        o["creabim_id"]: o
        for o in target.all_objects
        if "creabim_id" in o.keys()
    }
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=path)
    new_objects = [o for o in bpy.data.objects if o not in before]
    added = updated = 0
    seen = set()
    for o in new_objects:
        cid = o.get("creabim_id")
        if cid is None or o.type != "MESH":
            bpy.data.objects.remove(o)
            continue
        seen.add(cid)
        _quads(o.data)
        old = existing.get(cid)
        if old is not None:
            # keep the object (and anything done to it here), take the shape
            old_mesh = old.data
            old.data = o.data
            old.data.name = old.name
            old.matrix_world = o.matrix_world
            if old_mesh.users == 0:
                bpy.data.meshes.remove(old_mesh)
            bpy.data.objects.remove(o)
            updated += 1
        else:
            for c in list(o.users_collection):
                c.objects.unlink(o)
            target.objects.link(o)
            added += 1
    removed = 0
    for cid, o in existing.items():
        if cid not in seen:
            bpy.data.objects.remove(o)
            removed += 1
    scene.creabim_mtime = os.path.getmtime(path)
    return added, updated, removed


class CREABIM_OT_load(bpy.types.Operator):
    bl_idname = "creabim.load"
    bl_label = "Load CreaBIM model"
    bl_description = "Bring in (or refresh) the model exported from CreaBIM"

    def execute(self, context):
        path = bpy.path.abspath(context.scene.creabim_glb)
        if not os.path.isfile(path):
            self.report({"ERROR"}, "Pick the .glb exported from CreaBIM first")
            return {"CANCELLED"}
        a, u, r = load_model(context, path)
        self.report({"INFO"}, f"CreaBIM: {a} added, {u} updated, {r} removed")
        return {"FINISHED"}


class CREABIM_OT_mark(bpy.types.Operator):
    bl_idname = "creabim.mark"
    bl_label = "Mark for CreaBIM"
    bl_description = "Tell CreaBIM what the selected objects are"
    kind: StringProperty()

    def execute(self, context):
        for o in context.selected_objects:
            if self.kind:
                o["creabim_kind"] = self.kind
            elif "creabim_kind" in o.keys():
                del o["creabim_kind"]
        return {"FINISHED"}


class CREABIM_OT_save(bpy.types.Operator):
    bl_idname = "creabim.save"
    bl_label = "Save for CreaBIM"
    bl_description = "Save this .blend; CreaBIM reloads it if it is linked"

    def execute(self, context):
        if not bpy.data.filepath:
            self.report({"ERROR"}, "Save the .blend once with File > Save As")
            return {"CANCELLED"}
        bpy.ops.wm.save_mainfile()
        self.report({"INFO"}, "Saved. CreaBIM will pick up the changes.")
        return {"FINISHED"}


class CREABIM_PT_panel(bpy.types.Panel):
    bl_label = "CreaBIM"
    bl_space_type = "VIEW_3D"
    bl_region_type = "UI"
    bl_category = "CreaBIM"

    def draw(self, context):
        layout = self.layout
        scene = context.scene
        layout.prop(scene, "creabim_glb", text="")
        row = layout.row()
        row.operator("creabim.load", icon="IMPORT")
        layout.prop(scene, "creabim_follow")
        layout.separator()
        o = context.active_object
        if o is not None:
            box = layout.box()
            kind = o.get("creabim_kind") or o.get("ifc_class") or "free shape"
            box.label(text=f"{o.name}: {kind}")
            if "creabim_id" in o.keys():
                box.label(text="From CreaBIM", icon="LINKED")
            grid = box.grid_flow(columns=2, align=True)
            for k, label in KINDS:
                grid.operator("creabim.mark", text=label).kind = k
            box.operator("creabim.mark", text="Free shape").kind = ""
        layout.separator()
        layout.operator("creabim.save", icon="FILE_TICK")


def _follow():
    """Reload the model when CreaBIM writes the .glb again."""
    try:
        scene = bpy.context.scene
    except AttributeError:
        return 1.0
    if scene is None or not scene.creabim_follow or not scene.creabim_glb:
        return 1.0
    path = bpy.path.abspath(scene.creabim_glb)
    if os.path.isfile(path) and os.path.getmtime(path) != scene.creabim_mtime:
        try:
            load_model(bpy.context, path)
        except RuntimeError as e:  # importer refused the file mid-write
            print("CreaBIM Link:", e)
    return 1.0


CLASSES = (CREABIM_OT_load, CREABIM_OT_mark, CREABIM_OT_save, CREABIM_PT_panel)


def register():
    for c in CLASSES:
        bpy.utils.register_class(c)
    bpy.types.Scene.creabim_glb = StringProperty(
        name="CreaBIM model", subtype="FILE_PATH", description="The .glb exported from CreaBIM"
    )
    bpy.types.Scene.creabim_follow = BoolProperty(
        name="Follow changes",
        description="Refresh when CreaBIM exports the model again",
        default=True,
    )
    bpy.types.Scene.creabim_mtime = bpy.props.FloatProperty(default=0.0)
    if not bpy.app.timers.is_registered(_follow):
        bpy.app.timers.register(_follow, first_interval=1.0, persistent=True)


def unregister():
    if bpy.app.timers.is_registered(_follow):
        bpy.app.timers.unregister(_follow)
    for c in reversed(CLASSES):
        bpy.utils.unregister_class(c)
    del bpy.types.Scene.creabim_glb
    del bpy.types.Scene.creabim_follow
    del bpy.types.Scene.creabim_mtime
