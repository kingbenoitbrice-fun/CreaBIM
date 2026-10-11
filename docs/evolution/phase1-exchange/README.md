# Phase 1, milestone 5: files, exchange and modelling in place

Reference screenshots of what this milestone is meant to do. Most use the sample house; the Blender shots use a small garden file made in Blender 5.2, and the last one opens a structural model exported from Revit.

| Screenshot | What it shows |
| --- | --- |
| [01-handles-and-icons.png](01-handles-and-icons.png) | The model list has an icon for each category. A selected wall shows its handles in 3D: round ones for its height and faces, square ones for its ends. The divider between the 3D and 2D panes can be dragged, as in Blender (double-click resets it). |
| [02-door-widened-in-3d.png](02-door-widened-in-3d.png) | The front door's jamb handle dragged in 3D: the door gets wider, and the readout shows its new width. Head and sill handles change its height. |
| [03-door-widened-in-plan.png](03-door-widened-in-plan.png) | The same in the plan: the door's jamb handle widens it, and its swing and tag follow. |
| [04-walls-drawn-in-3d.png](04-walls-drawn-in-3d.png) | The Wall tool used right in the 3D view, on the ground floor, snapping to an existing corner (*Endpoint*). Walls, slabs, masses and sections can all be drawn in 3D; doors and windows go into the wall under the pointer. |
| [05-boolean-subtract.png](05-boolean-subtract.png) | A mass subtracted from the south wall with *Subtract…*. The hole shows in 3D and in the plan; the mass becomes a see-through ghost you can still select and move to change the cut. |
| [06-guides.png](06-guides.png) | The Guide tool: one guide parallel to the south wall, 1.25 m out (click the wall's edge, then where the guide goes), and one through two points. Guides show dashed in plans and in 3D, everything snaps to them (*On guide*), and they are never printed or exported. |
| [07-redraw-wall-path.png](07-redraw-wall-path.png) | *Redraw path* on the partition: click a new path with snaps; an extra corner adds a second wall like it. Slabs have *Redraw outline*. Both work in plans and in 3D. |
| [08-blend-linked.png](08-blend-linked.png) | A Blender file linked with *File > Import and link…*: the object in a *Walls* collection came in as a real wall, the one marked `ifc_class = IfcColumn` as a column, the rest as meshes with their Blender materials. |
| [09-assign-as-slab.png](09-assign-as-slab.png) | The terrace mesh turned into a slab with *Assign as… > Slab*. It gets slab handles, a thickness and its material, and stays linked to the Blender object. |
| [10-live-reload.png](10-live-reload.png) | The garden wall made longer in Blender and the file saved: CreaBIM reloaded it within a second ("Reloaded garden.blend: 1 updated"). The wall is still a wall and the terrace still a slab. |
| [11-ifc-import.png](11-ifc-import.png) | A Revit structural model (IFC2X3) opened directly: 17 native walls, 9 slabs, 42 columns and 43 beams on two storeys, with plans generated for both. |

## How to try it

- **Handles:** select a wall, slab, door, window or grid. Drag its handles in 3D or in a plan. Snaps show as a marker with a label; hold Shift in plans for orthogonal lines.
- **Booleans:** select a wall, slab or mass, then pick the element to cut away under *Booleans > Subtract…*. *Remove* undoes the cut; the Cut entry in the model list lets you switch it to Union or Intersect.
- **Guides:** pick the Guide tool. Click two points, or click an edge first to draw a guide parallel to it.
- **Redraw:** select a wall or slab and press *Redraw path* or *Redraw outline*. Click the corners; Enter finishes, clicking the first corner closes, Esc cancels.
- **Blender:** type the `.blend` path in the File box (or drop the file on the window) and use *Import and link…*. Name a collection *Walls*, *Slabs*, *Doors* or *Windows*, or give objects a `creabim_kind` property, to have them come in as those elements. To go the other way, *Export glTF (.glb) for Blender* and load it with the add-on in [tools/blender-addon](../../../tools/blender-addon).
- **FreeCAD:** link a `.FCStd` the same way. *Export FreeCAD (.FCStd)* writes each element as a solid with its BIM type.
- **IFC:** *Open* a `.ifc` (IFC2X3, IFC4, IFC4X3) or `.ifcx` (IFC5 alpha). *Export IFC4* and *Export IFC5* write them.
- **Project file:** *Save* writes a `.creabim` file (SQLite) with the undo history.

## Known gaps

- Cut elements export to IFC and glTF as meshes, not as IfcBooleanResult.
- A Blender object made a slab moves to the nearest level's height, since slabs sit on their level.
- If you change an imported element's class in CreaBIM and the Blender file names a different class, the file wins on the next reload.
- In FreeCAD, exported objects are plain solids with their BIM type, not native Arch walls, and curved faces are faceted.
- IFC import: openings are cut only into native walls, curved or sloped walls come in as meshes, spaces are skipped, and NURBS surfaces are not read. IFC5 is still an alpha schema.
- Opening a file uses the File box or drag and drop; there is no file picker yet.
- Slab outlines can be redrawn, but single corners can't be added or removed yet.
