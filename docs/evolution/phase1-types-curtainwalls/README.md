# Element types, curtain walls and openings

Walls, doors, windows, slabs, roofs, columns and beams can now be **instances of an
element type**, as in Revit and ArchiCAD. The type holds the shared parameters: an
assembly or thickness and material, opening sizes and frames, a profile, or a curtain
grid. The instance keeps its placement and any value it overrides. Editing a type
updates every instance of it, except where an instance overrides that value.

A **curtain wall** is a wall with a grid: bays along it and rows up it (by count or
spacing), mullions with a width and depth, and glass or solid panels. It is drawn in
plan, sections, elevations and 3D, and doors and windows can sit in its panels.

Openings gained three kinds: **holes in slabs and roofs**, a bare **wall opening**
(a hole with no door or window in it), and a **shaft** that cuts every slab and roof
it lies inside between a base and a top level.

All of these go through IFC 4 and IFCX and come back:

- `IfcWallType`, `IfcCurtainWallType`, `IfcDoorType`, `IfcWindowType`,
  `IfcSlabType`, `IfcRoofType`, `IfcColumnType` and `IfcBeamType`, linked with
  `IfcRelDefinesByType`;
- `IfcCurtainWall` with its `IfcMember` mullions and `IfcPlate` panels (`IfcRelAggregates`);
- `IfcOpeningElement` with `IfcRelVoidsElement` for holes, wall openings and shafts.

| Screenshot | What it shows |
|---|---|
| `01-types-window.png` | *Materials › Types…*. The types are grouped as walls, curtain walls, doors, windows, slabs, roofs, columns and beams, with each type's instance count. The "Storefront, 4 bays" curtain wall type is open: its IFC class and a summary, the grid along (count, 5 bays) and up (spacing, 2400 mm), the mullion width and depth, glass or solid panels, and the panel thickness. *Select all instances* and *Assign to the selection* are at the bottom, with *Add starters*, *Duplicate* and *Delete* below the list. (This shot was taken before the status-line wording was fixed to "1 instance follows".) |
| `02-properties-type-and-override.png` | A window whose width was changed to 1500 mm. The Type picker shows "Window 1200 × 1300, two panes · 9 instances", and *Overrides: Width* is shown in orange, with *Reset to type*, *Update type*, *Select all instances*, *Edit type…* and *Make a type of it*. |
| `03-swap-type-of-nine-windows.png` | All nine windows selected with *Select all instances*. The Type picker in Properties swaps the type of the whole selection in one step. |
| `04-wall-tool-type-picker.png` | The Wall tool's option strip starts with a *Type* menu. The type picked there is used for the next walls drawn, and the menu also opens *Types…*. |
| `05-curtain-wall-drawn-with-wall-tool.png` | A wall drawn with the "Curtain wall 1500 grid" type picked in the strip. It comes out as an `IfcCurtainWall`, and Properties show its grid fields under the *Curtain wall* checkbox. |
| `06-curtain-walls-3d.png` | 3D view of the typed sample house with the new curtain wall in front of it: mullions and glass panels. |
| `07-ff-slab-shaft-and-flue-holes-from-above.png` | 3D view from the top with the ground floor slab hidden. The first floor slab has the stair shaft's hole (outlined, the storefront visible below it) and its own small flue hole. |
| `08-section-shaft-cuts-first-floor-slab.png` | Section A: the first floor slab stops at the stair shaft. |
| `09-north-elevation-storefront-grid-and-door.png` | North elevation. The ground floor "Storefront, 4 bays" curtain wall shows its mullions, its transom and the garden door set in a panel. |
| `10-east-elevation-duct-opening-in-typed-wall.png` | East elevation. The bare duct opening (600 × 400 at a 2.2 m sill) is in "GF Wall 2", which is selected. Its Properties show the wall type "Exterior wall 365, brick cavity · 7 instances". |
| `11-wall-schedule-grouped-by-type.png` | The wall schedule with *Group by type* ticked in the schedule view's Properties. Heading rows name each type and its count, and clicking a heading selects all of its instances. A Type column appears whenever anything listed has a type. |
| `12-first-floor-slab-holes-and-shaft.png` | First floor plan with the slab selected. Both holes are drawn crossed. Under *Holes*, Properties list the slab's own hole with *Remove*, note "Cut by 1 shaft too", and offer *Draw a hole*. |
| `13-drawing-a-slab-hole.png` | *Draw a hole* in progress: the hole outline is drawn with the usual outline tool (lines or arcs, snapping, typed lengths). |
| `14-slab-hole-cut.png` | The new hole is cut in plan and in 3D, and the area updates to 55.22 m². It is one undo step. |
| `15-stair-shaft-in-ground-floor-plan.png` | Ground floor plan with the stair shaft selected. Its Properties show the base and top levels with offsets, the span (±0 to +3000 mm) and the area. |

## How to try it

1. *File › New typed sample house* (or `creabim sample-typed house.creabim.json`).
   Every wall, door, window and slab is typed, and the back wall is a storefront
   curtain wall with a garden door. The sample also has a duct opening in the east
   wall, a stair shaft from the ground floor to the first floor, and a flue hole in
   the first floor slab.
2. Open *Materials › Types…* (or *Create › Types…*, or the *Types* group in the project browser). Edit
   a type and watch every instance follow. In any project, *Add starters* adds a
   starter set of types.
3. Select an element. Pick another type in Properties, or change a value to
   override it, then use *Reset to type* or *Update type*. *Select all instances*
   selects every element of that type, and the Type picker then swaps all of them.
   *Make a type of it* turns an untyped element into a new type.
4. With the Wall, Door, Window, Slab, Roof, Column or Beam tool active, pick a type
   in the *Type* menu at the start of the option strip. *Create › Curtain wall*
   starts the Wall tool with a curtain wall type.
5. Select a slab or roof and press *Draw a hole*. Use *Create › Wall opening* to
   place a bare hole in a wall, and *Create › Shaft* to draw a shaft. A shaft
   reaches the next level up by default, and its levels are set in Properties.
6. In a schedule view's Properties, tick *Group by type*.
7. Export with *File › Export › IFC4 (.ifc)* or *IFC5 (.ifcx)* and import the file again. The types, the
   instance-to-type links, the curtain grid and the holes all come back. CI checks
   the export of the typed sample with `tools/validate_ifc.py`.

Projects saved before this change still open: elements without a type stay untyped,
and schedule views read without grouping.

## Known gaps

- **Frame overrides in IFC:** a typed door's or window's frame is written only on its
  IFC type. A per-instance frame override is lost on export.
- **Hole outlines:** holes are drawn as polygons only. They have no rectangle mode and
  no vertex grips. To change a hole, remove it and draw it again.
- **Redrawing a shaft:** a shaft's outline cannot be redrawn yet. It can be moved,
  and its levels can be changed.
- **Openings above the cut plane:** a wall opening above the plan's cut plane, like
  the duct opening, is not drawn dashed in plan.
- **Curtain panels:** individual panels can be swapped in the data, but the only UI
  for them is resetting them.
- **Tool type picker:** editing the thickness or size in a tool's option strip clears
  that tool's type, because the size no longer matches it.
- **Override display:** overrides are listed in a single summary line. Each field has
  no marker of its own.
- **Shaft highlight:** a selected shaft is highlighted on its base level's plan only.
  On the levels above it shows only as the hole it cuts.
