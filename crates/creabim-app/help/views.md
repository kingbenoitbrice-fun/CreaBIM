# Views

In CreaBIM a **view** is a way of looking at the model, kept in the project: a floor plan, a section, an elevation, a 3D view or a schedule. Views are drawn from the model each time, so they never go out of date. They save, undo and update with it.

## Kinds of views

- **Floor plans**: the building cut at a height above a level (the *Cut height*, usually about 1.2 m) and seen from above. Cut walls are drawn heavy and hatched; doors show their swing.
- **Sections** and **elevations**: see [Sections and elevations](sections).
- **3D views**: saved, parallel 3D views, such as *3D View South-East*, to place on sheets. Set the angle with *Altitude*, and turn on *Shaded*.
- **Schedules**: tables of elements. See [Schedules](schedules).

## Opening a view

- Click it in the project browser: they are grouped as *Floor plans*, *3D views*, *Sections*, *Elevations* and *Schedules*.
- Or use the view switcher at the top right.

The view opens in the 2D pane. You can draw and select in it with every tool.

## The view's settings

The right of the tool options strip shows the open view's settings:

- *Cut height* (plans), *Depth* (sections and elevations) or *Altitude* (3D views);
- *Shaded*: surfaces shaded in their material colours;
- *Hidden* and *In colour*: elements hidden inside others drawn dashed, and tinted in their colour (see [Visibility](visibility#hidden-elements));
- *X-ray*: see through walls and slabs (see [Visibility](visibility#x-ray));
- *Tag openings*: tag every door and window that has no tag;
- *Underlay* (plans): another level shown faintly underneath;
- *Visibility*: what this view leaves out.

With nothing selected, *Properties* shows the same and more, such as the view's scale.

## Ordering the browser

Drag a view up or down in its group. The group then keeps your order (it says *Your order*); its sort button puts it back in automatic order (plans by level).
