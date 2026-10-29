# Phase 1: drawing quality

The review of the Maple Lane starter house sheets on 2026-10-28 asked for plans that read like permit drawings. This milestone covers that work:

- a clear line-weight hierarchy;
- tinted footprints for what lies below the cut;
- a view depth;
- the stair drawn on both floors;
- clean joins in cut fills;
- labels moved off lines and other text;
- slope arrows on roof plans;
- window reveals in 3D.

Each "before" picture is from `main` at 6c6eb60, and each "after" picture is from this branch. Every picture is rendered from the starter house's sheets PDF at 150 dpi (`creabim sample house.creabim.json`, then `creabim pdf house.creabim.json sheets.pdf`).

## The markups

### 1. The stair on the first floor, and the void's cross

`01-first-floor-stair-and-well-before.png` and `01-first-floor-stair-and-well-after.png` (sheet A102).

**Before:** the first floor showed the stair well as an outline with one cross. That cross was made from the diagonals of the well's bounding box, snapped to the nearest corners, so on the L-shaped well it ran across the floor. The stair itself was not drawn.

**After:**
- The stair from the ground floor is drawn where the well shows it: its treads, sides and landing, the walking line with its arrow, and *DN* where it reaches this floor.
- The cross is drawn corner to corner in each convex part of the opening, so the L gets a cross in each leg. Every line stays inside the opening.
- The cross is now drawn in the lightest pen, so the stair reads through it.
- On the ground floor, the well is shown as the dashed edge of the floor above, not crossed over the stair.

### 2. Footings meeting at the loggia corner

`02-foundation-loggia-corner-before.png` and `02-foundation-loggia-corner-after.png` (sheet A103).

**Before:** where two strip footings overlapped, each kept its own outline, which left a small square.

**After:** cut fills of the same material that touch or overlap now merge into one outline. Edges that run inside another piece of the same material are dropped, as well as shared edges. The whole foundation plan is one network of strips with no inside lines.

### 3. Labels on lines

`03-ground-floor-loggia-label-before.png` and `03-ground-floor-loggia-label-after.png` (sheet A101).

**Before:** *LOGGIA 16.0 m²* sat on the B-B section line.

**After:** a placement pass runs on every plan. It moves room names, notes, tags and plain labels off cut lines, grid and section lines, other linework and each other, to the nearest clear spot within 14 mm on paper.
- A label moved more than 9 mm gets a leader back to where it was.
- Grid and section bubbles and dimension text stay where they are, and the other labels keep clear of them.

`06-villa-room-labels-before.png` and `06-villa-room-labels-after.png` show the same pass on the user's Tropical Villa (`TropicalVilla_REV00.creabim`, sheet A-101). Before, room names sat under the window and door tags: *SCREENED PATIO*, *COURTYARD*, *LAUNDRY*, *WALK-IN*, *BEDROOM 2* and the gatehouse. After, every name and tag reads on its own.

### 4. Roof plan slope arrows

`04-roof-plan-slope-arrows-before.png` and `04-roof-plan-slope-arrows-after.png` (sheet A104).

**Before:** only the notes said which way each roof falls.

**After:**
- Each sloping face of a roof gets an arrow down toward its eave, with its pitch written beside it (*35°*). The arrows are worked out from the roof's own faces.
- A shallow slope is written as a fall, such as *1:60*. Flat roofs have no arrow.
- The arrows are drawn only when the plan looks down on the roof from above, so the first floor plan, which is under the roof, has none.
- The starter house's slate notes no longer repeat the pitch, and they have moved off the B-B section line.

### 5. Line weights and below-cut tint

`05-ground-floor-weights-and-tint-before.png` and `05-ground-floor-weights-and-tint-after.png` (sheet A101, 1:50).

**Line weights.** The pens step down in weight:

| Pen | At 1:50 | At 1:100 |
| --- | ---: | ---: |
| Cut | 0.70 | 0.50 |
| Projection (slab edges, the stair, low walls) | 0.35 | 0.25 |
| Symbols (door leaves, swings, frames) | 0.25 | 0.18 |
| Furniture and fixtures | 0.18 | 0.13 |
| Hatching | 0.13 | 0.09 |
| Hidden, dashed | 0.18 | 0.13 |

