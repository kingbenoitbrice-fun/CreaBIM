# Phase 1: Align, Stretch and Extend in 3D, and centre lines

The Align tool's three modes now work in the 3D window and in 3D views, as well as in plans, sections and elevations. Every element also offers its **centre lines**, both as a reference and as what lines up, in 2D and 3D. The two gaps left by the manipulation round are fixed too: Stretch now accepts an edge that is nearly parallel, and its message is no longer overwritten by the guide hints. All screenshots are of the sample house.

| Shot | What it shows |
| --- | --- |
| [01-edge-reference-in-3d.png](01-edge-reference-in-3d.png) | The 3D window, Align tool in *Stretch to it* mode. The pointer is on the top edge of a pergola rafter, which is highlighted in blue. Clicking it makes that edge the reference. |
| [02-tab-to-a-buried-post-top.png](02-tab-to-a-buried-post-top.png) | The posts' tops are buried under the rafters. Pressing `Tab` reaches them ("2 of 2 under the pointer"). The top face to stretch is shown in orange, with a dashed line and its distance (200 mm) to the reference. |
| [03-post-stretched-to-the-edge.png](03-post-stretched-to-the-edge.png) | After the click, the posts' tops reach the rafter's top edge and their bottoms stay where they were. The status bar reads "Stretched Pergola Posts 200 mm out to the reference". |
| [04-centre-line-reference-in-plan.png](04-centre-line-reference-in-plan.png) | In a plan, the reference is a column's centre line (the blue dashed line). The beam beside it shows its centre line in purple, drawn bold because it is under the pointer. |
| [05-beam-centred-on-column-in-plan.png](05-beam-centred-on-column-in-plan.png) | Clicking the beam's centre line centres the beam on the column: its X goes from 12550 mm to 12000 mm. |
| [06-beam-centre-line-onto-column-in-3d.png](06-beam-centre-line-onto-column-in-3d.png) | The same in the 3D window. The reference is the column's centre plane, shown as a patch on the column. On the beam's end face, its upright centre line is bold purple. The preview shows the 550 mm the beam will move. |
| [07-beam-centred-on-column-in-3d.png](07-beam-centred-on-column-in-3d.png) | After the click, the beam sits centred on the column (X 12000 mm). |
| [08-extend-wall-preview-in-3d.png](08-extend-wall-preview-in-3d.png) | *Extend to it* in the 3D window. The reference is the house's east face. Hovering a free-standing wall near its west end draws a dashed line from that end to the face, labelled 2800 mm. |
| [09-wall-extended-in-3d.png](09-wall-extended-in-3d.png) | After the click, the wall's end runs to the face (End X 10150 mm) and its far end stays put. |
| [10-stretch-in-a-3d-view.png](10-stretch-in-a-3d-view.png) | The same tool in a 3D view (the parallel "3D View South-East"). The reference is the house's top corner, picked with the endpoint snap. The wall's top face is the target, and the preview reads 3000 mm. |
| [11-wall-top-stretched-in-a-3d-view.png](11-wall-top-stretched-in-a-3d-view.png) | The wall's top now reaches the corner's height: its Height goes from 3000 mm to 6000 mm, and its bottom stays put. |

## How to try it

1. Pick the Align tool: the last icon in the tool row. Choose **Align**, **Stretch to it** or **Extend to it** in the tool options strip.
2. In the 3D window or a 3D view, click the reference. It can be any of these:
   - a corner or midpoint (the point snaps);
   - an edge;
   - a face;
   - an element's centre line, drawn dashed purple on the face under the pointer;
   - a grid line;
   - empty space. In the 3D window this means the working level's plane; in a 3D view, the level nearest the ground.
3. Click a face of what to change. Press `Tab` to reach a face that is behind another one. Before you click, the face is shown in orange with the distance it will move.
   - **Align** moves the whole element until that face is flush with the reference. An upright element is turned to the reference first.
   - **Stretch to it** moves only that face and keeps the opposite one. It follows the same rules as Move face for each kind of element (a wall's top, a slab's underside, a column's top, a mass's face, and so on).
   - **Extend to it** runs the nearer end of a wall, beam or grid line to the reference. Walls joined to that end follow. Anything else stretches the face you clicked.
4. The reference stays picked, so you can line up several elements against it. It stays picked even when you move between 3D panes. `Esc` lets it go.

Centre lines in plans, sections and elevations work the same way: hover an element and its centre lines show faintly in purple. The one under the pointer is drawn bold. Click it as the reference, or as what to centre on the reference.

## Audit: Align, Stretch and Extend in 3D

