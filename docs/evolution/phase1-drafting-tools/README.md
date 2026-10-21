# Phase 1: drafting tools

Reference screenshots for the drafting work requested in the October markup:

- Temporary guide lines (page 11).
- Dimension direction (page 10).
- The Align tool (page 5).
- Annotation styles and sheet templates (page 18).

All shots are from the sample house, in the one-view layout.

| Screenshot | What it shows |
| --- | --- |
| [01-guides-wall-tool.png](01-guides-wall-tool.png) | The Wall tool after one click at the lower left, with the house's top-right corner rested (pointer held still, or Q). The corner sends out dashed blue guide lines: level, plumb, and the 45° steps. The pointer has snapped where the corner's vertical guide crosses the wall's own horizontal track, labelled *Vertical · on x axis*. The *Guides* menu is in the tool options strip. |
| [02-dimension-auto-horizontal.png](02-dimension-auto-horizontal.png) | The Dimension tool from the bottom-left corner to the top-right corner, with the pointer dragged below them. *Auto* picks **Horizontal**, so the preview measures 10300 across rather than the diagonal. The label says Shift holds the direction. The tool strip has *Auto / Aligned / Horizontal / Vertical* and a *Style* picker. |
| [03-dimension-auto-vertical.png](03-dimension-auto-vertical.png) | The same two points with the pointer to the right of both: Auto turns the dimension **Vertical**. |
| [04-align-pick-edge.png](04-align-pick-edge.png) | The Align tool in *Align* mode. The inner face of the left wall was clicked as the reference and is highlighted in blue. The pointer is on the edge of a skewed wall that is to be aligned. |
| [05-align-done.png](05-align-done.png) | After the click, the skewed wall has turned parallel to the reference and moved flush against it, as one undo step ("Align"). The reference stays picked, so more edges can be aligned to it. Esc lets it go. |
| [06-extend-pick.png](06-extend-pick.png) | *Extend to it* mode. The reference is the top wall's inner face (blue), and the pointer is near the top end of the aligned wall. |
| [07-extend-done.png](07-extend-done.png) | The nearer end has stretched to the reference (Start Y 5066 → 5850 mm), as one undo step ("Extend to reference"). This mode works on walls, beams and grid lines. |
| [08-style-manager.png](08-style-manager.png) | **File › Annotation styles…**, showing the new style *Presentation*. The editor covers name, font, size, colour, bold and italic, then *Dimension ends* (Dot), *Leader end*, *End size*, *Tag shape* (Hexagon) and *Meant for*. Below that are a live preview, *Select all N using it*, *Apply to 10 selected*, and the *New ones get it* switches for dimensions, notes and tags. |
| [09-style-applied-dots-hexagons.png](09-style-applied-dots-hexagons.png) | After *Apply to 10 selected*, the plan's dimensions end in dots and its door and window tags are hexagons. |
| [10-sheet-templates-dxf-import.png](10-sheet-templates-dxf-import.png) | **File › Sheet templates…** after importing a DXF title block (typed path, then *Import*). Its `{client}`, `{drawn_by}` and `{revision}` text became fields of its own, here with default values. The preview shows it in the bottom-right corner, inside the usual border. *Use on every sheet* has been pressed: "4 sheet(s) use the template". |
| [11-sheet-with-template.png](11-sheet-with-template.png) | Sheet A101 drawing the imported block. Its Properties have a *Title block* picker (Built-in or a template) and a field for each of the template's own fields. Each field shows the template default until the sheet is given its own value. |
| [12-title-block-fields-filled.png](12-title-block-fields-filled.png) | A close-up of the block. Every field is filled in: client, "Drawn by B. King · Rev B", the sheet title and number, and *SCALE 1:100* worked out from the views on the sheet. |
| [13-guides-in-section-dimension.png](13-guides-in-section-dimension.png) | Guides in a section. The Dimension tool has the window corner rested and its first point on the wall top, and the second point rides the window corner's vertical guide. The *Presentation* style picked in the strip already gives the preview its outline arrows. |
| [14-align-in-context-menu.png](14-align-in-context-menu.png) | *Align to a reference…* in the right-click menu of a 2D view. It is also in the Edit menu and in the tool palette, at the far right. |

## How to try it

### Guide lines (any drawing tool, and the Dimension tool, in plans and sections)

1. Hold the pointer still on a point or an edge for about half a second, or press **Q** over it. A small blue marker shows that it is rested. At most eight points and edges are rested at a time.
2. Each rested item gives guide lines:
   - a point gives level and plumb lines, and lines at every angle step (45° by default)
   - an edge gives its extension, and perpendiculars at both ends
   - once the tool has a first point: parallels and perpendiculars through that point, and the polar angles from it
   - an arc or circle gives its circle, radials, and tangents from the first point
