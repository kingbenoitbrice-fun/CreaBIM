"""Write `freecad-style-house.ifc`: a small house the way FreeCAD's IFC
exporter writes one, for the golden import test (`tests/freecad.rs`).

FreeCAD exports most parts as `IfcFacetedBrep` bodies (with any door and
window holes already cut into the walls), writes some windows with their
`IfcRelVoidsElement` / `IfcRelFillsElement` and some without, draws
multi-layer walls round a room as one `IfcWall` with a polyline axis and
one extrusion per layer, and exports furniture as B-rep meshes. This
file has one of each, in metres:

Ground storey (0):
- Wall-Ext-S, 300 thick, 2.75 high, along y = 0: a B-rep with three holes
  cut: W1 (filled by relationship), W2 (no relationship) and door D1
  (relationship).
- Wall-Ext-N along y = 7, a B-rep with window W3 cut (no relationship) and
  a 400 mm service hole (an opening with no filling).
- Wall-Ext-E along x = 9, a B-rep with no hole cut: window W4 fills an
  opening that voids it, as IFC intends.
- Wall-Ext-W along x = 0, a plain B-rep.
- Wall-Int-01, a 100 mm partition with door D2 cut, no relationship.
- Wall-Garden, one L-shaped B-rep: stays a mesh (not a straight wall).
- Wall-Loggia, two layers round three sides of the loggia, one IfcWall
  with a polyline axis and an extrusion per layer.
- Slab-GF (top at 0), Slab-Terrace (top at 0.45, no storey there), column
  Col-Loggia and beam Beam-Loggia, all B-reps; furniture: Sofa (two
  boxes), Table (top and four legs), Bed.
First floor (3.0):
- Slab-FF with a stair hole, top at 3.0.
- Wall-FF-S with window W6 both cut and filling an opening.
- Wall-Gable-E, a gable end (sloped top): stays a mesh, with window W5 in
  an opening that is cut into the mesh.
- Roof-Slab, a sloped slab: stays a mesh.
- an IfcSpace (CreaBIM skips spaces).

    python3 -m pip install ifcopenshell
    python3 crates/creabim-ifc/fixtures/make_freecad_style.py

The test checks the counts this makes, so change both together.
"""

import math
import os
import sys

import numpy as np
import ifcopenshell
import ifcopenshell.api.aggregate
import ifcopenshell.api.context
import ifcopenshell.api.feature
import ifcopenshell.api.geometry
import ifcopenshell.api.material
import ifcopenshell.api.project
import ifcopenshell.api.root
import ifcopenshell.api.spatial
import ifcopenshell.api.unit

HERE = os.path.dirname(os.path.abspath(__file__))


def placement(x, y, z, angle=0.0):
    c, s = np.cos(angle), np.sin(angle)
    m = np.eye(4)
    m[:3, :3] = [[c, -s, 0], [s, c, 0], [0, 0, 1]]
    m[:3, 3] = [x, y, z]
    return m


def newell(loop):
    n = np.zeros(3)
    for i, a in enumerate(loop):
        b = loop[(i + 1) % len(loop)]
        n += [(a[1] - b[1]) * (a[2] + b[2]), (a[2] - b[2]) * (a[0] + b[0]), (a[0] - b[0]) * (a[1] + b[1])]
    return n


def facing(loop, normal):
    """The loop wound counter-clockwise seen from `normal`."""
    return loop if np.dot(newell(loop), normal) > 0 else loop[::-1]


def box_faces(lo, hi):
    (x0, y0, z0), (x1, y1, z1) = lo, hi
    p = lambda x, y, z: (x, y, z)
    faces = [
        ([p(x0, y0, z0), p(x0, y1, z0), p(x1, y1, z0), p(x1, y0, z0)], (0, 0, -1)),
        ([p(x0, y0, z1), p(x1, y0, z1), p(x1, y1, z1), p(x0, y1, z1)], (0, 0, 1)),
        ([p(x0, y0, z0), p(x1, y0, z0), p(x1, y0, z1), p(x0, y0, z1)], (0, -1, 0)),
        ([p(x0, y1, z0), p(x0, y1, z1), p(x1, y1, z1), p(x1, y1, z0)], (0, 1, 0)),
        ([p(x0, y0, z0), p(x0, y0, z1), p(x0, y1, z1), p(x0, y1, z0)], (-1, 0, 0)),
        ([p(x1, y0, z0), p(x1, y1, z0), p(x1, y1, z1), p(x1, y0, z1)], (1, 0, 0)),
    ]
    return [(facing(l, n), []) for l, n in faces]


