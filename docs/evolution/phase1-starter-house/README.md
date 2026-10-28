# Phase 1: the starter house

CreaBIM now opens on the **Maple Lane House**, a contemporary two-storey family house with a single-storey kitchen wing, in place of the old sample house. It is built in `creabim-core/src/starter.rs` with the same elements a user draws, and comes with a full set of sheets. The old houses stay as *File › New simple sample house* and the other samples, which the tests use.

| Screenshot | What it shows |
| --- | --- |
| [00-before-the-old-sample-house.png](00-before-the-old-sample-house.png) | Before: the old sample house that the app opened on, and the README's old screenshot. |
| [01-first-launch.png](01-first-launch.png) | The app as it opens: the starter house in 3D beside its ground-floor plan, the views, schedules and eleven sheets in the project browser, and the status bar saying what to do. |
| [02-3d-and-plan-with-a-layered-wall.png](02-3d-and-plan-with-a-layered-wall.png) | The garden side in 3D (green roof, timber-clad wing, balcony on steel posts and beams, terrace), and the plan beside it at 1:50. The wing's east wall is selected in both. Properties shows its type, its layered assembly (oak cladding, air gap, wood-fibre insulation, block, plaster), its top attached to the wing roof and its joins. |
| [02b-readme-hero.png](02b-readme-hero.png) | The README's hero image: a three-quarter view of the garden side, showing the loggia, the balcony on its steel posts and beams, the gable and the green-roofed wing. The 3D view is in *Shaded with edges* and the plan is framed to fill its pane, with nothing selected. |
| [03-sheet-in-the-app.png](03-sheet-in-the-app.png) | Sheet A101 open in the app. Properties shows the project's own title block with its fields filled in (job, address, client, status, revision, drawn by). This is the README's sheet image. |
| [04-A000-cover.png](04-A000-cover.png) | A000 Cover (A3): the project's name, the garden view at 1:100, the drawing list and a short description, clear of the title block. |
| [05-A101-ground-floor-plan.png](05-A101-ground-floor-plan.png) | A101 Ground Floor Plan (A2, 1:50). It shows grids A–E and 1–3, layered walls joined at every corner and tee, doors and windows with tags outside the walls, the curtain wall with its door, the kitchen opening, the L stair, and rooms with names and areas. Furniture and fixtures are drawn with their plan symbols. Chains of dimensions run along the openings, the grids and the overall size. The section and elevation marks sit clear of the grid bubbles. |
| [06-A102-first-floor-plan.png](06-A102-first-floor-plan.png) | A102 First Floor Plan (A2, 1:50): bedrooms, bathroom, en-suite, the landing over the stair well, the balcony over the loggia, and stud partitions. |
| [07-A103-foundation-plan.png](07-A103-foundation-plan.png) | A103 Foundation Plan (A2, 1:50): the plan of the Foundation level cuts the strip footings under every wall and the pads under the loggia column and the terrace posts. It has notes with leaders and the grid and overall dimensions. |
| [08-A104-roof-plan.png](08-A104-roof-plan.png) | A104 Roof Plan (A3, 1:100): the slate gable roof with its ridge, the green flat roof over the wing and terrace, the zinc canopy, and notes. |
| [09-A201-section-a-a.png](09-A201-section-a-a.png) | A201 Section A-A (A2, 1:50), through the stair. It shows the footings, the insulated ground slab, the concrete first floor, the gable roof's layers, the furniture beyond, and storey heights with level datums. |
| [10-A202-section-b-b.png](10-A202-section-b-b.png) | A202 Section B-B (A2, 1:50), through the loggia, the living room, the bedroom and en-suite, and the kitchen wing. It shows the column and beams at the loggia, the balcony railing, the first-floor walls running up to the roof, and the wing's flat roof. |
| [11-A301-elevations.png](11-A301-elevations.png) | A301 Elevations (A2, 1:100): all four sides shaded in their materials, with level datums. |
| [12-A601-door-window-structure-schedules.png](12-A601-door-window-structure-schedules.png) | A601: the door schedule (16), the window schedule (20), and the column, beam and footing schedule (23 members). |
| [13-A602-furniture-fixture-wall-schedules.png](13-A602-furniture-fixture-wall-schedules.png) | A602: the furniture schedule (36 pieces), the fixture schedule (84 fixtures) and the wall schedule (25 walls), grouped by room or category. |
| [14-A901-3d-views.png](14-A901-3d-views.png) | A901 3D Views (A2): the street view and the garden view, both at 1:100. |

