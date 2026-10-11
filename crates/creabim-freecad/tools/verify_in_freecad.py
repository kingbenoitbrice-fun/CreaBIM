# SPDX-License-Identifier: GPL-3.0-or-later
"""Check that files written by creabim-freecad open correctly in FreeCAD.

    cargo run -p creabim-freecad --example write_samples -- /tmp/fc
    FC_CHECK_DIR=/tmp/fc freecadcmd crates/creabim-freecad/tools/verify_in_freecad.py
    # colours too (needs the GUI, e.g. under xvfb-run):
    FC_CHECK_DIR=/tmp/fc xvfb-run -a freecad crates/creabim-freecad/tools/verify_in_freecad.py

For every row of manifest.tsv it checks that the object exists, its shape is
valid, solids have the expected volume, the placement origin matches, the
IfcType hint survived and, in the GUI, the colour. Exits non-zero on failure
(FreeCADCmd) and writes the outcome to result.txt in the check directory.
"""

import os
import sys

import FreeCAD as App

DIR = os.environ.get("FC_CHECK_DIR") or (sys.argv[-1] if len(sys.argv) > 1 else ".")
failures = []
lines = []


def check(cond, msg):
    if not cond:
        failures.append(msg)


rows = {}
with open(os.path.join(DIR, "manifest.tsv"), encoding="utf-8") as f:
    for line in f:
        line = line.rstrip("\n")
        if not line:
            continue
        file, label, vol, pos, col, ifc = line.split("\t")
        rows.setdefault(file, []).append((label, float(vol), [float(v) for v in pos.split()], col, ifc))

for file, objs in sorted(rows.items()):
    doc = App.openDocument(os.path.join(DIR, file))
    shapes = [o for o in doc.Objects if o.TypeId == "Part::Feature"]
    check(len(shapes) == len(objs), "%s: %d Part::Feature, expected %d" % (file, len(shapes), len(objs)))
    groups = [o for o in doc.Objects if o.TypeId == "App::DocumentObjectGroup"]
    for g in groups:
        check(all(c in doc.Objects for c in g.Group), "%s: group %s has dangling members" % (file, g.Label))
    by_label = {}
    for o in shapes:
        by_label.setdefault(o.Label, []).append(o)
    for label, vol, pos, col, ifc in objs:
        cands = by_label.get(label) or []
        check(len(cands) >= 1, "%s: no object labelled %r" % (file, label))
        if not cands:
            continue
        o = cands.pop(0)
        shp = o.Shape
        check(not shp.isNull(), "%s/%s: null shape" % (file, label))
        if shp.isNull():
            continue
        check(shp.isValid(), "%s/%s: invalid shape" % (file, label))
        if vol > 0:
            check(len(shp.Solids) >= 1, "%s/%s: no solid" % (file, label))
            check(abs(shp.Volume - vol) <= 1e-6 * vol + 1e-6,
                  "%s/%s: volume %.6f, expected %.6f" % (file, label, shp.Volume, vol))
        base = o.Placement.Base
        check(abs(base.x - pos[0]) + abs(base.y - pos[1]) + abs(base.z - pos[2]) < 1e-6,
              "%s/%s: placement %s, expected %s" % (file, label, base, pos))
        if ifc:
            check(getattr(o, "IfcType", None) == ifc, "%s/%s: IfcType %r" % (file, label, getattr(o, "IfcType", None)))
        if App.GuiUp and col:
            r, g, b, a = [float(v) for v in col.split()]
            sc = o.ViewObject.ShapeColor
            check(all(abs(x - y) < 1.5 / 255 for x, y in zip(sc[:3], (r, g, b))),
                  "%s/%s: colour %s, expected %s" % (file, label, sc[:3], (r, g, b)))
            check(abs(o.ViewObject.Transparency - round((1 - a) * 100)) <= 1,
                  "%s/%s: transparency %s" % (file, label, o.ViewObject.Transparency))
        lines.append("ok %s/%s volume=%.3f" % (file, label, shp.Volume))
    App.closeDocument(doc.Name)

summary = "FreeCAD %s gui=%s: %d failures" % (".".join(App.Version()[:3]), App.GuiUp, len(failures))
with open(os.path.join(DIR, "result.txt"), "w", encoding="utf-8") as f:
    f.write("\n".join(lines + failures + [summary]) + "\n")
print("\n".join(failures + [summary]))
if App.GuiUp:
    os._exit(1 if failures else 0)
sys.exit(1 if failures else 0)
