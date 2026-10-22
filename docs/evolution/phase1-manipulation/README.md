# Phase 1: manipulation everywhere

This round makes moving and stretching work the same way for every kind of element and in every view. It adds a **Stretch** mode to the Align tool, a new Stair icon, and a **Flip** for section and elevation markers. All screenshots are of the sample house.

| Screenshot | What it shows |
| --- | --- |
| [01-stair-icon-toolbar.png](01-stair-icon-toolbar.png) | The new Stair icon: a flight seen from the side, with its handrail. It replaces the old bar-chart glyph. |
| [02-move-face-slab-top-in-section.png](02-move-face-slab-top-in-section.png) | Move in face mode in Section A, pointing at the top of the first-floor slab. In a section or plan, the edge under the pointer stands for the face square to the paper. It is highlighted on the slab's side, and dragging it changes the slab's thickness. Before this, face picking only worked in 3D. |
| [03-stretch-slab-top-to-line-in-section.png](03-stretch-slab-top-to-line-in-section.png) | Align tool, *Stretch to it* mode, in a section. Click a reference line, then the slab's edge, and that face moves to the line. The opposite face stays where it is. |
| [04-stretch-wall-top-in-section.png](04-stretch-wall-top-in-section.png) | The same with a wall's top: its height changes to meet the reference. |
| [05-stretch-wall-end-in-plan.png](05-stretch-wall-end-in-plan.png) | Stretch in a plan: a wall's end stretched to a reference. Walls joined to it follow. |
| [06-section-flip-arrow-before.png](06-section-flip-arrow-before.png) | A selected section marker now has a flip arrow, like a door or window. It points the way the section looks. |
| [07-section-flipped-looks-east.png](07-section-flipped-looks-east.png) | After a click on the arrow, the marker looks the other way along the same cut line. |
| [08-flipped-section-regenerated.png](08-flipped-section-regenerated.png) | The section view redraws from the other side. Notes and free dimension points are mirrored so they stay over the same parts of the building. |
| [09-stretch-column-top-to-beam-in-section.png](09-stretch-column-top-to-beam-in-section.png) | Stretch a pergola post's top up to the underside of a beam, in a section. |
| [10-stretch-beam-end-to-post-in-section.png](10-stretch-beam-end-to-post-in-section.png) | Stretch a beam's end to a post's face, in a section. |
| [11-move-face-stair-side-in-plan.png](11-move-face-stair-side-in-plan.png) | Move face in the plan, on a stair's side: the stair widens to 1600 mm. The status bar reads "Push/pull Stair: 600 mm out". |
| [12-section-flip-context-menu.png](12-section-flip-context-menu.png) | Right-click a selected section in a plan to see *Flip view direction* (Space), the same command the arrow runs. |

## How to try it