The 3D rows of the table in [phase1-manipulation](../phase1-manipulation/README.md#audit-element-kinds-against-views) are updated to match this one. "3D window" is the perspective 3D window; "3D view" is a parallel 3D view in the project browser. Both behave the same.

| Element | Align (3D window / 3D view) | Stretch (3D window / 3D view) | Extend (3D window / 3D view) | Centre lines offered |
| --- | --- | --- | --- | --- |
| Wall | yes, turns parallel first (tested) | yes: top, bottom, sides, ends (tested) | yes: nearer end, joins follow (tested) | its location line |
| Beam | yes (tested by its centre line) | yes: ends, sides, top, bottom (tested) | yes: nearer end | across and through its depth |
| Column | yes | yes: top, bottom, sides (tested) | as Stretch | both centre axes |
| Slab | yes | yes: top, underside, edges (tested) | as Stretch | X, Y and mid-thickness |
| Roof | yes | yes, by its Move face rules | as Stretch | X and Y |
| Stair | yes | yes, by its Move face rules | as Stretch | X and Y |
| Railing | yes | yes, by its Move face rules | as Stretch | none |
| Door / Window | moves its wall; by its centre line it slides along the wall (tested) | yes: jambs, head, sill | as Stretch | across its wall and at mid-height |
| Mass | yes | yes: any face, sloped too (tested) | as Stretch | X, Y and Z through its box |
| Grid | can't be picked as a target in 3D (gap) | — | — | — (its line is a reference) |

"As Stretch" means the same as in sections: for something that isn't a line, extending it to a face means moving its face. "Tested" means a unit test covers it.

## Design choices

- **One tool, three panes.** 3D uses the same Align tool, modes and two-click order as 2D. Only the picking differs: 2D picks edges, and 3D picks faces, edges, points and centre planes. The 3D reference is kept separately from the 2D one, so a reference picked in the 3D window still holds in a 3D view. A 3D reference doesn't carry over to a plan, though.
- **The order a reference is tried in**, under the pointer:
  1. point snaps;
  2. a centre line;
  3. a grid line;
  4. an edge;
  5. the face;
  6. the level plane.
  Points come first so that a corner can be picked on an edge. Centre lines come before edges so that a centre line along a face edge can be reached. A face's centre snap is left out, because it stands for the face itself.
- **Empty space is a level.** Clicking nothing in the 3D window takes the working level's plane. In a 3D view, which has no working level, it takes the level nearest the ground. This lets you drop a slab's underside to a level, or stand a column on it.
- **Shapes, not elements.** A reference is reduced to a plane, a line or a point:
  - A plane moves the face by the distance between the planes, measured along the face's normal.
  - A line must run along the face; the face moves to meet it.
  - A point moves the face along its normal to the point's level.
  Stretch then calls the same push rules as Move face, so every kind stretches the way it pushes.
- **Tolerance.** A face or edge counts as parallel within 5°. This is the same constant in 2D and 3D (`PARALLEL_TOL_DEG`). Past that, the message says how many degrees off it is. Distances are measured from the point nearest the click, so a slightly skew edge lands where you pointed.
- **Centre lines are planes.** A centre line is a rectangle through the element:
  - in a 2D pane seen edge-on, it is drawn as a line;
  - in 3D, it is drawn as its trace on the hovered face, clipped to that face.
  As a target, the centre plane stands in for a face. Align moves the element by the distance from its centre to the reference.
- **Stretch ignores centre lines as targets.** Stretch moves faces. On a small face, a centre line would otherwise cover the face, so it isn't offered once a reference is picked. Clicking a centre line in 2D Stretch explains this.
- **Doors and windows.** Aligning one by its edge or face moves its wall, as in 2D. Aligning it by its centre line slides the opening along its wall and leaves the wall in place.
- **Extend.** It is only for linear elements: walls, beams and grid lines. Their nearer end runs to where the line meets the reference, at the height you clicked. For other elements, or a level reference, Extend stretches the face instead. A dashed preview shows the run and its length before the click.
- **Messages stay.** The rest-on guide hints no longer replace the result of a click made by the Align tool in the same frame.

## Known gaps

- Grid lines can be references in 3D but not targets, because the 3D window has no face to click on them. Extend a grid in a plan.
- An arrayed mesh (the pergola rafters) offers the centre lines of the whole group, not of each copy.
- Thin faces, such as a wall's top seen from far away, are hard to pick. Zoom in, or use `Tab`.
- There is no centre plane across a wall's or beam's length (the midpoint of its run). Use the midpoint snap instead.
- The 3D window zooms toward the centre of the view, not the pointer. This is unrelated to this round, but it makes framing small targets slower.
- The distance label in the preview is drawn in orange without a background, so it can be hard to read over orange or brick faces.