## What the house demonstrates, and where

- **Levels:** Foundation (-800), Ground Floor (±0), First Floor (+3000). They show in the sections and elevations.
- **Grids:** A–E and 1–3, on the plans. The dimension chains measure from them.
- **Layered wall assemblies:** "Exterior wall 400, rendered", "Exterior wall 400, timber clad" and "Stud partition 100", plus layered floors and roofs. They show cut in the plans and sections (05, 06, 09, 10) and in Properties (02).
- **Wall joins:** L and T joins everywhere. Two partition ends are set to butt square (*Joins at*), where three walls would meet.
- **Slabs:** the ground floor slab, a first floor with a hole for the riser, and paving for the terrace, the loggia steps and the front path. A **shaft** cuts the stair well through the first floor (06).
- **Stair and railings:** an L stair with a landing, a handrail, a landing balustrade and a balcony railing (05, 09, 10).
- **Roofs:** a slate gable at 35° with overhangs, a green flat roof over the wing and terrace, and a zinc canopy (08, 11). The first-floor walls are attached to the gable roof, so they follow it.
- **Doors and windows:** 16 doors and 20 windows, each with a type and a frame. Several are flipped by hand and by facing. There is also one plain wall opening, the kitchen opening (05, 12).
- **Curtain wall:** the three-bay garden glazing cuts its host wall, with a door in its middle bay (05, 10).
- **Columns:** a steel column that rises through both floors at the loggia corner, and two round terrace posts. **Beams:** steel beams under the balcony and along the eaves (10, 12).
- **Footings:** strips under every ground-floor wall and pads under the columns. They are cut by the foundation plan (07) and listed in the structure schedule (12).
- **Rooms:** names and floor areas on both plans, in a bold room-name text style.
- **Annotation:** dimensions, tags on every door and window, and notes with leaders (05–08).
- **Materials:** custom render, anthracite aluminium and dark steel, alongside the library's materials.
- **Catalogue furniture and services fixtures:** kitchen units, sofas, beds, a bath, WCs, sockets, switches, lights, radiators, smoke detectors and a boiler. That makes 120 objects, in their own schedules (13).
- **Sheets:** eleven sheets with a custom title block, "Maple Lane title block", with filled-in fields (03–14). *File › Sheets as PDF* writes them as an 11-page PDF.
- **IFC:** `creabim ifc` exports 3 storeys, 216 contained elements and 40 openings. The export passes `tools/validate_ifc.py`.

## How to try it

- Start the app: it opens on the starter house. *File › New starter house* brings it back.
- `creabim sample house.creabim.json` writes it from the command line, and `creabim sample-simple` writes the old simple house.
- The Help has a page on it: *Getting started › The starter house*.

## Known gaps

- Rooms are notes, not room elements: an area is computed when the house is built and does not follow a moved wall. For the same reason there is no room schedule.
- The roof plan has no slope arrows; notes say which way each roof falls.
- The interior first-floor partitions run up to the vaulted roof rather than stopping at a ceiling, since there is no ceiling element.
- Where the two butt-jointed partitions meet the spine at the loggia corner, the foundation plan shows a small overlap square between their strip footings.
- In 3D, the reveals of the windows show the insulation layer's yellow colour.
- The app frames the 3D view and the plan for the window size it starts at, so on a large screen the plan first appears small until you zoom.