- **Stretch:** pick the Align tool (A), choose *Stretch to it* in the tool options, click the reference (a line, grid, level, or another element's edge), then click the edge to stretch. The edge must be parallel to the reference. As you hover, the edge Stretch will take is highlighted. This works in plans, sections and elevations.
- **Move face:** press M, switch to face mode, and point at a face. In 3D that is the face itself; in a plan or section it is the element's edge. Then drag, or type a distance.
- **Grips in sections:** select an element in a section. Its plan grips (wall ends, slab corners, beam ends, a column's base, roof corners, a stair's start, railing points, grid ends) slide along the section line, and the height grips move up and down.
- **Flip a section or elevation:** select its marker in a plan. Click the flip arrow, press Space, or right-click and choose *Flip view direction*. Ctrl+Z undoes it.

## Audit: element kinds against views

Key:
- **yes**: works.
- **—**: does not apply.
- **sel**: works once the element is selected some other way (see Known gaps).

The **Move** column covers G, M and dragging a selected element. **Grips** means dragging its handles.

| Element | View | Move | Move face (push/pull) | Grips | Align (move to) | Stretch | Extend |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Wall | Plan | yes | yes: sides, ends | yes | yes | yes | yes |
| Wall | Section / Elevation | yes | yes: top, bottom, ends | yes | yes | yes | yes (as Stretch) |
| Wall | 3D | yes | yes: all faces | yes | — | — | — |
| Slab | Plan | yes | yes: edges | yes | yes | yes | — |
| Slab | Section / Elevation | yes (sel in elevations) | yes: top, underside, edges | yes | yes | yes | yes (as Stretch) |
| Slab | 3D | yes | yes | yes | — | — | — |
| Roof | Plan | yes | yes: eaves and edges | yes | yes | yes | — |
| Roof | Section / Elevation | yes | yes: top and underside thicken; edges move | yes | yes | yes | yes (as Stretch) |
| Roof | 3D | yes | yes | yes | — | — | — |
| Column | Plan | yes | yes: width, depth, diameter | yes | yes | yes | — |
| Column | Section / Elevation | yes | yes: top, bottom, sides | yes | yes | yes | yes (as Stretch) |
| Column | 3D | yes | yes | yes | — | — | — |
| Beam | Plan | yes | yes: ends, sides | yes | yes | yes | yes |
| Beam | Section / Elevation | yes | yes: top, bottom, ends | yes | yes | yes | yes (as Stretch) |
| Beam | 3D | yes | yes | yes | — | — | — |
| Stair | Plan | yes | yes: side (width), first and last riser | yes | yes | yes | — |
| Stair | Section / Elevation | yes | yes: top, bottom, first and last riser | yes | yes | yes | yes (as Stretch) |
| Stair | 3D | yes | yes | yes | — | — | — |
| Railing | Plan | yes | yes: ends extend, sides move it | yes | yes | yes | — |
| Railing | Section / Elevation | yes | yes: top (height), ends | yes | yes | yes | yes (as Stretch) |
| Railing | 3D | yes | yes | yes | — | — | — |
| Door / Window | Plan | yes, along its wall | yes: jambs (width) | yes | yes | yes | — |
| Door / Window | Section / Elevation | yes, along its wall | yes: jambs, head, sill | yes | yes | yes | yes (as Stretch) |
| Door / Window | 3D | yes | yes | yes | — | — | — |
| Mass | Plan | yes | yes | yes | yes | yes | — |
| Mass | Section / Elevation | yes | yes | yes | yes | yes | yes (as Stretch) |
| Mass | 3D | yes | yes: any face, sloped too | yes | — | — | — |
| Grid | Plan | yes | — | yes | yes | — (falls back to Extend) | yes |
| Grid | Section / Elevation | — | — | yes | — | — | — |
| Level | Section / Elevation | yes | — | yes | yes | — | — |
| Section / Elevation marker | Plan | yes | — | yes | yes | — | — (Flip: yes) |

## Design choices

- Stretch is a mode of the Align tool, next to Align and Extend, rather than a new tool. The steps are the same: reference first, then target. The only difference is that the face moves instead of the whole element.
- In a section or elevation, Extend does what Stretch does, because "extend to that line" there means moving a face.
- Where edges overlap, picking prefers the selected element, then the element that is cut over one seen beyond. That way, the top of a slab is picked rather than the top of the wall behind it. If the best edge can't be stretched, Stretch tries the next one under the pointer.
- Pushing a roof's top or underside changes its thickness. Its boundary edges move from a plan, or from a level face seen in a section.
- A stair's end faces respond only near the first and last riser. Its other faces are treads and risers, not things you'd stretch.
- Pushing a railing's side moves it, because a railing has no width.
- Flipping a section swaps the two ends of its line. Free notes, tags and dimension points in that view are mirrored so they stay over the same parts of the building.

## Known gaps

- In elevations, a slab hidden behind a wall can't be clicked. Select it in a plan, in 3D or in the project browser, then use M, its grips or Stretch.
- Align, Stretch and Extend work in plans, sections and elevations, not in 3D. In 3D, use Move face.
- Stretch needs the edge to be parallel to the reference. It doesn't rotate the face to meet a slanted line.
- Stretch's confirmation message in the status bar can be replaced right away by the "rest on" guide hints.
- After a flip, the section's pane keeps its old camera, so the drawing may sit off-centre until you press Home or fit the view.
- Flip mirrors free dimension points. Points stuck to an element are worked out again from the model.
- Pushing a stair's side changes the first flight's width only. Landings keep their own width.
- Railings that follow a stair only change height with push/pull.
- Pushing a sloped roof face changes its thickness rather than moving the eave along the slope.
