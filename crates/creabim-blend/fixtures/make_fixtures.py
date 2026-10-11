# SPDX-License-Identifier: GPL-3.0-or-later
"""Build the .blend test fixtures for creabim-blend.

Runs either inside Blender or with the `bpy` module from PyPI:

    blender -b --factory-startup --python make_fixtures.py -- OUT_DIR
    python make_fixtures.py OUT_DIR

Writes `scene_<major>_<minor>.blend` (uncompressed) and
`scene_<major>_<minor>_<gz|zstd>.blend` (compressed) into OUT_DIR. The
scene is deliberately tiny and starts from empty factory settings so the
files stay small. Keep it in sync with `tests/fixtures.rs`. It also writes
`expected_worlds.txt`, Blender's own world matrices for every object.

The committed files came from: Blender 2.82 and 3.0.1 (Ubuntu focal and
jammy packages, run in a debootstrap chroot), 4.0.2 (Ubuntu noble), and
the PyPI `bpy` 4.2 and 5.2 wheels.
"""

import math
import os
import sys

import bpy
from mathutils import Matrix


def out_dir():
    args = sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else sys.argv[1:]
    return os.path.abspath(args[0] if args else os.path.dirname(__file__))


def material(name, rgba):
    mat = bpy.data.materials.new(name)
    mat.diffuse_color = rgba
    return mat


def mesh_object(name, verts, faces, collections):
    mesh = bpy.data.meshes.new(name + " Mesh")
    mesh.from_pydata(verts, [], faces)
    mesh.update()
    obj = bpy.data.objects.new(name, mesh)
    for coll in collections:
        coll.objects.link(obj)
    return obj


def cube(size):
    h = size / 2.0
    verts = [
        (-h, -h, -h),
        (h, -h, -h),
        (h, h, -h),
        (-h, h, -h),
        (-h, -h, h),
        (h, -h, h),
        (h, h, h),
        (-h, h, h),
    ]
    faces = [
        (0, 3, 2, 1),
        (4, 5, 6, 7),
        (0, 1, 5, 4),
        (1, 2, 6, 5),
        (2, 3, 7, 6),
        (3, 0, 4, 7),
    ]
    return verts, faces


def hex_prism(radius, height):
    bottom = [
        (radius * math.cos(i * math.pi / 3), radius * math.sin(i * math.pi / 3), 0.0)
        for i in range(6)
    ]
    top = [(x, y, height) for (x, y, _) in bottom]
    faces = [tuple(reversed(range(6))), tuple(range(6, 12))]
    for i in range(6):
        j = (i + 1) % 6
        faces.append((i, j, j + 6, i + 6))
    return bottom + top, faces


