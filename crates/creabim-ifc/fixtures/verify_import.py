"""Check CreaBIM's IFC import against IfcOpenShell's geometry engine.

For every product IfcOpenShell can build geometry for, find the element
CreaBIM imported with the same GlobalId and compare where it is:

- walls: thickness, height and base from the body measured across and
  along the imported centreline, and the centreline inside the body;
- slabs: plan bounds and top/bottom;
- doors and windows: their extent along the host wall and in height;
- free meshes: world bounding boxes.

    cargo build --release -p creabim-cli
    python3 crates/creabim-ifc/fixtures/verify_import.py target/release/creabim file.ifc [...]

Exits non-zero if anything is off by more than the tolerance (1 mm for
native elements, 5 cm for doors and windows, whose frames may stand proud
of their openings). Elements imported with known approximations (booleans
ignored) are reported but not counted as failures.
"""

import json
import logging
import multiprocessing
import os
import subprocess
import sys
import tempfile
import uuid

import numpy as np
import ifcopenshell
import ifcopenshell.geom
import ifcopenshell.guid

TOL = 1e-3
OPENING_TOL = 0.05


def world_vertices(f):
    """GlobalId -> (n, 3) world vertices, for every product with a body."""
    settings = ifcopenshell.geom.settings()
    settings.set("use-world-coords", True)
    out = {}
    it = ifcopenshell.geom.iterator(settings, f, multiprocessing.cpu_count())
    if it.initialize():
        while True:
            shape = it.get()
            v = np.array(shape.geometry.verts).reshape(-1, 3)
            if len(v):
                out[shape.guid] = v
            if not it.next():
                break
    return out


def load_model(path):
    with open(path) as fh:
        elements = json.load(fh)["elements"]
    return {k: (e["name"], e["kind"]) for k, e in elements.items()}


def kind_of(kind):
    (name, value), = kind.items()
    return name, value


def main(cli, path):
    with tempfile.TemporaryDirectory() as tmp:
        out = os.path.join(tmp, "model.creabim.json")
        res = subprocess.run([cli, "import-ifc", path, out], capture_output=True, text=True)
        if res.returncode != 0:
            print(res.stderr)
            return 1
        model = load_model(out)
    f = ifcopenshell.open(path)
    geo = world_vertices(f)
    levels = {k: kind["Level"]["elevation"] for k, (_, kind) in model.items() if "Level" in kind}

    failures = 0
    approx = 0
    checked = {"Wall": 0, "Slab": 0, "Opening": 0, "FreeMesh": 0}
    missing = []
    for product in f.by_type("IfcProduct"):
        if product.is_a("IfcOpeningElement") or product.is_a("IfcSpace") or product.GlobalId not in geo:
            continue
        key = str(uuid.UUID(ifcopenshell.guid.expand(product.GlobalId)))
        if key not in model:
            missing.append(f"{product.is_a()} {product.Name}")
            continue
        name, kind = model[key]
        kname, k = kind_of(kind)
        v = geo[product.GlobalId]
        lo, hi = v.min(0), v.max(0)
        problems = []
        if kname == "Wall":
            a, b = np.array(k["start"]), np.array(k["end"])
            d = (b - a) / np.linalg.norm(b - a)
            n = np.array([-d[1], d[0]])
            rel = v[:, :2] - a
            s = rel @ n
            u = rel @ d
            base = levels[k["level"]] + k["base_offset"]
            if abs((s.max() - s.min()) - k["thickness"]) > TOL:
                problems.append(f"thickness {s.max() - s.min():.4f} vs {k['thickness']:.4f}")
            if abs((s.max() + s.min()) / 2) > TOL:
                problems.append(f"centreline off by {(s.max() + s.min()) / 2:.4f}")
            if abs(lo[2] - base) > TOL or abs(hi[2] - (base + k["height"])) > TOL:
                if v[:, 2].min() < base - TOL or v[:, 2].max() > base + k["height"] + TOL:
                    problems.append(f"z {lo[2]:.3f}..{hi[2]:.3f} vs {base:.3f}..{base + k['height']:.3f}")
                else:
                    approx += 1  # clipped: lower than the box, as warned
            span = np.linalg.norm(b - a)
            if u.min() > TOL + k["thickness"] or u.max() < span - TOL - k["thickness"]:
                problems.append(f"length {u.min():.3f}..{u.max():.3f} vs 0..{span:.3f}")
        elif kname == "Slab":
            bnd = np.array(k["boundary"])
            top = levels[k["level"]]
            if np.abs(bnd.min(0) - lo[:2]).max() > TOL or np.abs(bnd.max(0) - hi[:2]).max() > TOL:
                problems.append(f"plan {bnd.min(0)}..{bnd.max(0)} vs {lo[:2]}..{hi[:2]}")
            if abs(hi[2] - top) > TOL or abs(lo[2] - (top - k["thickness"])) > TOL:
                problems.append(f"z {lo[2]:.3f}..{hi[2]:.3f} vs {top - k['thickness']:.3f}..{top:.3f}")
        elif kname == "Opening":
            _, wall = kind_of(model[k["host"]][1])
            a, b = np.array(wall["start"]), np.array(wall["end"])
            d = (b - a) / np.linalg.norm(b - a)
            u = (v[:, :2] - a) @ d
            base = levels[wall["level"]] + wall["base_offset"] + k["sill"]
            ours = (k["offset"], k["offset"] + k["width"], base, base + k["height"])
            theirs = (u.min(), u.max(), lo[2], hi[2])
            if max(abs(x - y) for x, y in zip(ours, theirs)) > OPENING_TOL:
                problems.append(f"extent {np.round(ours, 3)} vs {np.round(theirs, 3)}")
        elif kname == "FreeMesh":
            pts = np.array(k["mesh"]["vertices"]).reshape(-1, 3)
            z = levels.get(k["level"], 0.0)
            c, s = np.cos(np.radians(k.get("rotation", 0))), np.sin(np.radians(k.get("rotation", 0)))
            pts = pts * k.get("scale", 1.0)
            pts = np.stack([pts[:, 0] * c - pts[:, 1] * s, pts[:, 0] * s + pts[:, 1] * c, pts[:, 2]], 1)
            pts = pts + np.array(k["origin"]) + np.array([0, 0, z])
            dl = np.abs(pts.min(0) - lo).max()
            dh = np.abs(pts.max(0) - hi).max()
            if max(dl, dh) > TOL:
                # booleans are imported as their first operand, which is never smaller
                if (pts.min(0) <= lo + TOL).all() and (pts.max(0) >= hi - TOL).all():
                    approx += 1
                else:
                    problems.append(f"bounds {pts.min(0)}..{pts.max(0)} vs {lo}..{hi}")
        checked[kname] = checked.get(kname, 0) + 1
        if problems:
            failures += 1
            print(f"MISMATCH {product.is_a()} {product.Name!r} as {kname}: {'; '.join(problems)}")
    print(
        f"{os.path.basename(path)}: {len(geo)} shapes from IfcOpenShell; checked "
        + ", ".join(f"{n} {k}" for k, n in checked.items())
        + f"; {approx} approximate (booleans ignored); {len(missing)} not imported"
    )
    for m in missing:
        print("  not imported:", m)
    return 1 if failures or missing else 0


if __name__ == "__main__":
    logging.disable(logging.WARNING)
    cli = sys.argv[1]
    sys.exit(max(main(cli, p) for p in sys.argv[2:]))
