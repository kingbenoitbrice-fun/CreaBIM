# Phase 1: Move by base point, menus that fit, tool strip, one-view layout, 3D ground grid

Reference screenshots of the sample house in a 1400×860 window, plus two at 1280×720.

**Move by base point.** Move (M, the toolbar button, Edit menu or right-click) no longer grabs the selection and sends it after the pointer. If nothing is selected it says what to do, and Move is armed for the next selection. The flow has three steps:
1. *Base point.* A snap marker shows under the pointer. Nothing moves before this click. Shift+click adds or removes elements here.
2. *Follow.* The selection follows the pointer by the vector from the base point, with a dashed rubber band that shows its length. The destination snaps to ends, middles, crossings and grid lines; the moving elements are left out of the snaps, so they never snap to themselves. Shift keeps it square. X, Y or Z keeps it on that axis. Typing a distance and pressing Enter moves it that far in the direction the pointer points; the distance is in metres, and `mm` and `cm` suffixes also work. Ctrl toggles *Copy*, and then every click leaves a copy behind.
3. *Place.* Click. Esc or right-click steps back one stage (from following to choosing the base point again), then cancels. A move is one undo step, and each placed copy is one undo step.

It works the same in plans, sections and 3D; in 3D the move stays on its level unless Z is locked. G, the Blender-like grab, is still there, but it now starts at the snapped point under the pointer, without a jump, and nothing moves until the pointer has gone 4 px. **Rotate** uses the same base-point flow: centre, reference direction, new direction, all snapped.

**Drag-move.** Pressing on a selected element and dragging it moves it, in plans and 3D. Its grab point snaps (a wall picked near its end grabs the end), and the destination snaps too. Nothing moves within 4 px, so a click doesn't move anything.

**Menus that fit.** Popup menus are never taller than the window: a long menu scrolls inside itself, with a scrollbar and a ⌄ at the bottom while more is below. The Create menu shows in three columns (Building elements | Views | Annotation and Sketching), so it fits without scrolling. Arrow keys, Enter, Left/Backspace (back out of a submenu) and Esc work in every menu, and the command palette (Ctrl+K) is unchanged.

**Tool strip.** The active tool and its options now have a strip of their own, under the tools. It holds the wall thickness, the slab's Polygon / Rectangle / Fill room / From sketch and Line / Arc, sketch shapes, member options, the grid tool's grid system, and Move/Rotate while they run. The view options (cut height, tag openings, visibility) stay on the right of the strip. Tool options no longer float over the 3D navigation bar.

**One view at a time.** *View › One view at a time*, or the toggle left of the view switcher, shows a single large view, with tabs above it: *3D view* and the open drawing (its ⏷ lists the other views). *Split* goes back to 3D beside the drawing. Opening a view from the browser or the view switcher shows the drawing tab. The choice and the last tab are kept in `settings.json`.

**3D ground grid.** The grid on the ground can be turned off in *View › 3D view › Ground grid* or with the grid button in the 3D navigation bar, and its spacing chosen (0.25 to 10 m) in *View › 3D view › Ground grid spacing*. Both are kept in `settings.json`. It is drawn at the work level's height and grows with the spacing. Structural grid lines can be picked in 3D by a bubble or a line. A picked line can then be dragged, by its bubble or a line, square to itself, snapping to points that line up on screen. *View › 3D view shows › Grids* hides them in 3D.

