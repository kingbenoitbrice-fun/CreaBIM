# Phase 1, editing safety: picking corners, one grammar for every outline, select similar

These are reference screenshots for the round of 28 October. Shots 01–08 come from the starter house (the Garden terrace slab, Ground Floor plan). Shots 09–12 come from `TropicalVilla_REV00.creabim`.

| Screenshot | What it shows |
| --- | --- |
| [01-selected-outline-grips.png](01-selected-outline-grips.png) | The terrace slab is selected but not in Edit mode. It shows a square grip on each corner and a round push grip on each edge, the same as every other outline. Holes, diamonds and picking only come in Edit mode. |
| [02-picked-corners-and-tip.png](02-picked-corners-and-tip.png) | Edit mode is open. A diamond was dragged out to add a fifth corner. Then one corner was clicked and a second was Shift+clicked: both are filled in, and the status bar says *2 corners selected: Delete removes them.* The pointer rests on another corner and shows its tip. |
| [03-corner-menu.png](03-corner-menu.png) | Right-clicking a picked corner gives *Remove 2 corners* and *Select all corners*. |
| [04-edge-menu.png](04-edge-menu.png) | Right-clicking an edge gives *Add corner here* and *Make arc*. *Straighten* shows instead on an arc. |
| [05-invalid-outline-red.png](05-invalid-outline-red.png) | A hole has been drawn, and one of its corners is being dragged out across the slab's edge. The shape turns red, with the note *Not allowed: hole 1 would cross the outline*. The status bar says that releasing keeps the last good shape and that Esc cancels. Nothing is committed. |
| [06-hole-menu.png](06-hole-menu.png) | After Esc cancelled the drag, right-clicking inside the hole gives *Remove hole* and *Move hole*. |
| [07-box-select-drag.png](07-box-select-drag.png) | In Edit mode, a drag that starts off the grips draws a box… |
| [08-box-select-result.png](08-box-select-result.png) | …which picks every corner inside it, on the outline and on the hole: *4 corners selected*. |
| [09-wall-menu-split-and-similar.png](09-wall-menu-split-and-similar.png) | TropicalVilla: right-clicking a wall gives the new *Split wall here* and the *Select similar* submenu. |
| [10-select-similar-menu.png](10-select-similar-menu.png) | The *Select similar* submenu: same type in the view (Shift+A) or the project (Ctrl+Shift+A), same category in the view or the project, and similar tags (greyed out when a tag is not selected). |
| [11-same-type-selected.png](11-same-type-selected.png) | *All of this type in the view*: the villa's walls have no types, so it picks the 46 walls in this view that share the 200 mm thickness. |
| [12-villa-roof-edit-mode.png](12-villa-roof-edit-mode.png) | The carport roof, with Edit mode opened by Enter: the same grips as the slab. Pressing Delete with nothing picked only says *Select a corner to remove it…*. The roof stays, at 380 elements. |

## How to try it

- **Edit mode:** select a slab, roof, shaft, sketch or free railing. Then click *Edit* in Properties, press Enter, or double-click one of its corners.
  - **Pick corners:** click a corner to pick it, Shift+click to add or drop one, or drag a box from empty space. Click inside a hole to pick the hole.
  - **Delete** or **Backspace** removes what is picked, as one undo step (*Remove 2 corners of Garden terrace*). It never deletes the element. With nothing picked, it says what to do.
  - **Double-click a corner** to remove it.
  - **Add a corner:** drag a diamond. A click or double-click on a diamond changes nothing.
  - **Right-click** a corner, an edge, a hole or empty space for the menu.
  - **Esc:** the first press cancels a drag, the next clears the picks, and the last finishes editing.
- **Checks while dragging:** an outline that would cross itself, a hole that would cross the outline, leave it or overlap another hole, and too few corners all show red with the reason, and none of them is committed. Dropping a corner onto its neighbour merges the two. A shaft dragged so it only partly covers a slab or roof says so in the status bar.
- **While dragging:** rest the pointer on any grip for a tip. During an outline drag the lengths of the edges next to it show. Press Tab (or start typing a number) to type a length, then Enter.
- **Select similar:** right-click a selection, or use Edit › Select similar, Shift+A or Ctrl+Shift+A. *Similar tags* picks tags of the same kind, on the same category, in the same view. When the selection mixes categories, *Keep only …* narrows it to one.
- **New grips:**
  - Objects have a turning grip in front of them and an arrow to turn them round.
  - Columns have a turning grip, and grips for width and depth.
  - Stairs have a turning grip.
  - Walls have a slide grip off the face opposite the thickness grip, and *Split wall here* in the right-click menu.
  - Doors and windows have a slide grip in their middle.
  - Beams have a width grip.

## Known gaps

- A shaft that runs across a slab's edge is **reported** while it is dragged. It is not clipped to the slab.
- Footings have no polygon outline, and the app has no free 2D region element, so neither is in the shared grammar.
- Grips not done yet:
  - snapping an object to a wall face and sliding it along a host wall (objects have no host)
  - a wall's base offset in section
  - a beam's height in section
  - typing an opening's distance to the wall end (Tab typing works on the slide grip, as a distance moved)
  - flipping a stair and its landing grips
  - move and rotate grips on free meshes
  - leader and witness grips on tags, notes and dimensions
  - dragging the end of a level's label
  - crop grips on sheet viewports
- In Edit mode, dragging the body of the element no longer moves it: that drag draws a selection box instead. To move the element, finish editing first.
