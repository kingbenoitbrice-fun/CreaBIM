# Phase 1, services fixtures: sockets, switches, lights, plumbing and HVAC

Reference screenshots for the first set of electrical, lighting, plumbing and heating and ventilation fixtures. They were taken in the furnished sample house (*File › New furnished sample house*), which now has about 40 fixtures on its two floors. Shots 01 to 08 were taken before the plan button was renamed from *Tag objects* to *Tag fixtures*.

| Screenshot | What it shows |
| --- | --- |
| [01-catalogue-electrical.png](01-catalogue-electrical.png) | The catalogue has four new categories: Electrical, Lighting, Plumbing, and Heating and ventilation. Electrical shows the double and single sockets, 1- and 2-gang light switches, the consumer unit and the smoke detector. |
| [02-catalogue-lighting.png](02-catalogue-lighting.png) | Lighting: the pendant ceiling light, the recessed downlight and the wall light. |
| [03-plan-symbols.png](03-plan-symbols.png) | Ground floor plan at 1:100, zoomed in. Each fixture draws its standard plan symbol, always solid even when it sits above the cut: a crossed circle for a ceiling light, the socket and switch symbols at the wall, a smoke detector, and radiators under the windows. |
| [04-pendant-below-the-ceiling.png](04-pendant-below-the-ceiling.png) | A pendant selected in Section B. Its type is *Placed: From the ceiling*, so the instance reads *Below the ceiling 300 mm*: it hangs 300 mm lower than the slab soffit. The type's *New ones below the ceiling* sets the default for new placements, the way sockets default to 300 mm and switches to 1100 mm above the floor. The IFC class and predefined type are pickable (IfcLightFixture, POINTSOURCE). |
| [05-section-pendant.png](05-section-pendant.png) | The same section before the change: the pendant hangs from the underside of the first-floor slab, the boiler is on the kitchen wall and the radiators are 150 mm above the floor. |
| [06-fixtures-in-3d.png](06-fixtures-in-3d.png) | The 3D view with walls, slabs and roof hidden, so the furniture and fixtures show: pendants and downlights at the ceilings, the boiler and consumer unit on the walls, radiators under the windows, and the sockets and switches at their heights. |
| [07-new-schedule-menu.png](07-new-schedule-menu.png) | *New schedule* now offers a Furniture schedule and a Fixtures schedule as well as doors, windows, walls, rooms and materials. |
| [08-fixture-schedule.png](08-fixture-schedule.png) | The Fixtures schedule (47 fixtures in the sample house) counts by type and level under category headings. Each row has its mark, type, level, mounting height ("300 above floor", "From the ceiling") and IFC class. Clicking a type row selects every instance of it. *Group by type* switches it to one row per fixture. |
| [09-tag-fixtures.png](09-tag-fixtures.png) | *Tag fixtures* on the ground floor plan tags the 23 services fixtures on the ground floor with their type marks: E for electrical, L for lighting, P for plumbing and M for heating and ventilation. Furniture and sanitary ware are left for the Tag tool, so the plan stays readable. Tags still overlap where fixtures sit close together (the switches by the doors); drag them apart. |

## How to try it

- **Place a fixture:** open the catalogue (*Create › Catalogue…* or the sofa button), pick a category and click a fixture, then click in a plan or the 3D view. Wall fixtures snap to the wall and take their type's height. Ceiling fixtures are centred on the click and hang from the underside of the slab above.
- **Change a height:** select the fixture. Use *Above the floor* for floor and wall fixtures, or *Below the ceiling* for hung ones. To change the default for new ones, use *Edit type…* › *New ones above the floor / below the ceiling*. Moving a hung fixture up with M or the push grips reduces its offset.
- **Schedule:** *File › New schedule › Fixtures* (or *Furniture*). Edit a cell to rename a type or change its offset. *Export CSV* writes the table.
- **Tags:** on a plan, click *Tag fixtures* in the view bar, or use the Tag tool on any piece of furniture or fixture.
- **IFC:** *File › Export IFC*. Each fixture is written as its own IFC class with its predefined type: IfcOutlet, IfcSwitchingDevice, IfcLightFixture, IfcElectricDistributionBoard, IfcSensor, IfcWasteTerminal, IfcBoiler, IfcValve, IfcSpaceHeater, IfcAirTerminal, IfcUnitaryEquipment and IfcFan. It shares its type and mapped geometry, and re-imports at the same height. The furnished house passes `tools/validate_ifc.py`.
- Help: *Help › Fixtures*, plus the updated *Furniture*, *Schedules* and *Tags* pages.

## Known gaps

- Only the fixtures themselves for now: there are no pipes, ducts, cable runs or circuits, and no IfcDistributionSystem or port connections.
- The catalogue pictures show ceiling fixtures from above, as if they stood on the floor.
- Changing a type between *On the floor / On a wall* and *From the ceiling* keeps the stored offset number, so existing instances are reinterpreted against the other datum and jump.
- "The ceiling" is the underside of the lowest slab above the fixture, or the level's height. Roofs and suspended ceilings are not looked at yet.
- Tags take their size from the annotation style, so at 1:100 type marks are large next to small symbols such as sockets.
- IFC5 export still leaves furniture and fixtures out, as before. IFC4 has them.
- The mounting heights in the schedule are to the underside of a wall fixture, not to its centre line.
