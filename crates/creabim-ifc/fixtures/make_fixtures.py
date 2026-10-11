"""Write the IfcOpenShell-authored IFC fixtures the importer tests read.

The model is authored in millimetres, the way most tools write IFC, with
two storeys (0 and 3200), a wall with a door and a window in openings, a
wall on the upper storey typed by an IfcWallType, a slab, an extruded
column and a tessellated proxy. It is written in IFC4, IFC2X3 and IFC4X3.

    python3 -m pip install ifcopenshell
    python3 crates/creabim-ifc/fixtures/make_fixtures.py

The tests check positions against the numbers below, so change both together.
"""

import os
import sys

import numpy as np
import ifcopenshell
import ifcopenshell.api.aggregate
import ifcopenshell.api.context
import ifcopenshell.api.feature
import ifcopenshell.api.geometry
import ifcopenshell.api.material
import ifcopenshell.api.owner.settings
import ifcopenshell.api.project
import ifcopenshell.api.root
import ifcopenshell.api.spatial
import ifcopenshell.api.type
import ifcopenshell.api.unit

HERE = os.path.dirname(os.path.abspath(__file__))


def placement(x, y, z, angle=0.0):
    """A 4x4 matrix in metres: a turn about z, then a move."""
    c, s = np.cos(angle), np.sin(angle)
    m = np.eye(4)
    m[:3, :3] = [[c, -s, 0], [s, c, 0], [0, 0, 1]]
    m[:3, 3] = [x, y, z]
    return m


