# CreaBIM

CreaBIM is an open-source, BIM-native architectural modeler: one model for freeform concept design and permit drawings, with Blender-style modeling, live 2D views and native IFC.

![CreaBIM showing the sample house in 3D beside its generated ground-floor plan](docs/screenshot.png)

**Status: Phase 1 in progress (sheets and PDF export).** It proves the core architecture end to end. It is not yet a tool for real projects.

## What works today

- **One model, many views.** The 3D view and the plan read the same element store. An edit in either shows up in both at once.
- **Views are model elements:** plans, sections and elevations live in the model, so they save, undo and update with it. Pick one from the *View* list or the *Views* group in the model list.
- **2D view in three places:** docked beside 3D, in its own window (for a second monitor), or overlaid inside the 3D workspace. *View → Show the 2D view in 3D space* draws a plan at its cut height, or a section standing on its section line.
- **Sheets with live views:** place plans, sections, elevations and 3D views on A0 to A4 sheets with a title block. Views on a sheet are generated from the current model, so they never go stale. Drag a view to move it; double-click it to work on the model right there, with the same tools as the view itself (orbit a 3D view by dragging). *File → Export sheets as PDF* writes every sheet as a vector PDF page.
- **Clean joins and hatching:** wall corners are mitred and walls meeting another wall's side are trimmed to its face, in 3D, plans, sections and IFC. Cut walls are hatched diagonally and slabs cross-hatched.
- **Sections and elevations generated from the model:** cut walls, slabs and openings are drawn heavy; what lies beyond is projected with hidden lines removed. Level datums are labelled. Section and elevation marks appear in plans. Draw new ones with the *Section* and *Elevation* tools, and drag a selected section line by its ends or middle. Walls, doors, windows and masses can be drawn in sections and elevations too. Progress screenshots are in [docs/evolution](docs/evolution).
- **BIM elements:** levels, grids, walls, slabs, doors and windows. Doors and windows are hosted by walls and cut real openings.
- **Freeform meshes:** add a mass, click a face in 3D and extrude (E), taper, move or subdivide it (Catmull-Clark). Give any mesh an IFC class such as `IfcRoof` and it exports as that.
- **Undo and redo** for every edit. Dragging a value in the properties panel undoes as one step.
- **Plan drawings generated from the model:** walls cut at a set height, door swings, window symbols, slabs and masses in projection. Export as SVG at 1:100.
- **IFC4 export** with storeys, walls, slabs, openings, doors, windows and tessellated freeform elements. CI checks every export against the IFC schema and builds its geometry with [IfcOpenShell](https://ifcopenshell.org).

## Build and run

You need a recent stable Rust toolchain (1.95 or newer).

```sh
cargo run --release -p creabim-app   # the desktop app
cargo test --workspace               # the test suite
```

On Linux the app also needs a Vulkan or GL driver and `libxkbcommon-x11` at runtime. CI builds a Windows `.exe` on every pull request, downloadable from the run's artifacts.

### Command line

```sh
cargo run -p creabim-cli -- sample house.creabim.json            # write the sample house
cargo run -p creabim-cli -- ifc house.creabim.json house.ifc     # export IFC4
cargo run -p creabim-cli -- plan house.creabim.json "Ground Floor" gf.svg
cargo run -p creabim-cli -- views house.creabim.json             # list plans, sections, elevations
cargo run -p creabim-cli -- view house.creabim.json "Section A" a.svg
cargo run -p creabim-cli -- sheets house.creabim.json            # list sheets
cargo run -p creabim-cli -- pdf house.creabim.json house.pdf      # every sheet, one PDF
python3 tools/validate_ifc.py house.ifc                          # needs: pip install ifcopenshell pytest
```

## Using the app

| Action | How |
| --- | --- |
| Orbit, pan, zoom (3D) | Left drag, right drag, wheel |
| Pan, zoom (plan) | Right drag, wheel |
| Draw walls | Wall tool, click points; Esc or right-click ends; Shift keeps it orthogonal |
| Doors, windows | Door or Window tool, click on a wall |
| Slab | Slab tool, click two corners |
| Section, elevation | Section or Elevation tool in a plan, click two points; it looks to the left of the direction you draw |
| Move a section line | Select it in a plan, drag the square end handles or the round middle handle |
| Draw in a section | Wall: click two points (length along the line, height as drawn). Door, Window: click on a wall. Mass: click to place |
| Sheets | Pick a sheet from the View list. *Place view…* adds a view, drag to move it, double-click to edit through it, Esc to finish |
| Orbit a 3D view | Drag in it, on its own or on a sheet |
| Freeform mass | Mass tool, click to place; click a face in 3D, then E to extrude |
| Undo, redo | Ctrl+Z, Ctrl+Shift+Z or Ctrl+Y |
| Delete | Delete key (also removes doors and windows hosted on a wall) |

Files: *File → Save/Open* use the path in the toolbar (`.creabim.json`). *Export IFC4* and *Export view as SVG* write next to it.

## How it is built

| Crate | Role |
| --- | --- |
| `creabim-core` | Element store, edit transactions, undo history, Blender-style edit mesh. Every edit is a reversible op, which later also powers version history and live collaboration. |
| `creabim-geom` | Element geometry (walls with openings, slabs, meshes), plan drawing generation, picking. This is where OpenCascade plugs in for NURBS and B-rep work. |
| `creabim-ifc` | IFC4 STEP writer. Element ids are IFC GlobalIds, so re-exports keep identity. |
| `creabim-app` | Desktop app: egui UI, wgpu 3D viewport rendered offscreen so any number of live views can exist. |
| `creabim-cli` | Command-line export tools. |

Next steps follow the roadmap's Phase 1 plan: dimensions, tags and schedules, a modifier stack and workplane sketches, a SQLite project file with IFC import, and a Windows installer.

## License

CreaBIM is free software under the GNU General Public License v3.0 or later. See [LICENSE](LICENSE).
