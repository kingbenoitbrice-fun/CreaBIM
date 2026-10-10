# CreaBIM

CreaBIM is an open-source, BIM-native architectural modeler: one model for freeform concept design and permit drawings, with Blender-style modeling, live 2D views and native IFC.

![CreaBIM showing the sample house in 3D beside its generated ground-floor plan](docs/screenshot.png)

**Status: Phase 0 prototype.** It proves the core architecture end to end. It is not yet a tool for real projects.

## What works today

- **One model, many views.** The 3D view and the plan read the same element store. An edit in either shows up in both at once.
- **Plan view in three places:** docked beside 3D, in its own window (for a second monitor), or overlaid inside the 3D workspace. *View → Show plan cut in 3D space* draws the plan's cut lines at their real height in 3D.
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
| Freeform mass | Mass tool, click to place; click a face in 3D, then E to extrude |
| Undo, redo | Ctrl+Z, Ctrl+Shift+Z or Ctrl+Y |
| Delete | Delete key (also removes doors and windows hosted on a wall) |

Files: *File → Save/Open* use the path in the toolbar (`.creabim.json`). *Export IFC4* and *Export plan as SVG* write next to it.

## How it is built

| Crate | Role |
| --- | --- |
| `creabim-core` | Element store, edit transactions, undo history, Blender-style edit mesh. Every edit is a reversible op, which later also powers version history and live collaboration. |
| `creabim-geom` | Element geometry (walls with openings, slabs, meshes), plan drawing generation, picking. This is where OpenCascade plugs in for NURBS and B-rep work. |
| `creabim-ifc` | IFC4 STEP writer. Element ids are IFC GlobalIds, so re-exports keep identity. |
| `creabim-app` | Desktop app: egui UI, wgpu 3D viewport rendered offscreen so any number of live views can exist. |
| `creabim-cli` | Command-line export tools. |

Next steps follow the roadmap's Phase 0 to Phase 1 plan: a SQLite project file, IFC import, sections and elevations, sheets with live views, dimensions and tags, OpenCascade integration, and a Windows installer.

## License

CreaBIM is free software under the GNU General Public License v3.0 or later. See [LICENSE](LICENSE).