All widths are in millimetres on paper. Views at 1:75 or larger take the 1:50 weights; smaller scales take the 1:100 ones.

The weights belong to each view: in a plan's *Properties*, under *Graphics*, untick *Line weights from the scale* to set your own. *Pens by category* is a table that names the pen each category's lines take. By default, furniture and fixtures use the fine pen.

**Below-cut tint.** Furniture, sanitary ware, kitchen runs and imported meshes that lie wholly below the cut get a light tint of their material's colour under their lines. The tint is on by default. In *Graphics* you can set its strength, switch it to grey, or turn it off. Building services, such as lights and sockets, are left untinted.

### 6. The plan's graphics in Properties

`07-plan-graphics-in-properties.png`: the *Graphics* group of the Ground Floor plan, which *Properties* shows when nothing is selected or when the view is selected. In this shot *Line weights from the scale* is unticked so the six pen widths can be set, and *Pens by category* is open.

### 7. Window reveals in 3D

`08-window-reveals-in-3d-after.png`: the first-floor windows of the starter house in the 3D view, from this branch. The jambs and heads show the white render and plaster. For the yellow insulation they showed before, see `02b-readme-hero.png` in `phase1-starter-house`.

## Other changes

- **View depth.** *Graphics* in a plan's *Properties* has a *View depth*. It sets how far below the floor the plan looks through holes in the floor. By default the plan looks down to the level below. Above the floor and below the cut is *below*, drawn in projection. Under the floor, down to the view depth, is *beyond*.
  - Today the view depth applies to stairs seen through a well: a view depth of 1 m shows only the top of the flight.
  - It is stored in a `PlanRange` so that the view-range work (a top, a bottom and a depth relative to any level) can extend it.
- **Reveals in 3D.** In the 3D view, the jambs, head and sill of doors and windows in layered walls are lined with the wall's finishes: render on the outside part, plaster on the inside part. Insulation no longer shows yellow in the reveals. The linings are for the 3D view only: drawings, IFC and other exports leave them out.
- **Openings above the cut.** A wall opening wholly above the cut, such as a duct hole or a high window, is drawn dashed in its wall.
- **`creabim label-clashes <model>`** lists the labels, tags and notes that still overlap in each plan. It reports 0 on the starter house and on the Tropical Villa.

## The sheets, re-rendered

`sheet-01-A000-cover.png` to `sheet-11-A901-3d-views.png`: every starter house sheet from this branch, at 1600 px wide.

## How to try it

1. Open CreaBIM: it starts on the starter house (or use *File › New starter house*).
2. Open the *First Floor plan* to see the stair in its well, and the *Roof plan* to see the arrows.
3. Select a plan view in the browser, or click an empty spot in an open plan. Look at *Graphics* in *Properties*.
4. Export the sheets to PDF to see the weights in print.

## Known gaps

- The on-screen plan pane still uses its own fixed pixel widths. The view's line weights reach sheets, PDF and SVG.
- *Pens by category* covers lines seen below the cut. A cut line weight, a colour and a pattern per category are left for the object styles work (P1 of the 2026-10-28 spec).
- The view depth applies to stairs seen through holes in the floor. Other elements of the level below are not drawn through voids yet, and there is no top or bottom clip yet (P2 of the spec).
- Dimension text is not moved. Labels keep clear of it instead.
- The placement pass runs on plans only, not on sections or elevations.
- In a crowded room, a tag can move a fair way from its door or window. Its leader still points back to it.
- The tint of a piece of furniture is the convex outline of each of its parts. An L-shaped worktop made as one part is tinted as its hull.
- The cross in the stair well still crosses the stair, in the lightest pen.
- Shaded 3D views on sheets don't show the reveal linings, because drawings leave them out. The 3D viewport shows them.
- The fixtures on doors and the sofa against a door in the third markup are for the rooms and fixtures work, which runs on its own branch.
