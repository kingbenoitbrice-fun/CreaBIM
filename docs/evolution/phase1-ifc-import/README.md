# Phase 1: IFC import of B-rep models

Reference screenshots for the IFC import pass that follows the import flaw report of 2026-10-28. FreeCAD, and many other tools, write walls and slabs as faceted B-reps (`IfcFacetedBrep`, `IfcPolygonalFaceSet`) instead of extrusions. Before this pass, all of them came in as meshes, openings were not cut, and the warnings did not say which element they meant.

Two models are used:

- **The test house** (`crates/creabim-ifc/fixtures/freecad-style-house.ifc`), written by `fixtures/make_freecad_style.py` the way FreeCAD writes its files. It has brep walls with their holes already cut, windows with and without opening relationships, a two-layer wall along a polyline axis, a terrace slab with no storey at its height, a gable wall, a sloped roof slab and furniture.
- **OSA-M01** (`OSA-M01_REV01.ifc`), the real FreeCAD export from the report. It is a private project file, so it is not in the repository.

The OSA-M01 shots are kept with the project's private screenshots (`creabim-screenshots/phase1-ifc-import/`), not in the repository, since they show a private project.

| Screenshot | What it shows |
| --- | --- |
| [01-house-before.png](01-house-before.png) | The test house through the old importer. Walls and slabs are meshes (the plan cuts them as plain shapes), the windows are meshes floating in uncut walls, and the 3D view has no openings. |
| [02-house-after-report.png](02-house-after-report.png) | The same file opened now, with the new *Import report*. It shows counts per IFC class (native and mesh), the grouped summary, and one line per element with its class, Name, GlobalId and the reason. *Copy report* puts all of it on the clipboard. *File > Import report* opens it again. |
| [03-house-after.png](03-house-after.png) | The house after import. 9 native walls (the loggia's polyline wall becomes its three runs), 3 native slabs (the terrace keeps its top 450 mm above the ground floor), 5 windows and 2 doors hosted in their walls, a native column and a native beam. The gable wall stays a mesh, but its window opening is cut into it. |
| `04-osa-before.png` (private) | OSA-M01 through the old importer: 7 native walls, 1 slab, 1 door, no windows. The rest are meshes, and the walls are not cut. |
| `05-osa-after-report.png` (private) | OSA-M01's import report now. It names seven elements: two L-shaped wall breps and four notched or stepped beams left as meshes, and one opening with no host. |
| `06-osa-after.png` (private) | OSA-M01 now. The walls are native and cut, with all 11 windows and 3 doors in them, 15 columns and 12 beams native, and 4 slabs. |

## OSA-M01, before and after

Report §3.1 lists 13 walls, 11 windows, 3 doors and 4 slabs in the model.

| Element | Report §3.1 | Before (native) | After (native) |
| --- | --- | --- | --- |
| Walls | 13 | 7 | 25 native from 11 IFC walls (polyline walls split into runs); 2 L-shaped breps stay meshes |
| Windows | 11 | 0 | 11 (the window schedule lists W01 to W11) |
| Doors | 3 | 1 | 3 |
| Slabs | 4 | 1 | 4 |
| Holes (openings with no door or window) | | 1 | 8 |
| Columns | | 0 | 15 |
| Beams | | 0 | 12 (4 notched beams stay meshes) |
| Meshes in total | | 716 | 669 |

The other 669 meshes are furniture, pipes, fittings, coverings and members, which CreaBIM keeps as typed meshes.

`OSA-M01_REV00.FCStd` and `OSA-M01_REV00_Viz.blend` also link into the imported model with *Import and link…* (454 and 704 objects). The blend link used to fail on a zero-height window opening. That object is now left as a mesh, with a note.

## How to try it

- *File > Open* the test house: `crates/creabim-ifc/fixtures/freecad-style-house.ifc`. The report opens by itself whenever the import has notes.
- From the command line, `creabim-cli import-ifc <file.ifc> <out.creabim.json>` prints the same summary, the element-by-element list, and native and mesh counts per class.
- Tests: `cargo test -p creabim-ifc --test freecad` runs the golden test of the test house and the IFC4 round trip of the starter house. Set `CREABIM_OSA_M01=/path/to/OSA-M01_REV01.ifc` to also check the real file.

## What becomes native

- **Walls:** extrusions, as before, and now any brep or face set whose corners all lie on two parallel vertical faces. Holes found in its long face become the openings of its doors and windows, or *Hole* openings when nothing fills them. A wall along a polyline axis becomes one wall per run, with the runs joined at their corners.
- **Slabs:** any brep that is a flat, upright prism, with its outline and holes taken from the top face. Its top can be at any height: it goes on the storey below, with a top offset.
- **Doors and windows:** hosted through `IfcRelFillsElement`, or, when there is none, in the native wall they sit in. They snap to the hole cut in the wall, so the sizes and offsets are the hole's.
- **Mesh walls:** openings are cut into them, so their windows show.
- **Columns and beams:** rectangular and circular columns and straight beams of any body. CreaBIM's own IFC4 files also bring back roofs, stairs, railings and every member's type, through a `CreaBIM_Element` property set.

## Known gaps

- L-shaped or T-shaped wall breps stay meshes (OSA-M01's `Cut009` and `Plane_246`). They are not split into straight walls.
- Notched or stepped beams, and sloped columns and beams, stay meshes.
- Roofs, stairs and railings from other tools stay typed meshes. Only CreaBIM's own exports bring them back native.
- `IfcMember` (182 in OSA-M01) stays a mesh.
- `IfcSpace` is still skipped (rooms are a separate pass).
- The starter house round trip still loses its grids (IFC4 export writes none), and a shaft comes back as the holes it cuts.
- The import report shows the element list in a small font. It has no filter yet, and clicking a line doesn't select the element.