3. The pointer snaps to a guide line, and with more priority to where two guides cross, or where a guide crosses an edge or a track line. The label near the pointer names the guides, e.g. *Extension · Perpendicular*.
4. Rest on an item again to drop it.
5. **Esc** with no clicks pending clears every guide line, and a second Esc leaves the tool.
6. **K** keeps the straight guide under the pointer as a real Guide element. Keeping it can be undone.
7. The **Guides** menu in the tool strip turns guide lines on and off, picks the angle step (level and plumb only, 15°, 30° or 45°), and clears them. The switch is also the last line in the snap list.

### Dimension direction

- In the Dimension tool, *Auto* picks from where the line is dragged:
  - beside both points: Vertical
  - above or below them: Horizontal
  - otherwise: Aligned
- Hold **Shift** to keep the direction shown while you place the line.
- *Aligned*, *Horizontal* and *Vertical* in the strip force one direction.
- A placed linear dimension's Properties have *Measures* to change it later.
- Horizontal and vertical dimensions measure the projected distance, with witness lines back to the measured points. The direction is saved with the dimension (old files read as Aligned).

### Align and Extend

1. Pick the Align tool: the last icon in the palette, *Edit › Align to a reference…*, or the right-click menu.
2. Click a reference line: a wall face, a slab edge, a grid line, a guide element or a temporary guide.
3. Click the edge to align:
   - *Align* turns the element parallel (the shorter way round) and moves it flush.
   - *Extend to it* stretches the nearer end of a wall, beam or grid line to meet the reference.
   - Clicking an opening's edge moves its host wall.
4. Every click is one undo step, and the reference stays picked for further edges.

### Annotation styles

1. Open the manager with **File › Annotation styles…**, *Annotation styles…* in a style's Properties, or the bottom of any *Style* picker in the tool strip.
2. Use *New* or *Duplicate* to make a style, then set its text, its dimension ends and leader end (11 kinds), the end size, the tag shape (10 shapes) and what it is meant for.
3. A change shows at once on every dimension, note and tag that uses the style.
4. *Select all N using it* selects those users, and *Apply to N selected* gives the style to the selection.
5. Deleting a style returns its users to the default look, in the same undo step.
6. The Dimension, Note and Tag tools each have a *Style* picker in the strip for new annotations. The picker lists the styles meant for that kind, plus "any annotation" styles.

### Sheet templates

1. Open **File › Sheet templates…**.
2. Start a template in one of these ways:
   - *Copy of the built-in block*, to change the usual title block.
   - *Import DXF or SVG…*, or type a path and press *Import*.
   - Type the path of another `.creabim` and import it, which brings in its sheet templates and annotation styles.
3. Any `{name}` in the text is a field. `{sheet}`, `{number}`, `{project}`, `{scale}` and `{date}` are filled in by every sheet. Other fields belong to the template and get a default value in the window.
4. A drawing at least 250 mm wide is taken as a whole sheet frame: no extra border, and the paper size snapped to the nearest ISO size. A smaller one is a title block for the bottom-right corner.
5. Use *Use on this sheet*, *Use on every sheet* or *New sheet from it*, or pick the template from a sheet's *Title block* in Properties and fill in its fields there.
6. Templates are saved in the project. PDF and SVG export draw them.

## Known gaps

- **Title block import:** PDF and raster title blocks are not imported; use DXF or SVG. Templates are named and reusable within a project, and can be copied between projects by importing a `.creabim`.
- **DXF import:** blocks (INSERT), hatches, dimensions and splines are skipped.
- **SVG import:** curves come in as straight chords between their end points, and only plain text and `tspan` are read.
- **Template editing:** the window edits text and size, fields, corner and border. Lines cannot be moved by dragging; edit the drawing and import it again.
- **Where guides work:** temporary guides work in plan and section or elevation panes. They do not work in the 3D view, on sheets, or while dragging grips with the Select tool.
- **Keeping guides:** circle guides can't be kept with K; only straight ones become Guide elements.
- **Align in sections:** it only moves (along the reference's normal), for edges already parallel to the reference. Turning and Extend work in plans.
- **Align targets:** it moves what is picked as a whole. It does not move a single slab edge or a single sketch vertex.
- **Style coverage:** the style's dimension ends apply to linear dimensions and radius and diameter arrows. The angle and arc-length ends follow them too. Level and grid marks keep their own symbols.
