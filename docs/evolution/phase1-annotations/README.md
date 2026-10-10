# Phase 1, milestone 3: dimensions, tags, schedules and materials

Reference screenshots of what this milestone is meant to do, taken from the sample house. It also closes the gaps left by milestone 2.

| Screenshot | What it shows |
| --- | --- |
| [01-dimensions-and-tags-in-plan.png](01-dimensions-and-tags-in-plan.png) | The ground floor plan with dimensions (an overall string, a depth string and a chain along the front wall) and door and window tags (D01, W01…). A new dimension drawn with the **Dimension** tool snaps to wall corners and reads 5790. The **Tag** tool tagged the partition as "Stud partition 120", using its material and thickness. |
| [02-dimensions-follow-the-door.png](02-dimensions-follow-the-door.png) | The front door dragged along its wall. The chain dimension updates to 2750 / 1000 / 6550 and the door's tag moves with it, because both are attached to the door instead of to fixed points. |
| [03-edit-windows-in-the-schedule.png](03-edit-windows-in-the-schedule.png) | *File → New schedule → Windows*, an editable table. A window's width changed in the table updates the 3D view, the plans and the schedule on its sheet. Click a row's mark to select that window. **Export CSV** saves the table. |
| [04-crop-a-view-on-a-sheet.png](04-crop-a-view-on-a-sheet.png) | The 1:50 Entrance plan on sheet A601, cropped to the front door. Select a viewport and drag the dashed crop frame's edge handles, or type the crop in Properties. |
| [05-shaded-3d-view-on-a-sheet.png](05-shaded-3d-view-on-a-sheet.png) | The 3D view on sheet A001, now shaded in material colours under its hidden lines. It still orbits when you double-click it. Turn shading on or off per view with **Shaded** in the toolbar or Properties. |
| [06-slab-and-section-drawn-in-a-section.png](06-slab-and-section-drawn-in-a-section.png) | In Section B: a balcony slab drawn with the **Slab** tool (its top snaps to First Floor and it shows in 3D), then a new section drawn with the **Section** tool by clicking where it cuts and the side it looks towards. Walls now cut cleanly through the slabs that sit in them. |
| [07-new-section-opened.png](07-new-section-opened.png) | The new Section C, opened from the Views list. It looks west through the house, with the cut walls and slabs hatched. |
| [08-pdf-plans-with-dimensions-and-tags.png](08-pdf-plans-with-dimensions-and-tags.png) | Sheet A101 in the PDF export: the plans with their dimensions and tags. |
| [09-pdf-sections-and-shaded-elevation.png](09-pdf-sections-and-shaded-elevation.png) | Sheet A201: sections with storey-height dimensions attached to the levels, brick-hatched walls passing through concrete slabs, and the south elevation shaded in material colours. |
| [10-pdf-schedules-and-cropped-detail.png](10-pdf-schedules-and-cropped-detail.png) | Sheet A601: door, window and wall schedules (with totals) beside the cropped 1:50 entrance detail. |

A sample PDF of all four sheets, the sample model and a window schedule CSV are in the project files next to these screenshots.

## Materials

The sample comes with four materials: Brick, Concrete, Stud partition and Timber. Select one in the Model tree under *IfcMaterial* to change its hatch pattern (empty, solid, diagonal, cross, brick, concrete, timber) and its colour. Walls, slabs and masses pick a material in Properties. The menu there can also create a new one. Materials export to IFC as `IfcMaterial` with `IfcRelAssociatesMaterial`.

## Known gaps

- Drawing tools in 3D views: only the Tag tool works there. Walls, doors, slabs and dimensions are drawn in plans and sections.
- Dimensions are aligned only (measured along the line between their two points). There are no horizontal, vertical, angular or radius dimensions yet.
- In sections, dimensions snap to levels and to the corners of cut elements only, not to edges seen beyond the cut.
- Dimension text always sits at the middle of its line, so it can sit over a grid line or a section mark.
- Schedule columns are fixed for each category. You can't add, remove or reorder columns yet. Room and slab-area schedules come with rooms.
- Shaded images on sheets are made from pixels, not vectors, and have no anti-aliasing, so edges look a little stepped when you zoom far into the PDF.
- A slab drawn in a section takes its top from the nearest level. Free heights are set afterwards in Properties.
- Crops are rectangles only.
- Deleting a door also deletes its tags and the dimensions attached to it (the Delete button counts them as "attached").