def prism_faces(outer, holes, z0, z1):
    """An upright prism: `outer` and `holes` are plan loops."""
    lift = lambda loop, z: [(x, y, z) for x, y in loop]
    out = [
        (facing(lift(outer, z1), (0, 0, 1)), [facing(lift(h, z1), (0, 0, -1)) for h in holes]),
        (facing(lift(outer, z0), (0, 0, -1)), [facing(lift(h, z0), (0, 0, 1)) for h in holes]),
    ]
    area = lambda l: sum(l[i][0] * l[(i + 1) % len(l)][1] - l[(i + 1) % len(l)][0] * l[i][1] for i in range(len(l)))
    for loop, inward in [(outer, False)] + [(h, True) for h in holes]:
        ccw = area(loop) > 0
        for i, a in enumerate(loop):
            b = loop[(i + 1) % len(loop)]
            d = (b[0] - a[0], b[1] - a[1])
            n = (d[1], -d[0], 0) if ccw else (-d[1], d[0], 0)
            if inward:
                n = (-n[0], -n[1], 0)
            q = [(a[0], a[1], z0), (b[0], b[1], z0), (b[0], b[1], z1), (a[0], a[1], z1)]
            out.append((facing(q, n), []))
    return out


def wall_faces(length, t, height, holes):
    """A straight wall along local x, centred on y = 0, from z 0 to
    `height`, with rectangular holes (u0, u1, z0, z1) cut through it."""
    y0, y1 = -t / 2, t / 2
    doors = sorted(h for h in holes if h[2] <= 0)
    windows = [h for h in holes if h[2] > 0]
    # the long faces: the outline notched by doors, windows as inner bounds
    xz = [(0.0, 0.0)]
    for u0, u1, _, z1 in doors:
        xz += [(u0, 0.0), (u0, z1), (u1, z1), (u1, 0.0)]
    xz += [(length, 0.0), (length, height), (0.0, height)]
    faces = []
    for y, n in [(y0, (0, -1, 0)), (y1, (0, 1, 0))]:
        outer = facing([(x, y, z) for x, z in xz], n)
        inner = [
            facing([(u0, y, z0), (u1, y, z0), (u1, y, z1), (u0, y, z1)], tuple(-c for c in n))
            for u0, u1, z0, z1 in windows
        ]
        faces.append((outer, inner))
    faces.append((facing([(0, y0, height), (length, y0, height), (length, y1, height), (0, y1, height)], (0, 0, 1)), []))
    xs = [0.0] + [v for d in doors for v in d[:2]] + [length]
    for a, b in zip(xs[::2], xs[1::2]):
        faces.append((facing([(a, y0, 0), (b, y0, 0), (b, y1, 0), (a, y1, 0)], (0, 0, -1)), []))
    for x, n in [(0.0, (-1, 0, 0)), (length, (1, 0, 0))]:
        faces.append((facing([(x, y0, 0), (x, y1, 0), (x, y1, height), (x, y0, height)], n), []))
    for u0, u1, z0, z1 in holes:
        side = lambda x: [(x, y0, z0), (x, y1, z0), (x, y1, z1), (x, y0, z1)]
        flat = lambda z: [(u0, y0, z), (u1, y0, z), (u1, y1, z), (u0, y1, z)]
        faces.append((facing(side(u0), (1, 0, 0)), []))
        faces.append((facing(side(u1), (-1, 0, 0)), []))
        faces.append((facing(flat(z1), (0, 0, -1)), []))
        if z0 > 0:
            faces.append((facing(flat(z0), (0, 0, 1)), []))
    return faces


def gable_faces(length, t, eave, ridge):
    """A gable end along local x: a pentagon in x-z, `t` thick."""
    prof = [(0, 0), (length, 0), (length, eave), (length / 2, ridge), (0, eave)]
    y0, y1 = -t / 2, t / 2
    faces = [
        (facing([(x, y0, z) for x, z in prof], (0, -1, 0)), []),
        (facing([(x, y1, z) for x, z in prof], (0, 1, 0)), []),
    ]
    for i, a in enumerate(prof):
        b = prof[(i + 1) % len(prof)]
        q = [(a[0], y0, a[1]), (b[0], y0, b[1]), (b[0], y1, b[1]), (a[0], y1, a[1])]
        mid = ((a[0] + b[0]) / 2 - length / 2, 0, (a[1] + b[1]) / 2 - ridge / 3)
        faces.append((facing(q, mid), []))
    return faces


