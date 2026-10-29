# IFC import fixtures

The importer tests (`tests/import.rs`, `tests/ifcx.rs`) read only these
committed files, so CI needs no Python or network.

## Written for CreaBIM

| File | What it is |
|------|------------|
| `ifcopenshell-ifc4.ifc`, `ifcopenshell-ifc2x3.ifc`, `ifcopenshell-ifc4x3.ifc` | Written by IfcOpenShell's API with `make_fixtures.py`: a project in millimetres with two storeys, a wall (axis + layer set usage) with a door and a window in openings, a typed wall on the upper storey, a slab, an extruded column and a tessellated proxy (a faceted B-rep in IFC2X3). |
| `freecad-style-house.ifc` | Written with IfcOpenShell by `make_freecad_style.py` the way FreeCAD's exporter writes a house: faceted B-rep walls with door and window holes cut, windows with and without `IfcRelVoidsElement` / `IfcRelFillsElement`, a two-layer wall along a polyline axis, B-rep slabs (one with a stair hole, one topped off a storey), a column, a beam, a gable end and a sloped roof slab (which stay meshes) and B-rep furniture. `tests/freecad.rs` checks what becomes native. |
| `layered-wall.ifcx` | Hand-written IFC5 alpha file in the style of buildingSMART's examples: layered entries for one path, a window type inherited with an invisible void, USD transforms and an inherited material. |

Regenerate the IfcOpenShell files with

    python3 -m pip install ifcopenshell
    python3 crates/creabim-ifc/fixtures/make_fixtures.py
    python3 crates/creabim-ifc/fixtures/make_freecad_style.py

and check any IFC file against IfcOpenShell's geometry engine with

    cargo build --release -p creabim-cli
    python3 crates/creabim-ifc/fixtures/verify_import.py target/release/creabim file.ifc

## Third-party files (`thirdparty/`)

Copied unmodified; each keeps its own licence, which allows redistribution.

| File | Source | Licence |
|------|--------|---------|
| `web-ifc-example.ifc` (414 KB) | `tests/ifcfiles/public/example.ifc` in [ThatOpen/engine_web-ifc](https://github.com/ThatOpen/engine_web-ifc) (main branch, fetched 2026-10-10). A Revit structural model exported to IFC2X3 through EDM: walls, footings, slabs, beams and columns, millimetres and degrees. | Mozilla Public License 2.0, as the web-ifc repository. MPL-2.0 is file-level copyleft and compatible with GPL-3.0-or-later; the file stays under MPL-2.0 (<https://mozilla.org/MPL/2.0/>). |
| `bonsai-basic.ifc` (12 KB) | `src/bonsai/test/files/basic.ifc` in [IfcOpenShell/IfcOpenShell](https://github.com/IfcOpenShell/IfcOpenShell) (tag v0.8.0). IFC4 written by IfcOpenShell for Bonsai's tests: a wall, slab and beam whose bodies are polygonal face sets mapped from their types. | GPL-3.0-or-later (Bonsai; IfcOpenShell's libraries are LGPL-3.0-or-later), the same licence family as CreaBIM. |

buildingSMART's IFC5 examples (`hello-wall.ifcx` from
[IFC5-development](https://github.com/buildingSMART/IFC5-development)) were
used to check the IFCX importer locally (one native wall with two hosted
windows), but are not committed: that repository has no licence file and
asks that its examples not be redistributed as derivatives.
