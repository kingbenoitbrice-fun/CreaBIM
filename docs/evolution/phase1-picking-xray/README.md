# Phase 1: hover pre-highlight, overlap picking and X-ray

Reference screenshots, taken from *File > New sample with a beam in a wall*: the sample house with a timber lintel beam on the ground floor that sits inside the front wall (2.3–2.6 m up), so nothing in the drawings shows it until you look for it.

**Hover pre-highlight.** In plans, sections, elevations, the 3D window and sheets, the element under the pointer is drawn in a quiet blue before it is clicked, so you can see what a click will take. The orange selection colour is unchanged. The highlight shows in every open view, not only the one under the pointer.

**Overlap awareness.** When more than one element is under the pointer (a beam inside a wall, a column behind a slab edge), a small badge by the pointer counts them ("1/3 · Tab"). **Tab** and **Shift+Tab** cycle which one is highlighted, the status bar names it, and a click selects it. Clicking the badge opens a short list of the elements to pick from directly. A plain click with no Tab still takes what it took before. Moving the pointer over the same stack keeps the Tab choice. In elevations this also reaches what is behind the face you see: in the South Elevation, pointing at the south wall where the first floor slab sits behind it, the badge counts the slab as well and Tab takes it. In the 3D window and in 3D views, what a crop box cuts away is neither highlighted nor picked, and a crop box frame under the pointer is counted with the rest.

**2D X-ray (Alt+Z).** Alt+Z puts the view under the pointer (or the active one) into X-ray. Walls, slabs and roofs fade to faint fills, the elements they hide are drawn with solid lines, and those elements come first under the pointer. You can then select and drag them like anything else. X-ray is a working mode for this session: it is not saved, not undoable and never shows on sheets. In the 3D window, Alt+Z uses the existing 3D X-ray, and clicks now take framing (beams, columns, members) before the walls around it.

**Hidden elements in their colours (detailing).** A new per-view option, *In colour*, draws hidden elements with a semi-transparent fill in their material colour under their dashed hidden lines. You can read the timber lintel inside the wall as timber, and clicking inside the tint picks it. This is a view property: it is saved, goes through undo, prints on sheets and exports to SVG and PDF. Turning it on also turns on *Show hidden*. In 3D, *Hidden elements in colour* (in *View > 3D view shows*) draws buried elements in their real colours, half transparent, through whatever covers them.

| Screenshot | What it shows |
| --- | --- |
| [01-hover-wall.png](01-hover-wall.png) | Section A, pointer on the front wall over the door: the wall is pre-highlighted in blue, and the badge says three elements are under the pointer. |
| [02-tab-beam.png](02-tab-beam.png) | After Tab: the buried lintel beam is highlighted instead, and the status bar reads "Beam · Lintel beam (2 of 3 under the pointer)". |
| [03-beam-selected.png](03-beam-selected.png) | A click then selects the beam: orange in the drawing, with its properties in the panel. |
| [04-overlap-list.png](04-overlap-list.png) | The badge clicked: its list names each element under the pointer. Hovering a row highlights that element, and clicking a row selects it. |
| [05-xray-2d.png](05-xray-2d.png) | Alt+Z on the section: the walls and slabs fade, the beam is drawn solid inside the wall, and it is first under the pointer. |
| [06-xray-in-colour.png](06-xray-in-colour.png) | X-ray together with *In colour*: the beam shows in its timber colour. |
| [07-colour-detailing.png](07-colour-detailing.png) | *In colour* without X-ray, as it prints: the wall is drawn normally and the lintel shows through as a timber tint under its dashed hidden lines. |
| [08-colour-hover-picks-beam.png](08-colour-hover-picks-beam.png) | The pointer inside the tint picks the beam directly, with no Tab needed. |
| [09-3d-hover-wall.png](09-3d-hover-wall.png) | The 3D window with X-ray off: the hovered wall's edges are traced in blue, with the overlap badge. |
| [10-3d-xray-beam-first.png](10-3d-xray-beam-first.png) | 3D X-ray: the beam inside the wall is first under the pointer and highlighted. |
| [11-3d-hidden-in-colour.png](11-3d-hidden-in-colour.png) | *Hidden elements in colour* in 3D, from the side: the timber band of the lintel shows through the wall. |
| [12-plan-hover-slab.png](12-plan-hover-slab.png) | A ground floor plan: the slab is pre-highlighted when the pointer is over open floor. |
| [13-sheet-hover-view.png](13-sheet-hover-view.png) | A sheet: hovering a viewport outlines it before it is clicked. |

## How to try it

- *File > New sample with a beam in a wall*, then open Section A and zoom on the front wall above the door.
- Move the pointer over the wall. Press **Tab** until the badge reads 2/3 and the status bar names the lintel beam, then click. Or click the badge and pick the beam from its list.
- **Alt+Z** with the pointer over a plan, section or elevation toggles X-ray for that view. The toolbar's *X-ray* toggle does the same for the active view.
- The toolbar's *In colour* toggle (or the view's *Visibility* menu) turns on hidden elements in their colours for the active view. *View > 3D view shows > Hidden elements in colour* does the same for the 3D window.

## Data

- `View` gains `hidden_in_colour: bool`. It defaults to off, and is left out of files when off, so older projects open unchanged and save as before.
- The app settings gain `hidden_colour_3d`.
- Drawings gain a `Tint` item (a filled outline in a material colour). SVG writes it with `fill-opacity`, and PDF and other canvases use the colour lightened toward white.

## Known gaps

- 2D X-ray is per session. It is not saved with the view, and sheets never show it.
- The coloured fill of a hidden element is the convex hull of its projected shape, so it is accurate for beams, columns and members but rough for L-shaped or hollow elements.
- In 3D, "buried" (for *Hidden elements in colour*) is decided by bounding box: elements whose centre lies inside the bounds of a wall, slab or roof.
- Grid lines get no blue wash when hovered in 3D. Their bubbles still pick as before.
- Elevations use the same code as sections. The slab-behind-a-wall case is covered by a test, but no elevation is shown in the screenshots.
- In plans, the downward pick ray only finds elements the plan draws.
- Tab is taken back from egui focus over a drawing. Keyboard focus left in a web panel after clicking it is not covered.