def build() -> ifcopenshell.file:
    f = ifcopenshell.api.project.create_file(version="IFC4")
    f.header.file_name.originating_system = "FreeCAD 1.0 / IfcOpenShell"
    run = ifcopenshell.api.root.create_entity
    geo = ifcopenshell.api.geometry
    project = run(f, ifc_class="IfcProject", name="FreeCAD-style house")
    ifcopenshell.api.unit.assign_unit(f, units=[ifcopenshell.api.unit.add_si_unit(f, unit_type="LENGTHUNIT")])
    model = ifcopenshell.api.context.add_context(f, context_type="Model")
    body = ifcopenshell.api.context.add_context(
        f, context_type="Model", context_identifier="Body", target_view="MODEL_VIEW", parent=model
    )
    axis_ctx = ifcopenshell.api.context.add_context(
        f, context_type="Model", context_identifier="Axis", target_view="GRAPH_VIEW", parent=model
    )
    site = run(f, ifc_class="IfcSite", name="Site")
    building = run(f, ifc_class="IfcBuilding", name="House")
    gf = run(f, ifc_class="IfcBuildingStorey", name="Ground Floor")
    ff = run(f, ifc_class="IfcBuildingStorey", name="First Floor")
    ifcopenshell.api.aggregate.assign_object(f, relating_object=project, products=[site])
    ifcopenshell.api.aggregate.assign_object(f, relating_object=site, products=[building])
    ifcopenshell.api.aggregate.assign_object(f, relating_object=building, products=[gf, ff])
    for el, z in [(site, 0), (building, 0), (gf, 0), (ff, 3.0)]:
        geo.edit_object_placement(f, product=el, matrix=placement(0, 0, z))
    gf.Elevation, ff.Elevation = 0.0, 3.0
    brick = ifcopenshell.api.material.add_material(f, name="Brick", category="brick")
    concrete = ifcopenshell.api.material.add_material(f, name="Concrete", category="concrete")
    timber = ifcopenshell.api.material.add_material(f, name="Timber", category="wood")

    def point(p):
        return f.createIfcCartesianPoint(tuple(float(c) for c in p))

    def brep(faces):
        ifc_faces = []
        for outer, inners in faces:
            bounds = [f.createIfcFaceOuterBound(f.createIfcPolyLoop([point(p) for p in outer]), True)]
            bounds += [f.createIfcFaceBound(f.createIfcPolyLoop([point(p) for p in h]), True) for h in inners]
            ifc_faces.append(f.createIfcFace(bounds))
        return f.createIfcFacetedBrep(f.createIfcClosedShell(ifc_faces))

    def product(cls, name, storey, at, items, material=None, **attrs):
        e = run(f, ifc_class=cls, name=name)
        for k, v in attrs.items():
            setattr(e, k, v)
        geo.edit_object_placement(f, product=e, matrix=at)
        rep = f.createIfcShapeRepresentation(body, "Body", "Brep", items)
        geo.assign_representation(f, product=e, representation=rep)
        if storey is not None:
            ifcopenshell.api.spatial.assign_container(f, relating_structure=storey, products=[e])
        if material is not None:
            ifcopenshell.api.material.assign_material(f, products=[e], type="IfcMaterial", material=material)
        return e

    def wall(name, storey, x, y, angle, length, t, height, holes=(), z=0.0):
        return product(
            "IfcWall", name, storey, placement(x, y, z, angle), [brep(wall_faces(length, t, height, list(holes)))], brick
        )

    def filling(cls, name, storey, wall_at, u0, u1, z0, z1, depth=0.08):
        """A door or window body: a frame-sized box set in the wall."""
        x, y, z, angle = wall_at
        at = placement(x, y, z, angle) @ placement(u0, 0, z0)
        faces = box_faces((0, -depth / 2, 0), (u1 - u0, depth / 2, z1 - z0))
        return product(
            cls, name, storey, at, [brep(faces)], timber, OverallWidth=u1 - u0, OverallHeight=z1 - z0
        )

    def opening(name, host, wall_at, u0, u1, z0, z1, t):
        x, y, z, angle = wall_at
        at = placement(x, y, z, angle) @ placement(u0, 0, z0)
        o = run(f, ifc_class="IfcOpeningElement", name=name)
        geo.edit_object_placement(f, product=o, matrix=at)
        faces = box_faces((0, -t, 0), (u1 - u0, t, z1 - z0))
        geo.assign_representation(f, product=o, representation=f.createIfcShapeRepresentation(body, "Body", "Brep", [brep(faces)]))
        ifcopenshell.api.feature.add_feature(f, feature=o, element=host)
        return o

    def fill(o, e):
        ifcopenshell.api.feature.add_filling(f, opening=o, element=e)

    # ground floor walls (300 thick, 2.75 high); S and N run past the corners
    h = 2.75
    s_at = (-0.15, 0.0, 0.0, 0.0)
    w1, w2, d1 = (1.65, 2.85, 0.9, 2.1), (5.15, 6.35, 0.9, 2.1), (7.35, 8.25, 0.0, 2.1)
    s = wall("Wall-Ext-S", gf, *s_at[:2], 0.0, 9.3, 0.3, h, [w1, w2, d1])
    o = opening("W1 opening", s, s_at, *w1, 0.3)
    fill(o, filling("IfcWindow", "W1", gf, s_at, *w1))
    filling("IfcWindow", "W2", gf, s_at, *w2)
    o = opening("D1 opening", s, s_at, *d1, 0.3)
    fill(o, filling("IfcDoor", "D1", gf, s_at, *d1))

    n_at = (9.15, 7.0, 0.0, math.pi)
    w3, service = (2.15, 3.35, 0.9, 2.1), (6.15, 6.55, 0.3, 0.7)
    n = wall("Wall-Ext-N", gf, *n_at[:2], math.pi, 9.3, 0.3, h, [w3, service])
    filling("IfcWindow", "W3", gf, n_at, *w3)
    opening("Service hole", n, n_at, *service, 0.3)

    e_at = (9.0, 0.15, 0.0, math.pi / 2)
    w4 = (2.0, 3.2, 0.9, 2.1)
    e = wall("Wall-Ext-E", gf, *e_at[:2], math.pi / 2, 6.7, 0.3, h)
    o = opening("W4 opening", e, e_at, *w4, 0.3)
    fill(o, filling("IfcWindow", "W4", gf, e_at, *w4))

    wall("Wall-Ext-W", gf, 0.0, 6.85, -math.pi / 2, 6.7, 0.3, h)

    i_at = (4.5, 0.15, 0.0, math.pi / 2)
    d2 = (3.0, 3.8, 0.0, 2.0)
    wall("Wall-Int-01", gf, *i_at[:2], math.pi / 2, 6.7, 0.1, h, [d2])
    filling("IfcDoor", "D2", gf, i_at, *d2, depth=0.05)

    garden = [(10, 0), (12, 0), (12, 2), (11.8, 2), (11.8, 0.2), (10, 0.2)]
    product("IfcWall", "Wall-Garden", gf, placement(0, 0, 0), [brep(prism_faces(garden, [], 0, 1.2))], brick)

    # the loggia's wall: round three sides (axis on its outer face), a
    # 200 block layer and a 50 render layer, one extrusion each
    axis = [(9.15, 6.5), (12.5, 6.5), (12.5, 9.5), (9.15, 9.5)]
    loggia = run(f, ifc_class="IfcWall", name="Wall-Loggia")
    geo.edit_object_placement(f, product=loggia, matrix=placement(0, 0, 0))

    def band(lo, hi):
        """The region between offsets lo..hi inside the axis (a U)."""
        x0, x1, y0, y1 = 9.15, 12.5, 6.5, 9.5
        outer = [(x0, y0 + lo), (x1 - lo, y0 + lo), (x1 - lo, y1 - lo), (x0, y1 - lo)]
        inner = [(x0, y1 - hi), (x1 - hi, y1 - hi), (x1 - hi, y0 + hi), (x0, y0 + hi)]
        return outer + inner

    items = []
    for lo, hi in [(0.0, 0.05), (0.05, 0.25)]:
        pts = [f.createIfcCartesianPoint((float(x), float(y))) for x, y in band(lo, hi)]
        profile = f.createIfcArbitraryClosedProfileDef("AREA", None, f.createIfcPolyline(pts + [pts[0]]))
        items.append(
            f.createIfcExtrudedAreaSolid(
                profile,
                f.createIfcAxis2Placement3D(point((0, 0, 0))),
                f.createIfcDirection((0.0, 0.0, 1.0)),
                2.4,
            )
        )
    geo.assign_representation(f, product=loggia, representation=f.createIfcShapeRepresentation(body, "Body", "SweptSolid", items))
    axis_curve = f.createIfcPolyline([point((x, y)) for x, y in axis])
    geo.assign_representation(f, product=loggia, representation=f.createIfcShapeRepresentation(axis_ctx, "Axis", "Curve2D", [axis_curve]))
    ifcopenshell.api.spatial.assign_container(f, relating_structure=gf, products=[loggia])

    # slabs, a column and a beam
    product("IfcSlab", "Slab-GF", gf, placement(0, 0, 0), [brep(prism_faces([(-0.15, -0.15), (9.15, -0.15), (9.15, 7.15), (-0.15, 7.15)], [], -0.25, 0.0))], concrete, PredefinedType="FLOOR")
    product("IfcSlab", "Slab-Terrace", gf, placement(0, 0, 0), [brep(prism_faces([(9.15, 3.0), (12.0, 3.0), (12.0, 6.0), (9.15, 6.0)], [], 0.3, 0.45))], concrete, PredefinedType="FLOOR")
    product("IfcColumn", "Col-Loggia", gf, placement(0, 0, 0), [brep(box_faces((10.85, 6.0, 0.0), (11.15, 6.3, 2.75)))], concrete)
    product("IfcBeam", "Beam-Loggia", gf, placement(0, 0, 0), [brep(box_faces((9.15, 6.05, 2.45), (11.15, 6.25, 2.75)))], timber)
    # furniture
    product("IfcFurnishingElement", "Sofa", gf, placement(1.0, 4.5, 0.0), [brep(box_faces((0, 0, 0), (2.0, 0.9, 0.45))), brep(box_faces((0, 0.7, 0.45), (2.0, 0.9, 0.85)))], timber)
    legs = [brep(box_faces((x, y, 0), (x + 0.06, y + 0.06, 0.72))) for x in (0, 1.34) for y in (0, 0.74)]
    product("IfcFurnishingElement", "Table", gf, placement(2.0, 2.0, 0.0), [brep(box_faces((0, 0, 0.72), (1.4, 0.8, 0.76)))] + legs, timber)
    product("IfcFurnishingElement", "Bed", ff, placement(1.0, 1.0, 3.0), [brep(box_faces((0, 0, 0), (1.6, 2.0, 0.5)))], timber)

    # first floor
    product("IfcSlab", "Slab-FF", ff, placement(0, 0, 0), [brep(prism_faces([(-0.15, -0.15), (9.15, -0.15), (9.15, 7.15), (-0.15, 7.15)], [[(6.0, 4.0), (6.0, 6.5), (8.0, 6.5), (8.0, 4.0)]], 2.75, 3.0))], concrete, PredefinedType="FLOOR")
    ffs_at = (-0.15, 0.0, 3.0, 0.0)
    w6 = (3.65, 4.85, 0.9, 2.1)
    ffs = wall("Wall-FF-S", ff, -0.15, 0.0, 0.0, 9.3, 0.3, 2.6, [w6], z=3.0)
    o = opening("W6 opening", ffs, ffs_at, *w6, 0.3)
    fill(o, filling("IfcWindow", "W6", ff, ffs_at, *w6))
    g_at = (9.0, 0.15, 3.0, math.pi / 2)
    w5 = (2.75, 3.95, 1.0, 2.2)
    gable = product("IfcWall", "Wall-Gable-E", ff, placement(*g_at[:3], g_at[3]), [brep(gable_faces(6.7, 0.3, 1.2, 3.6))], brick)
    o = opening("W5 opening", gable, g_at, *w5, 0.3)
    fill(o, filling("IfcWindow", "W5", ff, g_at, *w5))
    # a sloped roof slab: a box turned 30° about x
    c, sn = math.cos(math.radians(30)), math.sin(math.radians(30))
    roof = [
        ([(x, y * c - z * sn, y * sn + z * c + 5.6) for x, y, z in loop], [])
        for loop, _ in box_faces((-0.3, 0.0, 0.0), (9.3, 4.2, 0.2))
    ]
    product("IfcSlab", "Roof-Slab", ff, placement(0, -0.3, 0), [brep(roof)], timber, PredefinedType="ROOF")
    space = run(f, ifc_class="IfcSpace", name="Living")
    ifcopenshell.api.aggregate.assign_object(f, relating_object=gf, products=[space])
    return f


def main() -> int:
    path = os.path.join(HERE, "freecad-style-house.ifc")
    build().write(path)
    print(path, os.path.getsize(path), "bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
