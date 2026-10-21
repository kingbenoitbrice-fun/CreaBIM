# Phase 1: menu icons, 3D visibility, guides toggle, X-ray, hidden lines, slabs in plan, more tags

Reference screenshots, taken from the sample house (a beam inside the north wall and a gable roof were added for shots 12 to 16).

**Menu icons.** Submenus can carry an icon like any other entry, so *Create › Views › Schedule* now has one (a table), and so do its own entries. Every menu was checked: File (new schedule, open recent, the recent files themselves, save as, reload, the exports), Edit (pivot), View (layout, underlay, 3D view visibility, camera, ground grid, theme) and the Materials and Assemblies windows' buttons. A test walks every menu and fails on any entry without an icon or with an icon the icon set does not have.

**3D view visibility.** The 3D view's *Show* dropdown (its navigation bar) and *View › 3D view visibility* list quick switches first: *All 2D objects*, *Grids*, *Annotation* (dimensions, tags, notes, levels, section marks), *Guides*, *Sketches*, *Void cutters*, *Ground grid*, *The 2D view in 3D space* and *X-ray*, then every category one by one. With nothing selected the properties panel shows the same as checkboxes under *3D view*. What the 3D view hides is remembered between sessions.

**Ground grid spacing, any value.** *View › Ground grid › Custom…* opens a small window where any spacing can be typed in the project's units or another (1250, 1.25 m, 4'-0"), with the usual values as buttons. The menu then reads *Custom: 1250 mm…* with a tick.

**Controls that look like controls.** Buttons, drop-downs, sliders and text fields got a visible fill and outline against the panel they sit on, a stronger hover, and a filled slider track, in every theme (egui windows and the web panes). A test checks that controls stand out from their window in each theme.

**Guides on and off without deleting them.** *View › Guides* (Shift+G) hides all guides in every view, plan, section and 3D; they stay in the model and still exist for undo, and the status bar says how many are kept. Picking the Guide tool shows them again. Each view's Visibility panel also has *Guides in every view*.

**X-ray in 3D.** *X-ray* (Alt+Z, the *Show* menu, the 3D navigation bar button or *View › X-ray*) draws walls, slabs, roofs, masses and openings see-through with their edges, so what sits inside them (a beam in a wall, columns) shows. Framing (columns, beams, stairs, railings and imported IfcBeam/IfcColumn/IfcMember meshes) and the selection stay solid.

**Hidden elements dashed in sections, elevations and plans.** A view's *Hidden* button (top bar) or *Elements hidden inside others, dashed* in its Visibility panel draws, dashed, the edges of elements hidden inside other elements: a beam in a wall in an elevation, a column inside a wall in a plan, a slab edge behind a wall. It is per view, saved with the view, undoable, and sheets show it the way the view does. Doors and windows are left out since their openings already show.

**Slabs in plan.** Slabs on the plan's level are drawn with a light fill, highlight under the pointer and can be clicked to select (and then edited) in the plan.

**Tags on more elements.** The Tag tool tags roofs (thickness, material and pitch, e.g. "250 30°"), columns (profile), beams (width × depth), stairs and railings, besides doors, windows, walls and slabs. Clicking something that cannot be tagged says so in the status bar.