def build_scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    scene = bpy.context.scene
    scene.unit_settings.system = "METRIC"
    scene.unit_settings.scale_length = 0.01

    structure = bpy.data.collections.new("Structure")
    furniture = bpy.data.collections.new("Furniture")
    scene.collection.children.link(structure)
    scene.collection.children.link(furniture)

    concrete = material("Concrete", (0.8, 0.8, 0.75, 1.0))
    wood = material("Wood", (0.6, 0.4, 0.2, 0.5))

    verts, faces = cube(1.0)
    wall = mesh_object("Wall Block", verts, faces, [structure])
    wall.data.materials.append(concrete)
    wall.scale = (4.0, 0.2, 3.0)
    wall.rotation_euler = (0.0, 0.0, math.radians(30.0))
    wall.location = (2.0, 1.0, 1.5)
    wall["IfcClass"] = "IfcWall"
    wall["FireRating"] = 60

    plane = mesh_object(
        "Floor Plane",
        [(-5, -5, 0), (5, -5, 0), (5, 5, 0), (-5, 5, 0)],
        [(0, 1, 2, 3)],
        [structure],
    )
    plane.data.materials.append(wood)

    verts, faces = hex_prism(1.0, 0.5)
    prism = mesh_object("Hex Prism", verts, faces, [furniture])
    prism.data.materials.append(concrete)
    prism.data.materials.append(wood)
    # The top hexagon uses the second slot.
    prism.data.polygons[1].material_index = 1
    prism.location = (5.0, 5.0, 0.0)
    prism.rotation_euler = (math.radians(90.0), 0.0, 0.0)

    verts, faces = cube(0.5)
    shade = mesh_object("Lamp Shade", verts, faces, [furniture, structure])
    shade.parent = prism
    shade.location = (0.0, 0.0, 2.0)
    # A non-XYZ Euler order and a non-identity parent inverse (what Ctrl+P
    # in the UI sets).
    shade.rotation_mode = "ZXY"
    shade.rotation_euler = (math.radians(10.0), math.radians(20.0), math.radians(30.0))
    bpy.context.view_layer.update()
    shade.matrix_parent_inverse = Matrix.Translation((0.0, 1.0, 0.0)) @ Matrix.Rotation(
        math.radians(15.0), 4, "Z"
    )

    marker = bpy.data.objects.new("Marker", None)
    furniture.objects.link(marker)
    marker.location = (-3.0, 0.0, 0.0)
    marker.delta_location = (0.0, 0.5, 0.0)
    marker.rotation_mode = "QUATERNION"
    marker.rotation_quaternion = (0.9, 0.1, 0.3, 0.2)
    marker.scale = (1.0, 2.0, 1.0)
    marker.delta_scale = (1.5, 1.0, 1.0)
    marker.hide_viewport = True

    sun = bpy.data.objects.new("Sun", bpy.data.lights.new("Sun", "SUN"))
    scene.collection.objects.link(sun)
    sun.location = (0.0, 0.0, 10.0)

    bpy.context.view_layer.update()
    # Hidden with the eye icon, which lives on the view layer's Base.
    sun.hide_set(True)
    bpy.context.view_layer.update()


def shrink():
    """Drop what makes an empty file big: the thumbnail and all but the
    active workspace (UI layout data)."""
    paths = bpy.context.preferences.filepaths
    if hasattr(paths, "file_preview_type"):
        paths.file_preview_type = "NONE"
    if hasattr(paths, "use_save_preview_images"):
        paths.use_save_preview_images = False
    keep = bpy.context.window.workspace if bpy.context.window else None
    extra = [ws for ws in bpy.data.workspaces if ws != keep]
    if keep is None and extra:
        extra = extra[1:]
    bpy.data.batch_remove(extra)


def save(path, compress):
    bpy.ops.wm.save_as_mainfile(filepath=path, compress=compress, check_existing=False)
    print("wrote", path, os.path.getsize(path), "bytes")


def expected_worlds(path):
    """Writes each object's world matrix as Blender computes it, for the
    tests. Hidden objects are not evaluated by the depsgraph, so the matrix
    is rebuilt from `matrix_basis` (which includes delta transforms)."""

    def world(obj):
        if obj.parent is None:
            return obj.matrix_basis
        return world(obj.parent) @ obj.matrix_parent_inverse @ obj.matrix_basis

    lines = []
    for obj in sorted(bpy.data.objects, key=lambda o: o.name):
        m = world(obj)
        assert obj.hide_viewport or all(
            abs(a - b) < 1e-5 for ra, rb in zip(m, obj.matrix_world) for a, b in zip(ra, rb)
        ), obj.name
        # Column-major, like `BlendObject::world`.
        cols = [[m[r][c] for r in range(4)] for c in range(4)]
        lines.append("%s|%s" % (obj.name, " ".join("%.9g" % v for col in cols for v in col)))
    with open(path, "w") as f:
        f.write("\n".join(lines) + "\n")


def main():
    build_scene()
    shrink()
    major, minor = bpy.app.version[:2]
    tag = "%d_%d" % (major, minor)
    packed = "zstd" if major >= 3 else "gz"
    directory = out_dir()
    save(os.path.join(directory, "scene_%s.blend" % tag), False)
    save(os.path.join(directory, "scene_%s_%s.blend" % (tag, packed)), True)
    expected_worlds(os.path.join(directory, "expected_worlds.txt"))


main()
