# Phase 1, milestone 1: sections and elevations

Reference screenshots of what this milestone is meant to do, taken from the app running the sample house.

| Screenshot | What it shows |
| --- | --- |
| [01-plan.png](01-plan.png) | Ground floor plan with section marks A and B and elevation marks N, E, S, W. Views are listed in the model list. |
| [02-section-a.png](02-section-a.png) | Section A: cut walls and slabs drawn heavy, the canopy cut on the left, windows beyond in projection, level datums on the right. |
| [03-south-elevation.png](03-south-elevation.png) | South elevation: hidden-line projection with the door, windows and canopy. |
| [05-section-added.png](05-section-added.png) | A new Section C drawn with the Section tool; its properties are editable on the right. |
| [06-section-in-3d.png](06-section-in-3d.png) | *View → Show the 2D view in 3D space* with Section C: the drawing stands on its cut line in 3D. |
| [07-live-edit.png](07-live-edit.png) | A first-floor wall raised to 4.5 m: the section and the 3D view update together. |

## Known gaps

- Wall corners are not cleaned up yet, so corners show doubled lines in elevations and overlapping fills in sections.
- Drawing tools (Wall, Door, Window, Slab, Mass) only work in plan views. In a section or elevation you can select and edit through Properties.
- Section and elevation lines can't be dragged in the plan yet; move them with the Start and End fields.
- No hatching or material patterns in cut faces yet.
- The 2D view shown in 3D is drawn on top of the model rather than depth-tested.
