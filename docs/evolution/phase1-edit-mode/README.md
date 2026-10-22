# Phase 1, Edit mode: reshaping elements, shafts in every view, curtain walls in walls, render modes

These are reference screenshots for the round of 22 October. Shots 01–03 and 16 come from the typed sample house; the rest come from the sample house.

| Screenshot | What it shows |
| --- | --- |
| [01-shaft-edit-mode.png](01-shaft-edit-mode.png) | The stair shaft selected on the Ground Floor plan, then put in Edit mode (Properties *Edit*, Enter, or a double-click). Every corner gets a square grip and every edge gets a round push grip plus a diamond at its middle to add a corner. The grips in its middle move the base and the top. The grips show in the plan and in 3D. |
| [02-shaft-corner-pulled-out.png](02-shaft-corner-pulled-out.png) | An edge's diamond was dragged out, so the shaft now has a fifth corner. The holes it cuts in the slabs follow. Ctrl+Z undoes it. |
| [03-shaft-edited-from-first-floor.png](03-shaft-edited-from-first-floor.png) | The First Floor plan, at the shaft's top level. The shaft is drawn there now, and a double-click on it selects it and opens Edit mode with the same grips. Before this change it was highlighted only on its base level. |
| [04-render-menu.png](04-render-menu.png) | The new render menu in the 3D view's toolbar: Shaded, Shaded with edges, Hidden line, Wireframe. The same list is under View › Render. |
| [05-render-modes-edges-hidden-wire.png](05-render-modes-edges-hidden-wire.png) | The 3D view in Shaded with edges, Hidden line and Wireframe, from left to right. |
| [06-section-style-menu.png](06-section-style-menu.png) | Drawn views (sections, elevations, 3D views) now have a *Style* menu in the bar and in Properties: Hidden line, Shaded with edges, Wireframe. It replaces the Shaded tick box. |
| [07-drawn-3d-view-wireframe.png](07-drawn-3d-view-wireframe.png) | The 3D View South-East set to Wireframe: every edge is drawn, including the ones behind other elements. |
| [08-sheet-follows-wireframe.png](08-sheet-follows-wireframe.png) | The sheet that holds that 3D view prints it in wireframe too. |
| [09-curtain-wall-cuts-its-opening.png](09-curtain-wall-cuts-its-opening.png) | A curtain wall drawn on the south wall's centre line. It cuts its own opening there, like a door, shown in the plan and in 3D. Properties says *Set in GF Wall 1: it cuts its own opening there…*. |
| [10-curtain-wall-moved-opening-follows.png](10-curtain-wall-moved-opening-follows.png) | The same curtain wall dragged along the wall: the opening moves with it and the old gap closes. |
| [11-xray-icon.png](11-xray-icon.png) | X-ray turned on, with its new icon (a dashed box with a beam seen through it). It no longer looks like any of the tool icons in the row above. |
| [12-slab-edit-mode.png](12-slab-edit-mode.png) | The First Floor slab in Edit mode: corner, edge and add-corner grips on its outline, in the plan and in 3D, with the hint in Properties. |
| [13-slab-hole-corner-dragged.png](13-slab-hole-corner-dragged.png) | A hole drawn in that slab (*Draw a hole*). Edit mode shows its corners and edges too, and one corner has been dragged. [13b-slab-hole-grips-closeup.png](13b-slab-hole-grips-closeup.png) is a close-up. |
| [14-dropdown-opens-upward.png](14-dropdown-opens-upward.png) | A dropdown at the bottom of Properties (*Booleans › Subtract…*) opens upward and scrolls instead of running off the window. |
| [15-status-menu-opens-upward.png](15-status-menu-opens-upward.png) | Menus in the web bars flip upward the same way when there is no room below (here the *Pivot* list in the status bar). |
| [16-shaft-picked-and-edited-in-section.png](16-shaft-picked-and-edited-in-section.png) | Section A: the shaft is drawn dashed in sections, elevations and drawn 3D views. A click on it picks it, and Enter opens Edit mode with its grips in the section and in 3D. |

## How to try it

- **Edit mode:** select an element, then click *Edit* in Properties, press Enter, or double-click it. Esc, Enter, or selecting something else finishes. Edit mode works on slabs and their holes, roofs and their holes, shafts, wall openings and sketches.
  - Drag a square corner grip to move that corner.
  - Drag a round edge grip to push that edge.
  - Drag or click a diamond to add a corner.
  - Double-click a corner to take it away.
- **Shafts:** pick one in any plan from its base level to its top level, in a section, in an elevation or in 3D. Use Tab or X-ray when it is hidden inside walls. Its middle grips move its base and top.
- **Curtain walls in walls:** draw a curtain wall (Create › Curtain wall) along an existing wall, or move one into a wall. It cuts its own opening there. IFC export writes the opening as an `IfcOpeningElement` that voids the host wall, filled by the curtain wall (`IfcRelFillsElement`). A curtain wall that is not in a wall stands on its own as before.
- **Render modes:** use the cube menu in the 3D view's toolbar or View › Render. For sections, elevations and saved 3D views, use the *Style* menu in the bar or in Properties. Sheets print what *Style* says.

## Known gaps

- The live 3D window's render mode is a per-user setting. It is not saved with each 3D view: saved views and sheets use the view's *Style*.
- A curtain wall's opening is worked out from where it stands (parallel to the wall and inside its thickness), not stored. It is written to IFC 4 but not yet to IFC5/ifcx.
- Nothing stops a curtain wall's opening from overlapping a door or window in the same wall (shot 10 touches W01).
- On sketches with arcs or constraints, Edit mode only moves corners. Adding or removing corners needs plain line sketches.
- Edge push grips sit 16 px beside the add-corner diamond, so both can be grabbed on short edges.
- The 3D view's toolbar is cut off at its right end when the 3D view is very narrow.
