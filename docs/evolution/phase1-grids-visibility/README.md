# Phase 1: grid systems, grids in 3D and per-view visibility

Reference screenshots, taken from the sample house with a second building's grid added beside it.

**Grid systems.** Grid lines can now belong to a *grid system* (a grid group, exported as one IfcGrid). Each system has a name, a prefix ("A-", "B-"), how its lines are counted (north–south lines A, B, C… and east–west lines 1, 2, 3… by default, I and O skipped) and a colour. A line stores only its own label, so it reads "A-1" in building A's system and "B-1" in building B's, and changing the prefix relabels the whole system. The new **Grid** tool draws lines into the active system, and each line takes the next label of its system and direction. Deleting a system keeps its lines, ungrouped, with the labels they showed.

**Grids in 3D.** Grid lines show in the 3D view, from the lowest level to the highest, with a line at every level, posts at both ends and a bubble with the label at each end, in their system's colour. Clicking a bubble selects the line. *View > 3D view shows* turns grids, any single grid system, or any other category off in 3D.

**Visibility per view** (replacing "exclude from sheet"). Each view stores what it leaves out: whole categories (walls, slabs, doors, windows, void cutters, masses, grids, levels, section marks, dimensions, tags, sketches, guides), grid systems, and single elements. A sheet's viewports show their view, so whatever a view hides is hidden on every sheet it is placed on. Hiding goes through the undo history like any edit and is saved with the project.

| Screenshot | What it shows |
| --- | --- |
| [01-grids-in-3d.png](01-grids-in-3d.png) | The 3D view with two grid systems: building A's grid (blue, A-A, A-B, A-1, A-2) on the sample house and building B's grid (orange, B-A to B-C and B-1, B-2) beside it. Lines at both levels, posts at the ends and bubbles at both ends. |
| [02-grid-tool-systems-menu.png](02-grid-tool-systems-menu.png) | The Grid tool picked: its *Grid system* dropdown in the top bar lists the systems with the active one ticked, *New grid system*, *Renumber the lines in order*, *Select its lines* and *Edit the grid system…*. The properties panel shows the active view's Visibility section. |
| [03-move-lines-into-a-system.png](03-move-lines-into-a-system.png) | The sample's four existing grid lines selected (*Select its lines* with *No grid system* active); the dropdown's *Move the 4 selected lines to* submenu puts them in a system. |
| [04-existing-grid-becomes-building-a.png](04-existing-grid-becomes-building-a.png) | After moving them into *Building A grid*: the same lines now read A-A, A-B, A-1, A-2 in the plan and in 3D, without being relabelled one by one. The Visibility panel lists the grid system with its colour. |
| [05-drawing-building-b-grid.png](05-drawing-building-b-grid.png) | Drawing a second building's grid with *Building B grid* active: three north–south lines became B-A, B-B, B-C and two east–west lines B-1, B-2. The last line is selected; its properties show its label after the prefix, its system and how it reads. |
| [06-grid-system-properties.png](06-grid-system-properties.png) | A grid system's properties: prefix, how north–south and east–west lines are counted, colour, its number of lines, and buttons to draw in it, select its lines and renumber them in order (west to east, south to north). |
| [07-3d-view-shows-menu.png](07-3d-view-shows-menu.png) | *View > 3D view shows*: every category and each grid system, ticked when shown. The list stays open while ticking. |
| [08-3d-without-building-b.png](08-3d-without-building-b.png) | Building B's grid turned off in 3D; building A's stays. |
| [09-3d-grids-off.png](09-3d-grids-off.png) | All grids turned off in 3D. |
| [10-view-visibility-menu.png](10-view-visibility-menu.png) | The *Visibility* dropdown of the active view (top bar, right): windows and building B's grid unticked for the ground floor plan. The same settings are in the properties panel (left of the menu), which counts what is hidden. |
| [11-plan-without-windows-and-grid-b.png](11-plan-without-windows-and-grid-b.png) | The ground floor plan with windows and building B's grid left out. The walls keep their window openings, and the window tags go with the windows. The 3D view is not affected. |
| [12-right-click-hide-in-view.png](12-right-click-hide-in-view.png) | Right-click on the partition in the plan: *Hide in view* and *Hide category in view* (and *Hide grid system in view* for grid lines). In the 3D view the same items act on the 3D view. |
| [13-wall-hidden-in-plan.png](13-wall-hidden-in-plan.png) | The partition hidden in this plan, its door with it. The properties panel lists it under *Hidden elements* with an *Unhide* button. Ctrl+Z brings it back like any edit. |
| [14-sheet-follows-view.png](14-sheet-follows-view.png) | Sheet A101: the ground floor viewport (left) shows the plan as its view does, without windows or building B's grid; the first floor viewport (right) shows both. |
| [15-3d-hide-in-view.png](15-3d-hide-in-view.png) | *Hide in 3D view* from the right-click menu in 3D: the first floor's front wall and its windows are left out of the 3D view only; the plan is unchanged. |

## How to try it

- Pick the **Grid** tool (next to Guide). Its *Grid system* dropdown in the top bar makes a new system, picks the active one, renumbers, or moves selected lines into a system. Click two points per line; Shift keeps it straight.
- Select a grid system (dropdown > *Edit the grid system…*, or the project browser under Grids) to change its name, prefix, label styles and colour.
- With nothing selected, the properties panel shows the open view's *Visibility*. The same list is the *Visibility* dropdown at the top right, and selecting a view in the browser shows it too.
- Right-click an element in a plan, section or 3D view: *Hide in view*, *Hide category in view*.
- *View > 3D view shows* for the 3D view's own settings (grids on by default).

## Data and exchange

- `Grid` gains `group: Option<Id>` and the new element kind `GridGroup { prefix, vertical, horizontal, color }`; `View` gains `hidden: ViewHidden { categories, elements, grid_groups }`. All are optional in files, so older projects open unchanged and projects without groups or hidden items save as before. A category from a newer version reads as *Other* instead of failing.
- IFC5 (IFCX): a grid system exports as an IfcGrid node whose children are its lines, exported as IfcGridAxis with their AxisTag; ungrouped lines stay one IfcGrid each. Re-importing gives back the same systems and lines.
- IFC4 import: an IfcGrid with several axes becomes a grid system holding them.

## Known gaps

- The 3D view's own visibility settings last for the session: they are not saved and not undone (views' settings are both).
- A viewport can't override its view's visibility yet; to show something differently on a sheet, place a duplicate view.
- In hidden-line sections and 3D views, a hidden door's or window's panel still hides the lines behind it (the opening stays in the wall on purpose).
- In plans and sections, grid lines are drawn with the usual grid pen, not in their system's colour, and still have a bubble at one end only.
- Grid lines in 3D are one pixel wide and depth-tested; their bubbles are drawn over everything. *Frame all* doesn't include grids.
- IFC4 (.ifc) export still writes no grids, grouped or not (as before).
- The Grid tool draws in plans and on a level in 3D, not in sections.
- The classic egui panels' right-click menu has no *Hide in view* items; the properties panel's Visibility section works there.
- A dimension attached to a hidden element is hidden with it.
