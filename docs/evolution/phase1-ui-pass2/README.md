# Phase 1: interface pass 2

Reference screenshots for the second interface pass, taken from the sample house.

This pass:
- fixes Move, which made things jump when it was started from a button or a menu;
- adds the themes and a few ideas from the UI template (a Create menu, a command palette);
- makes the toolbar much smaller and the side panels resizable;
- gives the 3D view navigation buttons;
- makes section and elevation marks easy to find and to resize;
- tidies the schedules.

| Screenshot | What it shows |
| --- | --- |
| [01-overview.png](01-overview.png) | The whole window. The tool row is about half as tall as before: each tool is an icon (its name shows in a tooltip), and the tool in use also shows its name. The 3D view has its navigation buttons at its top right. |
| [02-create-menu.png](02-create-menu.png) | The new *Create* menu next to *View*: building elements, views (section, elevation, sheet, schedules), annotation and sketching. It lists the same tools as the tool row, with their names, so nothing has to be found by its icon. |
| [03-palette.png](03-palette.png) | The command palette (Ctrl+K, or *View > Command palette*). Typing "elev" finds the Elevation tool and every elevation view. ↑ and ↓ choose, Enter runs it. It searches every menu item, every tool and every view and sheet. |
| [04-move-start.png](04-move-start.png) | A wall selected, then *Move* clicked in the top bar, and the pointer brought into the 3D view near the horizon. The move reads 0.000 m: nothing has moved yet. Before this fix the wall jumped by tens of metres at this point. |
| [05-move-follows.png](05-move-follows.png) | Moving the pointer a little moves the wall a little. The move is now measured where the wall is, so it follows the pointer the same way wherever the pointer came into the view. |
| [06-elevation-depth.png](06-elevation-depth.png) | The south elevation selected by clicking its "S" bubble (the bubble and the arrow now pick it). Its line is drawn bold and its depth, how far it sees, is shaded, in the plan and in 3D. Square handles move the line's ends; the round handle on the far edge was dragged to change the depth (11 m). Sections work the same way. |
| [07-snaps.png](07-snaps.png) | The snap list with a marker beside each kind: endpoint square, midpoint triangle, intersection cross, perpendicular, nearest, centre, quadrant, grid, guides, axis tracking and increment. |
| [08-look-from.png](08-look-from.png) | The 3D view's buttons: *Frame all* (Home), *Frame the selection* (.), *Orbit around the selection*, zoom in and out, perspective or orthographic. *View* looks from the top, front, right, back, left or the corner. The same commands are in *View > 3D view*. |
| [09-top-ortho.png](09-top-ortho.png) | Looking from the top in orthographic. |
| [10-themes.png](10-themes.png) | The themes from the template: Graphite, Paper, Blueprint, Sage and Ink (top to bottom, left to right), with the original dark theme last. They are under *View > Theme*, and Ctrl+Shift+T goes to the next one. The properties panel and the in-view controls follow the theme. The choice is remembered for next time. |
| [11-schedule.png](11-schedule.png) | A schedule on light paper, whatever the theme. It has a title and a count, with *Export CSV* on the title line. Column headers are on a band. Rows are lightly striped and the selected row is highlighted. Each column is as wide as its widest cell, numbers are right-aligned, and wide tables scroll sideways. |
| [12-resized.png](12-resized.png) | The project browser and the properties panel made wider by dragging their inner edges. Their widths are remembered with the theme. |

## How to try it

- Select something, click *Move* in the top bar (or in the right-click menu), then move the pointer into a view: nothing jumps.
- Press Ctrl+K and type part of a command, a tool or a view.
- Use *View > Theme* or Ctrl+Shift+T, then close and reopen the app.
- Drag the edge between the browser and the views, or between the views and the properties.
- Click a section or elevation mark (its arrow or its bubble) in a plan and drag the round handle on the shaded band.
- Home frames everything, `.` frames the selection.

## Known gaps

- The palette lists actions only. It doesn't search elements or properties yet.
- When the palette is opened with Ctrl+K, typing reaches it through the app window. A click in its box also works.
- *Orbit around the selection* turns about the middle of the selection's box, and only in the 3D view.
- The navigation buttons sit over the 3D view and cover a small part of it. In the overlaid layout they move to the top left.
- The orthographic camera has no clipping control, and sheet viewports of 3D views keep their own projection.
- The project browser's width is remembered, but it has no collapse button yet.
- Schedules still can't be sorted or filtered by clicking a header.
