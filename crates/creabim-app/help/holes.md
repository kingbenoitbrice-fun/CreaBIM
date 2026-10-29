# Holes, wall openings and shafts

Three ways to cut a hole in the building, each exported to IFC as an opening.

## Wall openings

A **wall opening** is a rectangular hole in a wall with no door or window in it, for a duct or a pass-through.

1. Pick the **Wall opening** tool (*Create › Wall opening*).
2. Click on a wall.
3. Set its width, height and sill in *Properties*.

## Holes in slabs and roofs

1. Select a slab or a roof.
2. Click **Draw a hole** under *Holes* in *Properties*.
3. Draw the hole's outline, with lines or arcs, snapping as usual.

The hole is cut in plan and in 3D, and the area updates. *Properties* lists the slab's holes, each with *Remove*. To reshape a hole, open the slab in [Edit mode](edit-mode) (`Enter`): each hole shows its corners, edges and add-corner diamonds. Drag inside a hole to move it; click inside it and press `Delete`, or right-click it, to remove it. A hole can't be dragged outside the slab, across its edge or over another hole.

![A first floor slab with a hole and a shaft](img/slab-holes.jpg)

## Shafts

A **shaft** is a vertical hole through every slab and roof between two levels: a stair well, a lift, a riser.

1. Pick the **Shaft** tool.
2. Draw its outline like a slab: `1` Polygon, `2` Rectangle, `3` Fill room or `4` From sketch.

By default it runs from this level to the next one up. Set its base and top levels and offsets in *Properties*. Move it and every hole it cuts moves with it.

You can select a shaft in every view it passes through: the plan of each level from its base to its top (click inside its crossed outline), sections and elevations (where it is drawn dashed) and 3D. Inside walls or slabs, `Tab` and [X-ray](visibility#x-ray) reach it.

Selected, its corners, edges, base and top have grips. Open it in [Edit mode](edit-mode) to add or take away corners. A shaft cuts only the slabs and roofs that hold it whole: dragged across a slab's edge it cuts nothing there, and the status bar says so.
