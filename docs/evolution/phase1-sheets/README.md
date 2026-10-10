# Phase 1, milestone 2: sheets, PDF, joins and hatching

Reference screenshots of what this milestone is meant to do, taken from the sample house.

| Screenshot | What it shows |
| --- | --- |
| [01-pdf-floor-plans.png](01-pdf-floor-plans.png) | Sheet A101 from *File → Export sheets as PDF*: both plans at 1:100, mitred wall corners, the partition trimmed to the wall it meets, diagonal hatching on cut walls, view titles and the title block. |
| [02-pdf-sections-elevations.png](02-pdf-sections-elevations.png) | Sheet A201: two sections (slabs cross-hatched, walls hatched) and two elevations with single clean lines at the corners. |
| [03-pdf-3d-view.png](03-pdf-3d-view.png) | Sheet A001: a hidden-line 3D view at 1:50. |
| [04-sheet-in-app.png](04-sheet-in-app.png) | The same sheet in the app, drawn by the same code as the PDF, beside the live 3D view. |
| [05-edit-through-sheet.png](05-edit-through-sheet.png) | Double-clicked the ground floor plan on the sheet and placed a door with the Door tool, right on the sheet. |
| [06-orbit-3d-on-sheet.png](06-orbit-3d-on-sheet.png) | Double-clicked the 3D view on its sheet and dragged to orbit it. |
| [07-live-edit-from-sheet.png](07-live-edit-from-sheet.png) | Clicked a wall in the sheet's 3D view (it highlights in the 3D workspace too) and raised it to 4.5 m: sheet and 3D update together. |
| [08-move-view-on-sheet.png](08-move-view-on-sheet.png) | Dragged the south elevation to a new place on its sheet. Section A on the same sheet already shows the section line moved in screenshot 10. |
| [09-draw-in-elevation.png](09-draw-in-elevation.png) | In the south elevation: a window placed by clicking on the upper wall, and a low garden wall drawn by clicking two points. Both appear in 3D. |
| [10-drag-section-line.png](10-drag-section-line.png) | Section A selected in the plan, dragged by its round middle handle from x = 2.1 m to 5.05 m. The square end handles drag one end. |

A sample PDF of all three sheets is in the project files as `sample-house-sheets.pdf`.

## Known gaps

- Where a slab runs into a wall (the first floor slab sits inside the ground floor walls), sections still show both fills overlapping. Walls join each other cleanly, but wall/slab joins need a priority rule that comes later.
- Hatch patterns are fixed per element type (walls diagonal, slabs crossed, everything else solid grey). Materials with their own patterns come with the material system.
- 3D views on sheets are hidden-line only, with no shading. Orbiting redraws the whole view, which takes a moment on bigger models.
- A 3D view centres on the model, so it shifts a little on its sheet when the model's overall size changes.
- Views on sheets cannot be cropped yet, so plans include their elevation marks and the whole drawing.
- In sections and elevations, Slab, Section and Elevation tools still only work in plans. Walls drawn in a section sit just behind the section line.
- Views on a sheet can be dragged on top of each other; there are no alignment guides yet.
- The title block layout is fixed, and the project name comes from the file name.
