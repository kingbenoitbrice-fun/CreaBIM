# Site: terrain, hard landscape and planting

The **Site** tools (*Create › Site*) model the ground around the building: the terrain, paved and gravelled areas, and trees and shrubs. Cars and people come from the catalogue.

## Terrain

1. Pick the **Terrain** tool.
2. Set its *Depth* in the tool options.
3. Draw the ground's outline like a slab: `1` Polygon, `2` Rectangle, `3` Fill room or `4` From sketch.

New terrain is flat at its level and covered in *Grass* from the material library. To shape it, add **spot heights** in *Properties*. Click *Add spot height*, then type its X and Y position and its height above the level. The surface runs through every spot height and slopes between them. The outline's corners stay at the terrain's *Top* unless you put a spot height on them, so one spot height in the middle makes a mound and spot heights on the corners tilt the whole site. Set *Contours every* to draw contour lines in plans, for example every 250 mm. Spot heights show in plans as small crosses.

## Hard landscape

**Hard landscape** areas are slab-like layers on the ground, each with its own material and surface pattern.

1. Pick the **Hard landscape** tool.
2. In the tool options, pick what to draw:
   - **Paving**: 600 × 400 concrete flags.
   - **Driveway**: block paving laid in stretcher bond.
   - **Gravel**: stippled.
   - **Decking**: boards.
   - **Asphalt**: no pattern.
3. Draw the area's outline like a slab.

The picks are only starting points. In *Properties*, change the area's *Top*, *Depth* and **Pattern**:

- **Units**: their length, width, staggered courses and angle.
- **Lines**: their spacing and angle.
- **Stipple**: its spacing.

Plans draw the pattern over the area, and 3D shows the area in its material. An area drawn inside flat terrain sits flush: the ground leaves room for it.

## Planting

1. Pick the **Planting** tool.
2. In the tool options, pick a **Tree**, **Conifer**, **Palm** or **Shrub**, and its *Canopy* (the spread of its crown) and *Height*.
3. Click to plant. Keep clicking to plant more.

In plan, each plant is a symbol as wide as its canopy: a scalloped crown with its trunk for a tree, a ringed star for a conifer, fronds for a palm, a smaller scalloped outline for a shrub. In 3D, sections and elevations it is a simple form: a trunk under a round crown, a cone, a tall trunk with a head of fronds, or a low mound. Give it a material to change the colour of its crown.

## Cars and people

*Create › Site › Cars and people…* opens the [Catalogue](furniture) at **Entourage**: a car, a person and a child, for scale. Place them like any other object.

## Reshaping

Terrain and hard landscape areas have the same grips as a slab: drag a corner or an edge, and open them in [Edit mode](edit-mode) (`Enter`) to add or remove corners or change an area's holes. Move plants with the [Move](move) tool.

## Exchange and visibility

Terrain exports to IFC as *IfcGeographicElement* (TERRAIN), plants as *IfcGeographicElement*, and hard landscape as *IfcCivilElement*. The IFCX format reads them back as the same editable elements. Views can hide them through the *Terrain and hard landscape* and *Planting* categories (see [Visibility](visibility)).
