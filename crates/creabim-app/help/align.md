# Align, Extend and Stretch

The **Align** tool lines things up against a reference, the way Revit's Align does. It is the last icon in the tool row, and in *Edit › Align to a reference…* and the right-click menu of a 2D view.

It always works in two clicks: **first the reference, then what to change**. The reference stays picked, so you can line up several things against it. `Esc` lets it go.

Pick the mode in the tool options strip.

## Align

1. Click a reference: a wall face, a slab edge, a grid line, a guide, a temporary guide line, or a centre line.
2. Click the edge of what should line up with it, or its centre line.

The element turns parallel to the reference (the shorter way round) and moves flush against it. Clicking a door's or window's edge moves its wall.

## Extend to it

Click the reference, then near the end of a wall, beam or grid line: that end stretches to meet the reference. Walls joined to it follow.

## Stretch to it

Click the reference, then an edge: only that face moves to the reference, and the opposite face stays put. As you hover, the edge that Stretch will take is highlighted.

- In a section, click a slab's top under a line: the slab thickens to it. A wall's top changes its height. A column's top can stretch up to the underside of a beam.
- In a plan, a wall's end stretches like Extend.

The edge must be parallel to the reference, within 5°. Stretch measures from the point of the edge nearest your click.

![Stretching a slab's top to a line in a section](img/stretch.jpg)

## Centre lines

Elements also offer their centre lines, drawn dashed in purple when you hover the element:

- a wall's location line, and a beam's centre, across and through its depth;
- both centre axes of a column;
- the middle of a slab, mass, roof or stair, each way;
- the middle of a door or window, across its wall and at mid-height.

A centre line can be the reference, or what lines up: click a beam's centre line after a column's to centre the beam on the column. By its centre line a door or window slides along its wall, instead of moving the wall. Stretch moves faces, so it doesn't take a centre line as what to change.

## In 3D

Align, Extend and Stretch also work in the 3D window and in 3D views.

1. Click the reference: a face, an edge, a corner, a centre line, a grid line, or empty space (the working level's plane; in a 3D view, the lowest level).
2. Click a face of what to change. As you hover, it shows in orange with the distance it will move.

- **Align** moves the whole element until that face is flush with the reference. Upright elements turn to it first.
- **Stretch to it** moves only that face, the way [Move face](move#move-a-face-push-and-pull) does: a wall's top, a slab's underside, a column's top.
- **Extend to it** runs the nearer end of a wall, beam or grid line to the reference. For anything else it stretches.

`Tab` picks what is behind, so you can reach a face buried among others. The reference stays picked as you move between 3D panes.

## Where it works

Plans, sections, elevations, the 3D window and 3D views. Each click is one undo step.
