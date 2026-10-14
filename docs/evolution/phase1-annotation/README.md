# Phase 1: annotation

Reference screenshots for the new annotation tools, taken from the sample house.

Plans, sections and sheets can now carry more than straight dimensions and tags:

- Angular dimensions.
- Radius and diameter dimensions.
- Arc lengths.
- Text notes with an optional leader.

Each annotation can use a named **text style**, which sets the font, the size on paper, the colour, and bold or italic. Any annotation can also take its own colour. Without a style, everything looks exactly as before.

| Screenshot | What it shows |
| --- | --- |
| [01-annotation-tools.png](01-annotation-tools.png) | The annotation group of the tool palette has five new tools: *Angle*, *Radius*, *Diameter*, *Arc length* and *Note*. Hovering over one shows how to use it. |
| [02-angle-pick-first-wall.png](02-angle-pick-first-wall.png) | The Angle tool after one click on the face of a wall: that face is outlined in orange, and the status bar asks for the second line. The angle can also be picked from three points: the corner, then one point on each arm. |
| [03-angle-preview.png](03-angle-preview.png) | After the second wall face: the arc follows the pointer until you click where it goes. The angle is measured counter-clockwise from the first line to the second, so the side you place it on picks the inside or the outside angle. |
| [04-radius-and-arc-length.png](04-radius-and-arc-length.png) | *R900* on the swing of door D02, picked by clicking the arc, and an arc length of 1414 mm on the same swing, which is selected. Properties shows the kind and the measured value, how many ends are attached (they follow the door), the offset, the text style and *Own colour*. The 90° angle from the previous step is at the lower left. |
| [05-note-new-text-style.png](05-note-new-text-style.png) | A two-line note with a leader to the middle wall. *New text style…* from the style list created "Text style 1", and its editor opens in place under the note: font, size on paper, colour, bold and italic. |
| [06-style-edited-inline.png](06-style-edited-inline.png) | The style set to Mono (Courier), 3.5 mm and bold, with a red *Own colour* on the note. Every annotation that uses the style changes with it, and Properties says how many there are. |
| [07-note-dragged.png](07-note-dragged.png) | With the Select tool, dragging a note's text moves it, and the square grip at the arrow moves the leader (snapping to what it lands on). |
| [08-note-drag-undone.png](08-note-drag-undone.png) | Ctrl+Z puts the note back ("Undid: Move note"). Placing a note, changing it, restyling it and dragging it can all be undone. |
| [09-sheet-note.png](09-sheet-note.png) | Notes on sheets: the Note tool on sheet A101, with a leader pointing into the plan's viewport. It uses the same "Text style 1" as the plan note ("Changes every one of its 2 users"). The styled plan note shows in the viewport in red bold Courier. |
| [10-pdf-export.png](10-pdf-export.png) | Part of sheet A101 exported to PDF: the 90° angle, R900 and 1414 on the door swing, and the red bold Courier note. The PDF and SVG use the real font faces (Helvetica, Times or Courier, with bold and italic). The R900 and 1414 labels overlap only because both were placed on the same small swing. |
| [11-hide-notes-in-view.png](11-hide-notes-in-view.png) | After merging the grids and visibility work, *Notes* is a category in a view's *Visibility* list. Unticking it hides the "Living room" note in this plan and on its sheets; the new R900 stays. |

## How to try it

- **Angle:** click two lines (wall faces, sketch lines, edges), then where the arc goes. Or click the corner and one point on each arm, then where the arc goes. Click away from wall midpoints: they snap to a point, not a line.
- **Radius, Diameter and Arc length:**
  - Click an arc (a door swing or a sketch arc), then where the dimension goes.
  - Or click three points on a circle, then where it goes.
  - Radius and diameter are drawn from the centre through the point you place; arc length is drawn as an arc offset from the curve.
- **Note:**
  - With *Leader* on (in the tool options), click what the note points at, then where the text goes. With it off, one click places the text.
  - The text box in Properties takes the focus with "Note" selected, so you can type straight away. Enter adds a line.
  - If the leader's point snapped onto an element, it follows that element when it moves.
- **Sheets:** pick the Note tool on a sheet, outside any active viewport, to write on the paper itself (title notes, general notes).
- **Text styles:** in any dimension's, tag's or note's Properties, open the style list and pick *New text style…*. Edit it in place, or select it in the browser under *Text styles*.
  - Size is in millimetres on paper, so text is the same size at any scale.
  - *Own colour* overrides the style for that one annotation.
- **Exports:** *File > Sheets as PDF*, *File > This view or sheet as SVG* and `creabim-cli pdf <project> <out.pdf>` all include these annotations.
- **Older files:** they open unchanged. Their dimensions are read as linear, and annotations without a style draw as before.

## Known gaps

- There is no DXF exporter in CreaBIM yet, so none of this (nor the older dimensions) goes to DXF.
- Line colours per category (the request's "if it fits naturally" part) are not in this change. `ViewStyle` is a simple style switch, and the grids and visibility work was changing `View` at the same time. Colour is per annotation and per text style for now.
- On screen, Serif is drawn with the interface's sans font, and bold is faked by drawing the text twice. Sheets on screen use the same approximation. PDF and SVG use the real Times, Courier and bold or italic faces.
- Annotation tools work in plans, sections and elevations, and notes also work in axonometric views and on sheets. They do not work in the live 3D viewport.
- Leaders of notes placed on sheets end at a free point on the paper. They are not attached to the element they point at.
- A note whose leader is attached to an element is deleted with that element, like attached dimensions are.
- Radius, diameter and arc length can only pick drawn arcs (door swings and sketch arcs). Any other curve needs the three-point pick.
- Labels are not moved apart automatically when they overlap (see the PDF screenshot).
- Colours are chosen with the plain egui colour picker, not a named palette.