| Screenshot | What it shows |
| --- | --- |
| [01-move-base.png](01-move-base.png) | M with the partition selected: the base-point step. The snap marker sits on its end, labelled *Endpoint*, and the status bar says what to do. The Move panel offers Free / X / Y / Z, a distance field and Copy (Ctrl); Finish waits for a base point. Nothing has moved. |
| [02-move-follow.png](02-move-follow.png) | After the base-point click: the partition (and its door) follows the pointer, with a dashed rubber band reading 3.118 m from the base point. The 3D view previews it too. |
| [03-move-snapped.png](03-move-snapped.png) | The destination snapped onto the end of the front wall beside the entrance (*Endpoint*, 3.340 m): the base point lands exactly there. |
| [04-move-copy.png](04-move-copy.png) | Ctrl turned Copy on: one copy is already placed (middle left), and another follows the pointer. The original stays where it was. |
| [05-move-3d.png](05-move-3d.png) | Moving the first floor's front wall in the 3D view from a snapped corner: it follows the pointer on its level, with the rubber band and its length. |
| [06-drag-move.png](06-drag-move.png) | Dragging the selected partition in the plan: it moves 1.850 m and keeps to the X axis (*On X axis*), shown with a red guide. |
| [07-create-menu.png](07-create-menu.png) | The Create menu in three columns, with Slab picked from the keyboard (arrow keys). |
| [08-create-menu-720.png](08-create-menu-720.png) | The same menu in a 1280×720 window: it still fits. |
| [09-view-menu-720.png](09-view-menu-720.png) | The View menu in a 1280×720 window: no taller than the window, it scrolls, with a scrollbar and a ⌄ showing more below. The new items are here too: *One view at a time*, *Split: 3D and drawing*, *Ground grid* and *Ground grid spacing*. |
| [10-tool-strip.png](10-tool-strip.png) | The Slab tool: its options (Polygon (1), Rectangle (2), Fill room (3), From sketch (4), Line (L), Arc (A)) sit in the strip under the tools, clear of the 3D navigation bar. |
| [11-single-3d.png](11-single-3d.png) | One view at a time, on the 3D tab: the 3D view gets the whole centre. |
| [12-single-plan.png](12-single-plan.png) | The same layout on the *Ground Floor plan* tab. |
| [13-ground-grid-off.png](13-ground-grid-off.png) | The ground grid turned off from the navigation bar's grid button. |
| [14-grid-drag-3d.png](14-grid-drag-3d.png) | Grid line 1 picked by its bubble in 3D and dragged square to itself (2.963 m, snapped to an endpoint). Its lines at both levels and both bubbles go with it. |
| [15-rotate.png](15-rotate.png) | Rotate by points: centre at the partition's foot and reference along the partition, then turned to the snapped corner of the east wall (−49.6°). |

## How to try it

- Select something and press **M**, or click the Move button. Click a base point, then click where it goes. While it follows the pointer:
  - type `2` and press Enter to move it 2 m the way the pointer points (`450mm` works too);
  - X, Y or Z keeps it on that axis, and Shift keeps it square;
  - Ctrl leaves copies.

  Esc steps back, and a second Esc cancels.
- With nothing selected, press M, then pick what to move.
- Hover a view and press **G** for the quick grab, which starts at the snapped point under the pointer.
- Drag a selected element to move it.
- Rotate (toolbar or Edit menu): click the centre, a reference direction, then the new direction.
- Open Create or View in a small window: the menu stays inside the window and scrolls. Use the arrow keys and Enter to pick.
- Pick Slab, Wall, Sketch or Grid and look at the strip under the tools.
- Use *View › One view at a time* (or the square button left of the view switcher). Switch with the tabs, and go back with *Split*.
- *View › 3D view › Ground grid* or the grid button in the 3D navigation bar; *Ground grid spacing* to change the spacing.
- In 3D, click a grid bubble, then drag the bubble or the line.

## Known gaps

- Typed distances are metres (with mm and cm suffixes). There is no project units setting yet.
- Drag-move moves the element under the pointer, not the whole selection; Move (M) moves all of it.
- Moving a wall doesn't stretch the walls joined to it, so it can leave a gap at a corner.
- Scale (S) still works the old way, by pointer distance, without base points.
- Rotate by points turns about the vertical in plans and 3D; in a section it tilts about the axis facing the viewer.
- A grid line dragged in 3D snaps only to points that line up with it on screen, so a point at another height can still be missed.