def build(schema: str) -> ifcopenshell.file:
    f = ifcopenshell.api.project.create_file(version=schema)
    if schema == "IFC2X3":
        # IFC2X3 requires owner history on every rooted entity
        person = f.createIfcPerson(FamilyName="Fixture")
        org = f.createIfcOrganization(Name="CreaBIM")
        user = f.createIfcPersonAndOrganization(person, org)
        app = f.createIfcApplication(org, "0.1", "make_fixtures.py", "make_fixtures")
        ifcopenshell.api.owner.settings.get_user = lambda f: user
        ifcopenshell.api.owner.settings.get_application = lambda f: app
    run = ifcopenshell.api.root.create_entity
    project = run(f, ifc_class="IfcProject", name="Fixture")
    length = ifcopenshell.api.unit.add_si_unit(f, unit_type="LENGTHUNIT", prefix="MILLI")
    ifcopenshell.api.unit.assign_unit(f, units=[length])
    model = ifcopenshell.api.context.add_context(f, context_type="Model")
    body = ifcopenshell.api.context.add_context(
        f, context_type="Model", context_identifier="Body", target_view="MODEL_VIEW", parent=model
    )
    axis_ctx = ifcopenshell.api.context.add_context(
        f, context_type="Model", context_identifier="Axis", target_view="GRAPH_VIEW", parent=model
    )

    site = run(f, ifc_class="IfcSite", name="Site")
    building = run(f, ifc_class="IfcBuilding", name="Building")
    gf = run(f, ifc_class="IfcBuildingStorey", name="Ground")
    ff = run(f, ifc_class="IfcBuildingStorey", name="Upper")
    ifcopenshell.api.aggregate.assign_object(f, relating_object=project, products=[site])
    ifcopenshell.api.aggregate.assign_object(f, relating_object=site, products=[building])
    ifcopenshell.api.aggregate.assign_object(f, relating_object=building, products=[gf, ff])
    for el, z in [(site, 0), (building, 0), (gf, 0), (ff, 3.2)]:
        ifcopenshell.api.geometry.edit_object_placement(f, product=el, matrix=placement(0, 0, z))
    ff.Elevation = 3200.0
    gf.Elevation = 0.0

    geo = ifcopenshell.api.geometry

    def contain(storey, *products):
        ifcopenshell.api.spatial.assign_container(f, relating_structure=storey, products=list(products))

    # materials: a layer set on wall A, a typed layer set on wall B
    category = (lambda c: {}) if schema == "IFC2X3" else (lambda c: {"category": c})
    brick = ifcopenshell.api.material.add_material(f, name="Brick", **category("brick"))
    concrete = ifcopenshell.api.material.add_material(f, name="Concrete", **category("concrete"))

    # Wall A: 5 m along +x from (1, 2), 200 thick on the +y side of its axis
    wall = run(f, ifc_class="IfcWall", name="Wall A")
    geo.edit_object_placement(f, product=wall, matrix=placement(1, 2, 0))
    rep = geo.add_wall_representation(f, context=body, length=5.0, height=3.0, thickness=0.2)
    geo.assign_representation(f, product=wall, representation=rep)
    axis = geo.add_axis_representation(f, context=axis_ctx, axis=[(0.0, 0.0), (5.0, 0.0)])
    geo.assign_representation(f, product=wall, representation=axis)
    layers = ifcopenshell.api.material.add_material_set(f, name="Brick wall", set_type="IfcMaterialLayerSet")
    layer = ifcopenshell.api.material.add_layer(f, layer_set=layers, material=brick)
    layer.LayerThickness = 200.0
    ifcopenshell.api.material.assign_material(f, products=[wall], type="IfcMaterialLayerSetUsage", material=layers)
    contain(gf, wall)

    # a door 1 m along the wall and a window 3 m along it, 0.9 m up
    for cls, offset, sill, width, height in [("IfcDoor", 1.0, 0.0, 0.9, 2.1), ("IfcWindow", 3.0, 0.9, 1.2, 1.2)]:
        opening = run(f, ifc_class="IfcOpeningElement", name=f"{cls} opening")
        geo.edit_object_placement(f, product=opening, matrix=placement(1 + offset, 2 - 0.1, sill))
        orep = geo.add_wall_representation(f, context=body, length=width, height=height, thickness=0.4)
        geo.assign_representation(f, product=opening, representation=orep)
        ifcopenshell.api.feature.add_feature(f, feature=opening, element=wall)
        fill = run(f, ifc_class=cls, name=cls[3:])
        kw = dict(context=body, overall_height=height * 1000, overall_width=width * 1000, unit_scale=0.001)
        if cls == "IfcDoor":
            frep = geo.add_door_representation(f, **kw)
        else:
            frep = geo.add_window_representation(f, **kw)
        geo.assign_representation(f, product=fill, representation=frep)
        fill.OverallWidth = width * 1000
        fill.OverallHeight = height * 1000
        ifcopenshell.api.feature.add_filling(f, opening=opening, element=fill)
        # the door's origin is its lower left corner, set into the wall
        geo.edit_object_placement(f, product=fill, matrix=placement(1 + offset, 2.05, sill))
        contain(gf, fill)

    # Wall B on the upper storey, turned 90°, its material on its type
    wtype = run(f, ifc_class="IfcWallType", name="Concrete 250")
    tlayers = ifcopenshell.api.material.add_material_set(f, name="Concrete wall", set_type="IfcMaterialLayerSet")
    tlayer = ifcopenshell.api.material.add_layer(f, layer_set=tlayers, material=concrete)
    tlayer.LayerThickness = 250.0
    ifcopenshell.api.material.assign_material(f, products=[wtype], type="IfcMaterialLayerSet", material=tlayers)
    wall_b = run(f, ifc_class="IfcWall", name="Wall B")
    geo.edit_object_placement(f, product=wall_b, matrix=placement(1, 2, 3.2, np.pi / 2))
    rep = geo.add_wall_representation(f, context=body, length=4.0, height=2.8, thickness=0.25)
    geo.assign_representation(f, product=wall_b, representation=rep)
    ifcopenshell.api.type.assign_type(f, related_objects=[wall_b], relating_type=wtype)
    contain(ff, wall_b)

    # slab: 6 x 4 m, 250 thick, its top at the ground storey
    slab = run(f, ifc_class="IfcSlab", name="Floor")
    geo.edit_object_placement(f, product=slab, matrix=placement(0.5, 0.5, -0.25))
    rep = geo.add_slab_representation(f, context=body, depth=0.25, polyline=[(0, 0), (6, 0), (6, 4), (0, 4)])
    geo.assign_representation(f, product=slab, representation=rep)
    ifcopenshell.api.material.assign_material(f, products=[slab], type="IfcMaterial", material=concrete)
    contain(gf, slab)

    # column: a 300 mm square profile, 3 m high, centred on (7, 1)
    column = run(f, ifc_class="IfcColumn", name="Column")
    geo.edit_object_placement(f, product=column, matrix=placement(7, 1, 0))
    profile = f.createIfcRectangleProfileDef("AREA", None, None, 300.0, 300.0)
    if schema == "IFC2X3":
        profile.Position = f.createIfcAxis2Placement2D(f.createIfcCartesianPoint((0.0, 0.0)))
    rep = geo.add_profile_representation(f, context=body, profile=profile, depth=3.0)
    geo.assign_representation(f, product=column, representation=rep)
    contain(gf, column)

    # a tessellated proxy on the upper storey: a unit right tetrahedron at (8, 3)
    proxy = run(f, ifc_class="IfcBuildingElementProxy", name="Tetra")
    geo.edit_object_placement(f, product=proxy, matrix=placement(8, 3, 3.2))
    verts = [[(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (0.0, 1.0, 0.0), (0.0, 0.0, 1.0)]]
    faces = [[[0, 2, 1], [0, 1, 3], [1, 2, 3], [0, 3, 2]]]
    rep = geo.add_mesh_representation(f, context=body, vertices=verts, faces=faces)
    geo.assign_representation(f, product=proxy, representation=rep)
    contain(ff, proxy)
    return f


def main() -> int:
    for schema, name in [("IFC4", "ifc4"), ("IFC2X3", "ifc2x3"), ("IFC4X3_ADD2", "ifc4x3")]:
        path = os.path.join(HERE, f"ifcopenshell-{name}.ifc")
        f = build(schema)
        f.write(path)
        print(path, os.path.getsize(path), "bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
