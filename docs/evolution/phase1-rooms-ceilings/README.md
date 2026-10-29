# Phase 1: rooms and ceilings

CreaBIM now has **rooms** and **ceilings**. A room is a real element: it has a name, a number and finishes, and its area, perimeter and height come from the walls round it. A room separator divides an open-plan space where there is no wall. A ceiling hangs under the floor above, has a reflected ceiling plan, sets the height of the room under it, and is a top that partitions can stop at.

In the starter house, the room notes are now 16 rooms with tags and a room schedule sheet (A603). There are 9 ceilings, and the first-floor partitions stop at them. The furniture and fixtures that stood in doorways have been moved, and `creabim check` finds no objects in a doorway or a door's swing.

| Screenshot | What it shows |
| --- | --- |
| [01-ground-floor-plan-with-room-tags.png](01-ground-floor-plan-with-room-tags.png) | The app on the starter house. Every room has a tag (name, number, area) that reads from the room itself. The toolbar has the new Room, Room separator and Ceiling tools after Object. The view options have *Tag rooms*, *Ceiling plan* and *Room colours*. |
| [02-A102-first-floor-plan.png](02-A102-first-floor-plan.png) | Sheet A102 from *File › Sheets as PDF*: the first floor with room tags 1.01–1.07. The bedroom 1 bed and its bedside tables are clear of door D14. |
| [03-room-colours.png](03-room-colours.png) | *Room colours* on: each room filled with a colour of its own. A room keeps the same colour in every view. |
| [04-room-properties.png](04-room-properties.png) | A room in *Properties*: its area and perimeter measured to the walls' inner faces, its number, finishes, occupancy and department, and *Own height*. Department and own height have been set by hand. |
| [05-room-tool-fill-room.png](05-room-tool-fill-room.png) | The Study deleted, then placed again with the Room tool in *Fill room (3)* mode: one click inside the walls. It gets the next number (0.10) and a tag, and is renamed *Office* in Properties. |
| [06-room-separator-splits-living-dining.png](06-room-separator-splits-living-dining.png) | A room separator drawn across the open Living / Dining room. Living / Dining shrinks to 14.18 m², and a new room *Living* (11.39 m²) fills the other side. Together they still make 25.57 m². |
| [07-ceiling-tool-and-properties.png](07-ceiling-tool-and-properties.png) | The Ceiling tool clicked in the Study: the ceiling goes over the room and follows its walls. *Properties* shows its height (2500), thickness, *Suspended grid* with a 600 tile, and material. |
| [08-reflected-ceiling-plan.png](08-reflected-ceiling-plan.png) | The same plan with *Ceiling plan* on: the ceilings and their tile grids (the selected Study ceiling in orange, the WC, store and utility ceilings), and the lights and smoke detectors that hang from them. Furniture and floor items are left out. |
| [09-room-tag-fields.png](09-room-tag-fields.png) | A room tag selected: *Shows* ticks which fields it writes. Height and floor finish are added here. The height reads 2500 because of the new ceiling. |
| [10-room-schedule-live.png](10-room-schedule-live.png) | The room schedule in the app, read live from the model. It shows the edits above: Study 2500 high, Living / Dining 14.18 m², 0.10 Living. Ten rooms on the ground floor. |
| [11-room-schedule-sheet-in-app.png](11-room-schedule-sheet-in-app.png) | Sheet A603 Room Schedule open in the app, with the title block. |
| [12-A603-room-schedule-sheet.png](12-A603-room-schedule-sheet.png) | A603 as printed for the unchanged starter house: 16 rooms grouped by level, each level with its room count and total area, and 211.54 m² in all. The level headings run on across the empty cells. |
| [13-boiler-out-of-store-door-D09.png](13-boiler-out-of-store-door-D09.png) | Before (main) and after: the combi boiler stood in the swing of door D09. It is now on the utility room's west wall. |
| [14-sofa-off-hall-door-D07.png](14-sofa-off-hall-door-D07.png) | Before and after: the three-seat sofa and a light switch sat in the swing of hall door D07. The sofa now stands clear of it and the switch sits by its latch. |
| [15-socket-out-of-loggia-doorway-D03.png](15-socket-out-of-loggia-doorway-D03.png) | Before and after: a double socket stood in the loggia doorway by D03. It is now gone from the doorway. |
| [16-first-floor-bed-and-bedroom-1-door.png](16-first-floor-bed-and-bedroom-1-door.png) | Before and after: in bedroom 1 the bedside table stood in door D14's swing. The bed and both tables have moved east, clear of the swing. |
| [17-first-floor-partitions-stop-at-ceilings.png](17-first-floor-partitions-stop-at-ceilings.png) | Section A-A, before and after: the first-floor partitions used to run up into the gable roof. They now stop at the 2500 bedroom and landing ceilings. |

