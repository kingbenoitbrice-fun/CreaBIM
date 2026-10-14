# Phase 1, workflow pass: slabs, workplanes, snaps and the mouse

Reference screenshots for the fixes asked for after milestone 6, taken from the sample house.

| Screenshot | What it shows |
| --- | --- |
| [01-grid-stays-when-slab-thickens.png](01-grid-stays-when-slab-thickens.png) | The ground slab set to 1.50 m thick. Its top stays on the level and it grows down; the 3D grid stays on the working level instead of dropping under the lowest object. Properties now has *Keep: Top / Bottom* (which face stays when the thickness changes), a *Top offset*, and the top and underside heights. |
| [02-right-click-menu.png](02-right-click-menu.png) | Right-click on the south wall in 3D with the Select tool: it is selected and a menu offers Move, Rotate, Scale, Duplicate, Copy, Cut, Delete, Hide, Show only selected, Paste, Show everything, Select all, Select nothing, *Place pivot cursor here* and *Set workplane here*. The same menu opens in plans and sections. |
| [03-move-panel.png](03-move-panel.png) | *Move* picked from the menu (or the new edit row under the tools). A panel at the bottom of the view has axis buttons (Free, X, Y, Z), a value box, Finish and Cancel, so a move, rotate or scale can be done without the keyboard. Here Y is picked and 2 typed. |
| [04-workplane-on-a-face.png](04-workplane-on-a-face.png) | *Workplane > Pick a face…* then a click on the south wall: an orange grid shows the workplane on the wall. *Through three points…* and *Level plane you can offset* are in the same list, and *Offset ±* slides it along its normal. |
| [05-sketch-on-the-workplane.png](05-sketch-on-the-workplane.png) | The Sketch tool draws on the workplane: a closed sketch on the wall face (its plane faces south). E extrudes it as before. |
| [06-guide-on-the-workplane.png](06-guide-on-the-workplane.png) | The Guide tool in 3D also draws on the workplane: a guide across the wall face. |
| [07-guide-in-a-section.png](07-guide-in-a-section.png) | Guides can now be drawn in sections: this one lies on Section A's plane and shows in the section and in 3D. Sketches already worked in sections. |
| [08-slab-with-an-arc.png](08-slab-with-an-arc.png) | The Slab tool's new bar: *Polygon*, *Rectangle*, *Fill room*, *From sketch*, and *Line / Arc* while drawing. This slab was clicked corner by corner with one arc (A, then the arc's end and a point it passes through). Properties: 5 corners, 1 arc, 55.13 m². |
| [09-slab-fills-a-room.png](09-slab-fills-a-room.png) | *Fill room*, then a click inside the east room: a 24 m² slab to the walls' centre lines. |
| [10-snap-list.png](10-snap-list.png) | The *Snap* toggle (F3) and its list: endpoint, midpoint, intersection, perpendicular, nearest on edge, centre, quadrant, grid and level lines, guides, axis tracking and 5 cm steps, with All and None. Holding Alt suspends snapping while you click. |
| [11-dimension-on-a-mass.png](11-dimension-on-a-mass.png) | A dimension drawn on two corners of a mass, which then was scaled by 1.5: the dimension follows and reads 3000. Dimensions now attach to masses, slabs, grids, columns, sketches and levels, in plans and sections. |

## How to try it

- **Leave a tool:** right-click or Esc once drops the clicks so far, a second time goes back to Select. A sketch or outline with points is finished by right-click, as before.
- **Without a keyboard:** the row under the tools has Undo, Redo, Move, Rotate, Scale, Duplicate, Copy, Cut, Paste, Delete, Hide, Show only selected, Show everything, Select all and Select nothing. After Move, Rotate, Scale or Duplicate, point at a view: the edit follows the pointer and the panel gives axes, a value, Finish and Cancel.
- **Workplane:** pick it in the toolbar list, or right-click a face and choose *Set workplane here*. Choose *Level (follows the plan)* to go back.
- **Slabs:** pick the Slab tool, then a mode in the bar (or 1 to 4). In Polygon, A and L switch between arcs and lines; click the first corner or press Enter to close.
- **Snaps:** click *Snap* to switch all snapping off or on (F3), or untick kinds in the list next to it.

## Known gaps

- The new interface in Tauri comes next: these screens are still the old interface, with a toolbar that runs off the window on narrow screens.
- Drawing a slab inside a section is still the two-corner rectangle with its top on the nearest level.
- Fill room has no preview before the click, and goes to wall centre lines only (not the inner faces).
- Move, rotate and scale don't snap the moved selection onto other objects; they keep their Ctrl steps.
- A dimension on a slab follows moves and turns but not edits to the slab's first two corners; a dimension on a mass doesn't follow edits to its vertices.
- Walls, doors, windows and masses still draw on the level in 3D, not on a custom workplane; only sketches and guides use it.
- The workplane isn't saved with the project.
