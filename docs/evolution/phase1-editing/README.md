# Phase 1, milestone 6: everyday editing

Reference screenshots of what this milestone is meant to do, taken from the sample house.

| Screenshot | What it shows |
| --- | --- |
| [01-box-in-plan.png](01-box-in-plan.png) | A box dragged left to right on empty paper in the plan. Solid blue means it takes only what lies wholly inside. Dragged right to left it turns green and dashed, and takes what it touches. |
| [02-box-result.png](02-box-result.png) | The result: "5 selected: 2 windows, 2 walls, 1 tag", highlighted in the plan and in 3D. Properties shows the last one picked. Shift+click adds or removes one element; Ctrl+A selects everything. |
| [03-move-many.png](03-move-many.png) | G moves all of them at once. The tag can't be moved this way and stays put, and the status bar says so. The windows go with their walls. |
| [04-hide.png](04-hide.png) | H hides the selection (here the south wall, with its door and window) in 3D and in the plan, so you can work behind it. Sheets and exports still show everything. |
| [05-isolate.png](05-isolate.png) | Shift+H shows only the selection: the front wall and the pergola. Alt+H shows everything again. |
| [06-tilt-a-mass.png](06-tilt-a-mass.png) | R then X tilts a mass 30° about the X axis. Y tilts about Y. In a section, R tilts about the axis you look along. Walls and slabs stay upright and are skipped. |
| [07-stretch-on-one-axis.png](07-stretch-on-one-axis.png) | S then Z scales the mass to twice its height without changing its footprint. A stretched wall changes its height; a stretched slab changes its thickness. |
| [08-pivot-cursor.png](08-pivot-cursor.png) | Shift+right-click places the 3D cursor (here at the north-east corner), and the pivot switches to *Pivot cursor*. R then turns the south wall about it. The *Pivot* list in the toolbar also has median point, bounds centre, each element's own origin and the active element. |
| [09-slab-corner-added.png](09-slab-corner-added.png) | Double-click a slab's edge handle to add a corner there, then drag it. The slab now has 5 boundary points. Double-click a corner to take it away. |
| [10-copy-and-paste.png](10-copy-and-paste.png) | Ctrl+C on a wall, Ctrl+V pastes a copy in place with its door and window, and G moves it. The clipboard is text, so a copy also pastes into a second CreaBIM window. Ctrl+X cuts. |
| [11-sketch-arc-and-constraint.png](11-sketch-arc-and-constraint.png) | A sketch drawn with lines and an arc (press A, click its end, then a point it passes through), then closed. A *Length* constraint of 3.00 m on its first edge was added in Properties; the sketch moved to meet it, and the value shows on the edge. |
| [12-circle-extruded.png](12-circle-extruded.png) | A circle (press C, click the centre, then a point on it) extruded with E into a cylinder. |
| [13-box-in-3d.png](13-box-in-3d.png) | In 3D, press B, then drag. This is a right-to-left box, so it takes what it touches. It tests each element's real triangles, not its bounding box, so it doesn't pick the house. |
| [14-box-in-3d-result.png](14-box-in-3d-result.png) | The result: the pergola's posts and beams. |
| [15-open-recent.png](15-open-recent.png) | *File > Open…* (Ctrl+O) and *Save as…* use the system file picker. *Open recent* lists the last ten files. |

## How to try it

- **Select many:** Shift+click in 3D, plans, sections or the model list. Drag on empty paper in a plan or section for a box; in 3D press B first. Hold Shift to add to what's selected.
- **Edit many:** Delete, G, R, S, Shift+D, Ctrl+C, Ctrl+X and H act on everything selected. Properties edits the last one picked.
- **Pivot:** choose it in the toolbar or under *Edit > Pivot*. Shift+right-click anywhere sets the cursor.
- **Sketch shapes:** in the Sketch tool, L, A and C switch between lines, arcs and circles. Select a sketch to add constraints in Properties: length, equal, horizontal, vertical, parallel, perpendicular, coincident or radius. Drag a constraint's value and the sketch follows.

## Known gaps

- Dimension constraints are set in Properties only. You can't drag a dimension in the viewport yet, and there are no tangent, angle or fixed-point constraints.
- A sketch is a single profile: no holes, and a circle is a sketch of its own.
- With many elements selected, Properties edits only the active one, and its Delete button counts only that element's attached items.
- Paste is in place only. There is no "paste at cursor" or paste onto another level yet.
- Hidden elements aren't saved with the project; they all show again when you reopen it.
- The *Bounds centre* pivot uses each element's origin, not its full geometry.
- On Linux the file picker needs xdg-desktop-portal. Without it, type a path in the File box as before.
- Tags and dimensions can be box-selected and deleted together with the rest, but they can't be moved with G.