In the old starter house, `creabim check` lists 17 objects in the way of doors and windows. In the new one it lists none.

## What is new

- **Rooms** (*Create › Room*). *Fill room (3)* clicks inside walls, and the room then follows the walls. *Polygon (1)*, *Rectangle (2)* and *From sketch (4)* draw an outline where nothing closes the space off. Each new room gets the next number on its level and a tag in the level's plans. If a room stops being enclosed, it says *Not enclosed*.
- **Room separators** (*Create › Room separator*): two clicks. An end drawn a little short of a wall, or on its centre line, still reaches the wall.
- **Ceilings** (*Create › Ceiling*). Click inside a room and the ceiling follows it, or draw an outline. Each ceiling has a height, thickness, suspended grid and material. Rooms take their height from the ceiling over them. Walls can attach their top to a ceiling.
- **Views:** *Reflected ceiling plan* and *Colour rooms* in a plan's (or section's) Properties and in the view options. *Tag rooms* tags any untagged rooms.
- **Room tags** show the fields ticked in *Properties › Shows*. They can be dragged in plans, and the Tag tool tags rooms and ceilings too.
- **Schedules:** *File › New schedule › Rooms*, grouped by level with totals. `creabim schedule <model> rooms` prints it as CSV. The starter house has it on sheet A603.
- **Checks:** *Create › Check doorways* selects every object standing in a doorway, a window or a door's swing. `creabim check <model>` lists them and fails if there are any. Objects placed against a wall with the Object tool now slide along it to stay clear of its doors and windows.
- **Older projects:** *Create › Rooms from room notes* (or `creabim rooms-from-notes <model> <out>`) turns notes like "LIVING / 37.2 m²" into rooms wherever walls close them off.
- **IFC:**
  - Rooms export as **IfcSpace**, with their quantities and property sets, an **IfcRelSpaceBoundary** for each bounding wall (separators as virtual boundaries), and their ceilings through **IfcRelCoversSpaces**.
  - Ceilings export as **IfcCovering** of type *CEILING*.
  - Both import back, and both round-trip through IFC4 and IFCX (`crates/creabim-ifc/tests/rooms.rs`). The starter house IFC passes `tools/validate_ifc.py`, with 16 IfcSpace and 9 IfcCovering.
- **Help:** new pages *Rooms* and *Ceilings*, and the *Starter house* page is updated.

## How to try it

- Start the app: the starter house opens with its rooms and ceilings. Turn on *Room colours* and *Ceiling plan* in the view options.
- Pick **Room**, press `3`, and click inside walls. Pick **Room separator** and click across an open-plan room, then place a room on the new side. Pick **Ceiling** and click inside a room.
- Open *Schedules › Room Schedule* or sheet *A603*.
- From the command line:
  - `creabim sample house.creabim.json`
  - `creabim schedule house.creabim.json rooms`
  - `creabim check house.creabim.json`
  - `creabim ifc house.creabim.json house.ifc && python tools/validate_ifc.py house.ifc`

## Known gaps closed from earlier phases

- *Rooms are notes, not room elements* (phase1-starter-house): rooms are elements now, and their areas follow the walls.
- *There is no room schedule* (phase1-starter-house): there is a room schedule now, on sheet A603.
- *The interior first-floor partitions run up to the vaulted roof rather than stopping at a ceiling* (phase1-starter-house): they stop at the ceilings now.
- Furniture and fixtures standing in doorways in the starter house (the markup items above): moved, and `creabim check` reports none.

## Known gaps

- A ceiling is drawn only in ceiling plans, sections and 3D. Selecting one in an ordinary floor plan doesn't highlight it there.
- In Section A-A, a short outline of a partition beyond the cut shows above the ceiling line over the landing.
- The Room and Ceiling tools' `1`–`4` keys work only while the pointer is over the drawing, as with the Slab tool.
- `Ctrl+S` pressed over a view is read as the Scale command rather than Save. Use *File › Save*.
- Rooms in sections are coloured only when *Colour rooms* is on. There are no room tags in sections or elevations.
- IfcSpace from other programs keeps the outline of its body. It does not follow the walls until it is placed again with the Room tool.
