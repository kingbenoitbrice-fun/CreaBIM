# Phase 1: building elements

Reference screenshots for the new building elements: columns, beams, roofs, stairs and railings. They were taken from the sample house with a few of each added.

Each element has its own tool in the palette, next to Wall, Door, Window, Slab and Mass. Each one also has a properties panel, handles, undo, a plan and section drawing, a 3D mesh and its own visibility category. Each exports to IFC4 and IFCX with the right class (IfcColumn, IfcBeam, IfcRoof, IfcStair, IfcRailing, with the predefined type) and to glTF and FreeCAD. Columns and beams also have a schedule.

| Screenshot | What it shows |
| --- | --- |
| [01-column-tool.png](01-column-tool.png) | The Column tool. The bar offers *Square* or *Round* and a *Size*. Each click places a column, which runs from its level up to the next one. Three columns are placed under the terrace, shown in 3D and in plan, where they are cut and filled solid. The properties show the profile, rotation, base offset and an optional fixed height. |
| [02-beam-tool.png](02-beam-tool.png) | The Beam tool, with *Width* and *Depth* in the bar. Beams chain like walls: each click starts the next beam from the end of the last one, and Esc stops. The preview shows the next beam. By default a beam hangs under the next level (*Top offset* 0). |
| [03-stair-plan.png](03-stair-plan.png) | The ground floor plan, zoomed in, with the stair selected. The properties show 17 risers of 176 mm and 2R + G = 633 mm. The plan shows the risers below the cut, the break line, the treads above the cut dashed, the walking line with its arrow, and *UP*. The beam inside the house and the terrace beams are dashed because they are above the cut. The terrace columns are cut. |
| [04-3d-roofs.png](04-3d-roofs.png) | A hip roof on the house (30°, 0.5 m overhang) and a mono-pitch terrace roof at 6° sitting on the beams and columns. |
| [05-section.png](05-section.png) | Section B runs along the house. The hip roof is cut, and its slopes show as a trapezoid because the cut runs along the ridge. The beam under the first floor and the terrace roof are cut, and the terrace column is shown beyond. The stair and its railing are in projection. Each view's *Visibility* list now has Columns, Beams, Roofs, Stairs and Railings. |
| [06-roof-gable.png](06-roof-gable.png) | The roof selected, switched from *Hip* to *Gable* in the properties. The roof slopes up from the *Eave edge* and from the edge opposite it, and the other two edges become gable ends (*Next edge* turns the ridge). The summary line gives the corners, the area with the overhang and the roof's height. |
| [07-roof-tool.png](07-roof-tool.png) | The Roof tool. It draws an outline like the Slab tool (Polygon, Rectangle, Fill room, From sketch), and the bar sets *Flat*, *Mono-pitch*, *Gable* or *Hip* and the *Pitch* first. |
| [08-structure-schedule.png](08-structure-schedule.png) | *New schedule > Columns and beams*: every column and beam with its type, level, material, section and length, and the total length at the bottom. The material can be changed from the schedule. |
| [09-l-stair-railings.png](09-l-stair-railings.png) | An L-shaped stair placed with two clicks (start, then the direction it climbs), with *L-shaped* on in the bar. The landing comes after 8 risers and the stair turns left. *Add railings* puts a railing on both sides that follows the flights and the landing. The railings are tied to the stair: they move with it and are deleted with it (*Delete (and 2 attached)*). |

## How to try it

- **Column**: pick *Column*, choose Square or Round and the size in the bar, then click where each column goes. Drag the centre handle to move a column and the top handle to change its height.
- **Beam**: pick *Beam* and click its start and end. Keep clicking to chain beams; Esc stops. Handles move each end and the top.
- **Roof**: pick *Roof*, choose the shape and pitch, then draw the outline the way you would draw a slab (keys 1–4 switch between Polygon, Rectangle, Fill room and From sketch). The *Base offset* raises the eaves above the level, for example 3.00 m to put the roof on top of a storey's walls. *Next edge* chooses which edge is the eave of a mono-pitch roof, or which edges are the gable ends.
- **Stair**: pick *Stair*, click where it starts, then click in the direction it climbs. It rises to the next level in equal risers no higher than *Max riser*. You can instead tick *Fixed rise* and type a height. Turn on *L-shaped* for a landing, and set where the landing comes and which way the stair turns in the properties.
- **Railing**: pick *Railing* and click inside a stair to add railings on both its sides. Otherwise, click points along a path and click the last point again to finish. Stairs also have an *Add railings* button.
- Each of the five has a category under *Visibility* in a view's properties, so they can be hidden view by view.

## Known gaps

- Gable and hip roofs need a convex outline. On an L-shaped or other non-convex outline the roof falls back to flat, and the properties say so. Valleys aren't computed yet.
- Arcs in a roof outline are flattened to straight segments.
- Stairs are straight or L-shaped. There are no U-shaped stairs, winders or spirals yet.
- A stair doesn't cut a hole in the slab above. Draw the opening in the slab yourself.
- The last riser of a stair is the edge of the floor or landing it climbs onto, so a flight is drawn up to its top tread.
- Railings are a top rail, a bottom rail and posts. There are no balusters, glass panels or handrail profiles yet.
- Beams don't join or cut back at columns or at each other. They simply overlap.
- A plan doesn't show the roof of the level below as hidden lines. The roof shows in the plan of the level it is drawn on.
- Importing IFC4 gives back columns, beams, roofs, stairs and railings as masses that keep their IFC class. IFCX brings them back as real elements.
- The Create menu (another branch) will list the five tools by itself once it is merged, because they are in the building-elements group of the palette.
