# Levels

A **level** is a floor of the building at a height, such as *Ground Floor* at ±0 or *First Floor* at +3000. Each element stands on a level, at an offset from it. Move the level and everything on it moves too. Each level usually has its own floor plan.

## The Levels window

Open it with *View › Levels…* (also in *File* and *Create*), the **⋯** on the browser's *Levels* group, *Levels…* in a level's *Properties*, or by picking the Level tool anywhere but a section.

- Levels are listed highest first, with their name, elevation, storey height, the gap to the next level (with **Fit** when they differ) and how many elements sit on each.
- **Add level above the top** adds one a storey up. **Add at** adds one at an elevation you type. With *Make a floor plan for each new level* ticked, each comes with its floor plan.
- Rename a level, change its elevation or height, duplicate it, open or make its plan.
- **Delete** asks what happens to what is on it: move those elements to another level (they stay where they are in space) or delete them too.

![The Levels window](img/levels.jpg)

Plans named after a level follow when you rename it ("Level 2 plan" becomes "Roof plan").

## Levels in sections

In a section or elevation, each level is a line with a head showing its name and elevation.

- Drag a level line (or its head) up or down to move the level and everything on it. It snaps to model points.
- With the **Level** tool, click where the new level's line starts (that sets its height), then where it ends. Its floor plan is made with it.

## Walls tied to levels

A wall's top can be set to *Up to* a level: then the wall grows or shrinks when that level moves. See [Walls](walls#base-and-top).

## Underlay

A plan can show another level faintly underneath, to trace and snap to. Pick it in the tool options strip (*Underlay*: None, Level below, Level above or any level), in the view's *Properties*, or *View › Underlay*.