| Screenshot | What it shows |
| --- | --- |
| [01-create-menu-icons.png](01-create-menu-icons.png) | *Create* menu: every entry, including the *Views › Schedule* submenu, has an icon. |
| [02-schedule-submenu-icons.png](02-schedule-submenu-icons.png) | *Create › Views › Schedule*: Doors, Windows, Walls, Slabs, Columns and beams, each with its icon. |
| [03-3d-show-menu.png](03-3d-show-menu.png) | The 3D view's *Show* dropdown in its navigation bar: quick switches (All 2D objects, Grids, Annotation, Guides, Sketches, Void cutters, Ground grid, The 2D view in 3D space, X-ray) above the categories. The properties panel shows the same under *3D view*. |
| [04-3d-show-menu-grids-off.png](04-3d-show-menu-grids-off.png) | Grids and void cutters unticked: the grid bubbles and the cutter boxes are gone from 3D; the plan is not affected. |
| [05-3d-grids-and-cutters-off.png](05-3d-grids-and-cutters-off.png) | The same with the menu closed; *Visibility (2 hidden)* in the properties panel. |
| [06-view-menu.png](06-view-menu.png) | *View* menu: *3D view visibility*, *X-ray* (Alt+Z), *Ground grid*, and under *Every view* the *Guides* toggle (Shift+G) and *2D view visibility*. |
| [07-ground-grid-menu.png](07-ground-grid-menu.png) | *View › Ground grid*: the preset spacings and *Custom…*. |
| [08-ground-grid-custom.png](08-ground-grid-custom.png) | The *Ground grid spacing* window with 1250 typed in. |
| [09-ground-grid-1250.png](09-ground-grid-1250.png) | The ground grid every 1250 mm. |
| [10-guides-shown.png](10-guides-shown.png) | Two guides (dashed magenta) in the plan and in 3D. |
| [11-guides-hidden.png](11-guides-hidden.png) | After Shift+G: guides hidden in every view; the status bar says "Guides hidden in every view (2 kept)". |
| [12-xray-beam-in-wall.png](12-xray-beam-in-wall.png) | X-ray on: walls and slabs see-through with their edges, the beam inside the north wall (selected) and the pergola solid. |
| [13-elevation-hidden-dashed.png](13-elevation-hidden-dashed.png) | North elevation with *Hidden* on: the beam inside the wall, the inner walls and the first floor slab behind the facade, dashed. |
| [14-plan-slab-hover.png](14-plan-slab-hover.png) | Ground floor plan: the floor slab highlighted under the pointer. |
| [15-plan-slab-selected.png](15-plan-slab-selected.png) | The slab selected in the plan with its properties. |
| [16-roof-tag.png](16-roof-tag.png) | Section A: the Tag tool on the gable roof gives "250 30°" (thickness and pitch; the roof has no material). |
| [17-controls-dark.png](17-controls-dark.png) | Materials window, Dark theme: fields, drop-downs, sliders and buttons with visible fills and outlines. |
| [18-controls-light.png](18-controls-light.png) | The same in Light. |
| [19-controls-paper.png](19-controls-paper.png) | The same in Paper. |
| [20-controls-dark-window.png](20-controls-dark-window.png) | The ground grid window and the web panes in Dark. |

## How to try it

- Open *Create › Views*, *File*, *View*: every entry has an icon.
- 3D view: click *Show* in its navigation bar, or *View › 3D view visibility*. Untick *Grids* or *Annotation*. Restart: it is remembered.
- *View › Ground grid › Custom…*, type `1.25 m` or `4'`, Enter.
- Draw a guide or two (Guide tool), then Shift+G or *View › Guides*. Shift+G again, or pick the Guide tool, brings them back.
- Draw a beam along a wall with its top inside the wall. Alt+Z for X-ray in 3D. Open an elevation and click *Hidden* in the top bar.
- In a floor plan, move over a slab and click it.
- Tag tool: click a roof, column or beam in a plan or section, then where the tag goes.

## Data and exchange

- `View` gains `show_hidden: bool`, optional in files (left out when off), so older projects open unchanged and save as before. IFC export is not affected.
- The app's settings file gains `hidden_3d`, `xray` and `guides_hidden`, all optional; older settings files load as before.

## Known gaps

- Single elements hidden in 3D (right-click *Hide in 3D view*) last for the session only; the categories and quick switches are remembered. 3D visibility is per user (app settings), not saved in the project.
- X-ray transparency is not depth-sorted, so overlapping see-through faces can blend in a slightly different order as the camera turns.
- In sections, where a wall's cut already has a beam's cut subtracted from it, the beam's cut is drawn normally and not dashed (only covered lines and projected edges are revealed).
- The classic egui menus (*View › Classic panels*) still show text only; the icons are in the web menus.
- Roof tags point to the centre of the roof's bounding box, which for pitched roofs is inside the attic.
