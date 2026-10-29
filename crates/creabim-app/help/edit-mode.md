# Edit mode

Edit mode opens one element so every one of its grips shows, in every view it is in: plans of each level it passes through, sections, elevations and 3D. Use it to reshape what was drawn, not just move it.

## Opening and closing it

- Select the element, then click **Edit** at the top of *Properties*;
- or double-click the selected element (or one of its corners) in any view;
- or press `Enter` with it selected.

`Esc`, `Enter` again, *Finish editing* in the right-click menu, or selecting something else closes it. *Properties* says *Editing* while it is open, and the status bar lists what you can do.

## Outlines: one way for all of them

Slab outlines and their holes, roof outlines and their holes, shafts, sketches and free railing paths all work the same way. Their outlines are traced in orange, with:

- a **square grip** on each corner;
- a **diamond** (◇) in the middle of each edge;
- a **round grip** just outside each edge of a closed outline.

| Do this | And |
|---|---|
| Drag a corner | It moves, with snaps; press `Tab` to type how far |
| Drag an edge's round grip | The edge moves square to itself |
| Drag a ◇ | A new corner comes out of the edge |
| Click a corner | It is picked (filled orange); `Shift+click` picks more |
| Drag a box off the grips | Picks every corner inside it (`Shift` adds) |
| `Delete` or `Backspace` | Removes the picked corners, in one undo step |
| Double-click a corner | Removes it |
| Click inside a hole | Picks the hole: `Delete` removes it |
| Drag inside a hole | Moves the whole hole |
| Right-click a corner, an edge or a hole | Its menu (below) |
| `Esc` | Cancels a drag; then drops the picks; then leaves Edit mode |

`Delete` in Edit mode never deletes the element you are editing: with no corner picked it only says to pick one. A closed outline keeps at least three corners, an open one (a railing's path, an open sketch) two.

Selected but not in Edit mode, an outline shows only its own corners and edges, to drag. Its holes, its diamonds and picking and removing corners need Edit mode, for every element alike.

A sketch with arcs or constraints keeps its corners: you can drag them, but not add or remove them, and the tip by a corner says so.

## The right-click menus

- **On a corner:** *Remove corner* (or *Remove 3 corners* when several are picked), *Select all corners*.
- **On an edge:** *Add corner here* (where you clicked), *Make arc* or *Straighten* (slab outlines), and on a hole's edge *Remove hole*.
- **Inside a hole:** *Remove hole*, *Move hole*.
- **Anywhere else:** *Select all corners*, *Deselect corners*, *Finish editing*.

## Shapes are checked as you drag

While you drag, the shape is checked. If it would be broken, it is drawn **red** with the reason by the pointer and in the status bar, and it is never kept: let go and the last good shape stays. These are refused:

- an outline or a hole that would cross itself;
- a hole outside its slab or roof, or across its edge;
- two holes overlapping.

A corner dropped on the one next to it merges with it, instead of leaving an edge of no length.

## Hover, lengths and typing

Rest the pointer on a grip: it grows, and a tip says what dragging, clicking, `Delete` and right-clicking it do. While you drag a corner, the lengths of the edges either side show by the pointer; an edge or a hole shows how far it has moved.

Press `Tab` (or just start typing digits) during a drag to type a length: the grip goes that far from where you grabbed it, the way you were dragging. `Enter` applies it, `Esc` goes back to the pointer.

Every gesture is one undo step with a clear name, such as *Remove 2 corners of Floor slab* or *Move hole 1 of Floor slab*.

## Shafts

A shaft's corners and edges and the two round grips in its middle (its **base** and its **top**) show whenever it is selected; Edit mode adds its add-corner diamonds and picking. Every slab and roof it cuts follows as you drag. A shaft only cuts a slab or roof that holds it whole: drag it across an edge and the status bar warns that it doesn't cut that slab. See [Shafts](holes#shafts).

## Other elements

Walls, doors, windows, wall openings, columns, beams, stairs and the rest show their usual [handles](handles) in Edit mode, in every view.
