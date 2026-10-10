# Phase 1, milestone 4: modifier stack, sketches, move, rotate and stretch

Reference screenshots of what this milestone is meant to do, taken from the sample house. The sample now has a timber pergola in the garden, built from two cuboids and Array modifiers.

| Screenshot | What it shows |
| --- | --- |
| [01-array-modifier.png](01-array-modifier.png) | The pergola rafters selected. Properties shows their Array modifier (7 copies, 0.5 m apart). Changing the count or offset updates 3D and the plan as you drag the value. |
| [02-move-in-3d.png](02-move-in-3d.png) | G with the pointer over the 3D view moves the rafters, here kept to the X axis by pressing X. The status bar shows the distance; type a value to set it exactly. |
| [03-rotate-in-plan.png](03-rotate-in-plan.png) | R in the plan turns the rafters about their origin, following the pointer. Typing 30 and Enter turns them exactly 30°. |
| [04-stretch-wall-grip.png](04-stretch-wall-grip.png) | A selected wall in the plan shows square grips at its ends. Dragging one stretches the wall, and the joined north wall follows so the corner stays closed. |
| [05-sketch-on-wall-face.png](05-sketch-on-wall-face.png) | The Sketch tool in 3D: clicking the upper west wall sets a workplane on that face (dashed square). Each click adds a corner; the readout shows the length of the line being drawn. |
| [06-extrude-sketch.png](06-extrude-sketch.png) | Clicking the first corner closed the sketch. E extruded it 1 m out of the wall into a mass with its far face selected, and E again pushed it another metre. |
| [07-mirror-and-subdivision.png](07-mirror-and-subdivision.png) | The same mass with a Mirror modifier (across Y, at −2 m) and a Subdivision modifier on top. The face picked before still highlights on both halves. |
| [08-duplicate.png](08-duplicate.png) | After applying the Mirror (it becomes part of the mesh) and scaling with S by 0.5, Shift+D copied the mass and moved the copy to the ground in front of the house. |
| [09-sketch-in-plan.png](09-sketch-in-plan.png) | A closed sketch drawn in the ground floor plan and extruded with E into a 1 m mass (off screen in 3D, north-west of the house). |
| [10-solidify-open-sketch.png](10-solidify-open-sketch.png) | An open sketch (Enter after the third corner) extrudes into a surface. A Solidify modifier gives it 0.2 m of thickness, seen in the plan and in 3D (red, right). |

## How to try it

- **Modifiers:** select a mass, then *Add modifier…* in Properties. Tick a modifier to turn it off, use Up and Down to reorder, Remove to drop it, and Apply (on the top one) to bake it into the mesh.
- **Sketch:** pick the Sketch tool. In 3D, click a face (or the ground) to start; in a plan or section, click the drawing. Click the first corner to close the sketch, or press Enter or right-click to leave it open. Shift keeps lines square to the workplane. Select a finished sketch and press E to extrude it by *Extrude by* in Properties.
- **G, R, S, Shift+D:** select an element, put the pointer over a view and press the key. Move the pointer or type a value, press X, Y or Z to keep to an axis, hold Ctrl to snap. Click or Enter finishes, Esc or right-click cancels. The whole edit undoes as one step.
- **Grips and dragging in plans:** select a wall, slab or grid to see its square grips (wall ends, slab corners and edge midpoints, grid ends). Drag the element itself to move it. Selected elements can also be dragged in sections.

## Known gaps

- R turns about the vertical axis only. Masses can't be tilted yet, and rotating in a section is refused.
- S scales evenly in every direction. Walls and slabs scale in plan and keep their heights; there is no scaling along one axis.
- Rotate and scale work about the element's own origin (for a mass made from a sketch, the sketch's first corner). There is no way to pick a different pivot yet.
- G, R and S work on one element at a time. There is no multiple selection yet.
- Grips are in plans only. 3D and sections have no grips, and masses have none (use face extrude and Move for those).
- Stretching snaps to a 5 cm grid and to wall ends, not yet to other edges or midpoints.
- Sketches are straight lines only, with no arcs, circles, constraints or dimensions. A sketch is used up when you extrude it, so the mass doesn't remember it.
- A workplane is set by clicking a face. To move it elsewhere, edit Plane X, Y and Z or press G; you can't yet set one by three points or offset it from a face.
- Modifiers are for masses only, not walls or slabs. Mirror works across the mass's own axes. Modifier values are edited in Properties, not with handles in 3D.
- Masses show in plans as projected edges only. They aren't cut or hatched at the cut height yet.
- Shift+D on a door or window places the copy beside the original on the same wall.
