# Columns and beams

## Columns

1. Pick the **Column** tool.
2. In the tool options choose *Square* or *Round* and the *Size*, or a column **Type**.
3. Click where each column goes.

A column runs from its level up to the next one, unless you tie its top to a level or give it a fixed height in *Properties*. *Properties* also has its profile and rotation. Drag its centre handle to move it, its top handle to change its height and its base handle to change its base offset.

## Over several levels

A column's *Properties* has four rows that say where it starts and stops:

- **Base level** and **Base offset**: the level it stands on, and how far above that level its foot is.
- **Top level** and **Top offset**: the level its top is tied to, and how far above that level the top is. *Unconnected* means a fixed height, or up to the next level.

When the top is tied to a level, the column stretches when either level moves.

For example, say a column was drawn on Level 5 and needs to stand on Level 3 and run up to Level 5:

1. Select it and set its *Top level* to Level 5.
2. Set its *Base level* to Level 3. The top stays where it is, so it now runs through Level 4.

Changing the base level always keeps the top where it is. A column that ran up to the next level stays tied to that level.

In a section or elevation, drag its top or base handle up or down. When the top is tied to a level, dragging it changes the *Top offset*.

A column that runs through a level is drawn cut in that level's plan too.

Copying a column to other levels with **Copy to levels…** keeps its span in storeys. See [Levels](levels#copying-and-moving-to-other-levels).

## Beams

1. Pick the **Beam** tool. Set *Width* and *Depth* in the tool options, or a beam **Type**.
2. Click the beam's start, then its end. Keep clicking to chain beams. `Esc` stops.

By default a beam's top sits at the top of the storey (*Top offset* 0), hanging under the next level. Its handles move each end and its top.

## In drawings and schedules

Cut columns are filled solid in plans; beams above the cut are dashed. For footings under columns, see [Footings](footings). *File › New schedule › Columns and beams* lists every one with its type, level, material, section and length.

A beam inside a wall is hidden in most drawings: use [X-ray](visibility#x-ray) or *Hidden* in a view to see it, and `Tab` to [pick it](select#picking-what-is-underneath).
