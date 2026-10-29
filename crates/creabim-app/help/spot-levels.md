# Spot levels

A **spot level** writes the height of a point: the top of a floor, a terrace or a step. In a plan, it is also the **level marker** that says what level the plan is at.

## Placing a spot level

1. Pick the **Spot level** tool (in the Annotation group).
2. Click the point:
   - In a section or elevation, click a floor, slab or roof edge. A small triangle stands on the point.
   - In a plan, click a slab: the spot level reads the slab's top. Click anywhere else and it reads the plan's own level, as a level marker, drawn with a target symbol.
3. Click where its text goes.

The spot level follows what it was put on. Move the slab, or change its level, and the height changes.

## What it reads

Select it and type a pattern in **Reads** in *Properties*. These words in braces are filled in:

- `{elevation}`: the height in the project's units, with its sign: +450.
- `{m}`: in metres to two places: +0.45. `{m3}` gives three places.
- `{mm}`: in millimetres: +450.
- `{level}`: the name of the level at or below the point.

`{m} FFL` reads "+0.45 FFL", and `FFL {m} ({level})` reads "FFL +0.45 (Ground floor)". A new spot level reads like the last one made, so set the pattern once.

## Moving and styling

With the Select tool, drag its text to move it. A spot level takes a [style](styles) like a dimension. Spot levels are dimensions for [visibility](visibility): a view that hides dimensions hides them too.
