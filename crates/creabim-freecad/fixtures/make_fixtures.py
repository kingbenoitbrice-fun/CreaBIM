# SPDX-License-Identifier: GPL-3.0-or-later
"""Regenerate the .FCStd fixtures used by the creabim-freecad tests.

Run inside the FreeCAD GUI so GuiDocument.xml (colours, visibility) is
written too; a virtual display is enough:

    xvfb-run -a freecad crates/creabim-freecad/fixtures/make_fixtures.py
    # or, FreeCAD 1.x only: QT_QPA_PLATFORM=offscreen freecad make_fixtures.py

Set FIXTURE_DIR to write somewhere else. FreeCADCmd also works but then the
files have no GuiDocument.xml. The tests hard-code the dimensions below, so
keep them in sync when changing this script.
"""

import math
import os

import FreeCAD as App
import Part

V = App.Vector

try:
    HERE = os.path.dirname(os.path.abspath(__file__))
except NameError:
    HERE = os.getcwd()
OUT = os.environ.get("FIXTURE_DIR", HERE)


def colour(obj, rgb, transparency=0):
    """Set the view colour when the GUI is up (ignored in FreeCADCmd)."""
    if App.GuiUp and obj.ViewObject is not None:
        obj.ViewObject.ShapeColor = rgb
        obj.ViewObject.Transparency = transparency


def rect_wire(x0, y0, x1, y1, z=0.0):
    pts = [V(x0, y0, z), V(x1, y0, z), V(x1, y1, z), V(x0, y1, z), V(x0, y0, z)]
    return Part.makePolygon(pts)


def make_parts():
    doc = App.newDocument("Parts")

    box = doc.addObject("Part::Box", "Box")
    box.Label = "Red box"
    box.Length, box.Width, box.Height = 1000, 500, 300
    box.Placement = App.Placement(V(100, 200, 0), App.Rotation(V(0, 0, 1), 30))

    cyl = doc.addObject("Part::Cylinder", "Cylinder")
    cyl.Label = "Column"
    cyl.Radius, cyl.Height = 200, 1000
    cyl.Placement.Base = V(2000, 0, 0)

    # Slab: 2000 x 1000 outline with a square and a round hole, 200 thick.
    outer = rect_wire(0, 0, 2000, 1000)
    square = rect_wire(300, 300, 700, 700)
    circle = Part.Wire(Part.makeCircle(150, V(1500, 500, 0)))
    # Part.Face([...]) keeps the wire orientations as given, which yields
    # an inside-out hole; the bullseye face maker orients holes properly.
    face = Part.makeFace([outer, square, circle], "Part::FaceMakerBullseye")
    slab = doc.addObject("Part::Feature", "Slab")
    slab.Label = "Slab with holes"
    slab.Shape = face.extrude(V(0, 0, 200))
    slab.Placement.Base = V(0, 0, 3000)

    sph = doc.addObject("Part::Sphere", "Sphere")
    sph.Radius = 300
    sph.Placement.Base = V(0, -2000, 0)

    cone = doc.addObject("Part::Cone", "Cone")
    cone.Radius1, cone.Radius2, cone.Height = 300, 100, 500
    cone.Placement.Base = V(-2000, 0, 0)

    torus = doc.addObject("Part::Torus", "Torus")
    torus.Radius1, torus.Radius2 = 500, 100
    torus.Placement.Base = V(0, 3000, 0)

    # Closed periodic B-spline outline extruded: exercises spline curves and
    # surfaces of linear extrusion.
    pts = [V(400 * math.cos(a), 250 * math.sin(a), 0)
           for a in [i * 2 * math.pi / 7 for i in range(7)]]
    bs = Part.BSplineCurve()
    bs.interpolate(pts, PeriodicFlag=True)
    blob = doc.addObject("Part::Feature", "Blob")
    blob.Shape = Part.Face(Part.Wire(bs.toShape())).extrude(V(0, 0, 300))
    blob.Placement.Base = V(3000, 3000, 0)

    import Mesh

    mesh = doc.addObject("Mesh::Feature", "Mesh")
    mesh.Label = "Mesh box"
    mesh.Mesh = Mesh.createBox(100, 200, 300)
    mesh.Placement.Base = V(0, 0, -1000)

    group = doc.addObject("App::DocumentObjectGroup", "Group")
    group.Label = "Assembly"
    group.addObject(box)
    group.addObject(cyl)

    doc.recompute()
    colour(box, (1.0, 0.0, 0.0))
    colour(cyl, (0.0, 0.0, 1.0), 50)
    colour(slab, (0.0, 1.0, 0.0))
    if App.GuiUp:
        sph.ViewObject.Visibility = False
    else:
        sph.Visibility = False

    report(doc)
    save(doc, "parts.FCStd")
    App.closeDocument(doc.Name)


