# Masses, modifiers and booleans

A **mass** is a free 3D shape you can model like in Blender: for early design studies, or anything that isn't a wall, slab or roof.

## Making a mass

- Pick the **Mass** tool and click to place a box.
- Or extrude a closed [sketch](sketches) with `E`.

## Editing its shape

- Click a face in the 3D view to select it, then press `E` to extrude it, or move it with `G`.
- Use [Move face](move#move-a-face-push-and-pull) to push or pull a face by a typed distance.
- Rotate, tilt and scale it with `R` and `S` (see [Move, rotate and scale](move)).

Give a mass an IFC class in *Properties* (such as `IfcRoof`) and it is exported as that.

## Modifiers

Modifiers change a mass's shape without touching it, so you can change or remove them later. Select a mass, then *Add modifier…* in *Properties*:

- **Array**: copies in a row (a pergola's rafters);
- **Mirror**: a mirrored copy;
- **Solidify**: gives a surface a thickness;
- **Subdivision**: smooths it.

Untick a modifier to turn it off, use *Up* and *Down* to reorder, *Remove* to drop it, and *Apply* (on the top one) to make it part of the shape.

## Booleans: cutting one shape from another

Select a wall, slab or mass, then under *Booleans* pick *Subtract…* and the element to cut away. The cutter stays as a see-through ghost you can select and move to change the cut. Switch it to *Union* or *Intersect*, or *Remove* it.