def make_arch():
    import Arch

    doc = App.newDocument("Arch")

    wall = Arch.makeWall(None, length=4000, width=200, height=3000)
    wall.Label = "South wall"
    wall.Align = "Center"
    doc.recompute()

    win = Arch.makeWindowPreset(
        "Fixed", width=1000, height=1200, h1=50, h2=50, h3=50, w1=100, w2=50,
        o1=0, o2=50,
        placement=App.Placement(V(1500, 0, 900), App.Rotation(V(1, 0, 0), 90)),
    )
    win.Label = "Window"
    win.Hosts = [wall]

    # A curved wall on a 90 degree arc of radius 3000.
    arc = Part.Edge(Part.ArcOfCircle(Part.Circle(V(0, 0, 0), V(0, 0, 1), 3000),
                                     0, math.pi / 2))
    base = doc.addObject("Part::Feature", "Arc")
    base.Shape = Part.Wire(arc)
    curved = Arch.makeWall(base, width=200, height=2500)
    curved.Label = "Curved wall"
    curved.Align = "Center"
    curved.Placement.Base = V(0, 5000, 0)

    col = Arch.makeStructure(None, length=300, width=300, height=3000)
    col.Label = "Column"
    col.Placement.Base = V(5000, 0, 0)

    level = Arch.makeFloor([wall, win, col])
    level.Label = "Ground floor"
    doc.recompute()
    colour(wall, (0.8, 0.8, 0.7))

    report(doc)
    save(doc, "arch.FCStd")
    App.closeDocument(doc.Name)


REPORT = []


def make_binary():
    """A box saved with FreeCAD's 'SaveBinaryBrep' option (BinBRep .bin)."""
    grp = App.ParamGet("User parameter:BaseApp/Preferences/Document")
    old = grp.GetBool("SaveBinaryBrep", False)
    grp.SetBool("SaveBinaryBrep", True)
    try:
        doc = App.newDocument("Binary")
        box = doc.addObject("Part::Box", "Box")
        box.Length, box.Width, box.Height = 300, 200, 100
        box.Placement.Base = V(10, 20, 30)
        doc.recompute()
        report(doc)
        save(doc, "binary.FCStd")
        App.closeDocument(doc.Name)
    finally:
        grp.SetBool("SaveBinaryBrep", old)


def save(doc, name):
    """Save without leaving FreeCAD's .FCBak backup next to the fixture."""
    path = os.path.join(OUT, name)
    if os.path.exists(path):
        os.remove(path)
    doc.saveAs(path)


def report(doc):
    """Record what the tests should expect (the GUI swallows stdout)."""
    REPORT.append("# " + doc.Name)
    for o in doc.Objects:
        shp = getattr(o, "Shape", None)
        line = "%s %s %r" % (o.Name, o.TypeId, o.Label)
        if shp is not None and not shp.isNull():
            bb = shp.BoundBox
            line += " volume=%.3f bbox=(%.3f %.3f %.3f)-(%.3f %.3f %.3f)" % (
                shp.Volume, bb.XMin, bb.YMin, bb.ZMin, bb.XMax, bb.YMax, bb.ZMax)
        REPORT.append(line)


make_parts()
make_binary()
try:
    make_arch()
except Exception as exc:  # Arch needs extra modules; keep the Part fixture.
    print("Arch fixture skipped:", exc)

with open(os.path.join(OUT, "expected.txt"), "w") as f:
    f.write("Generated by make_fixtures.py with FreeCAD %s\n" % ".".join(App.Version()[:3]))
    f.write("\n".join(REPORT) + "\n")

if App.GuiUp:
    os._exit(0)
