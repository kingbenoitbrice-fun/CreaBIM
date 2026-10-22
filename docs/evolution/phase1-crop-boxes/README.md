# Crop boxes: Revit-style section boxes for 3D views

A crop box is a box that cuts a 3D view down to what is inside it, so the inside of a building shows. It works like Revit's section box, with three additions:

- **Crop everything or only chosen elements.** A box crops every element in the view, or only the elements picked for it. The other elements stay whole. Picking a wall takes its doors and windows along.
- **As many boxes as you want.** Each 3D view can have any number of crop boxes. An element that several boxes crop keeps only what is inside all of them.
- **Switches per box.** Each box can be switched off (it keeps its place but crops nothing) or hidden (its frame goes but it still crops; the frame shows again while the box is selected). Each box can be renamed, moved and deleted, and every change undoes with Ctrl+Z.

Crop boxes are saved in the project file like other elements. Files from before this round open unchanged. Crop boxes are documentation, so IFC, glTF and FreeCAD exports leave them out and are never cropped.

**Whose crop boxes the 3D window shows.** The 3D window works with one set of crop boxes at a time. When a saved 3D view (such as *3D View South-East*) is open in the drawing pane, the 3D window shows and edits that view's crop boxes, and the view's drawing follows them. Sheets that show that 3D view follow them too. Otherwise the 3D window uses its own crop boxes. Selecting a crop box in the browser always shows that box's set.

**Cut faces.** Every cut is closed with a cap, so cut walls, slabs and layers look solid. The cap is a darker shade of the element's own colour, and its outline is drawn as a dark cut edge. A layered wall shows each of its layers in the cut.

## How to try it

1. Pick **View › 3D view › Crop boxes › New crop box**. A box is fitted round the whole model and selected. You can also use **Add crop box** in Properties (with nothing selected), or the **+** on the *Crop boxes* group in the project browser.
2. Drag the round handles on its faces to crop. Drag the square handle at its corner to move the box across the plan (G moves it too). Or type exact extents in Properties (*X/Y/Z from … to …*), in project units.
3. To crop only some elements, select them first and pick **New crop box for the selection only**. You can also set *What it crops › Only chosen elements* on any box. Then click **Pick elements…** and click elements in the 3D view to add them; clicking one again leaves it whole. Press Esc, right-click or click **Done picking** when you are finished.
4. **New crop box round the selection** fits a box round the selected elements, and it crops everything in the view.
5. To manage a set of boxes, deselect everything: Properties lists the crop boxes of the current set. Each row has a *Crop* checkbox, a *Show the box* checkbox, the box's name (click it to select the box) and *Delete*. The View menu's *Crop boxes* submenu lists the same boxes with on/off ticks. The project browser has a *Crop boxes* group listing every box; a 3D view's boxes carry the view's name.
6. Open *3D View South-East* from the browser and add a crop box. The drawing pane, and the *3D View* sheet, show it cropped.

## Screenshots

| Screenshot | What it shows |
|---|---|
| `01-menu.png` | View › 3D view › Crop boxes: a new box round the model, round the selection, or for the selection only. |
| `02-new-crop-box.png` | A new *Crop box 1*, fitted round the sample house and selected. It has round handles on its six faces and a square move handle at its corner. Properties show *Crop*, *Show the box*, *What it crops*, the typed extents in mm, and *Fit to the model*. |
| `03-drag-top-face.png` | The top face dragged down. It snapped to the first-floor slab, and the status bar reads "Resize Crop box 1". |
| `04-typed-extent.png` | *Z to* typed as 1800 mm: the ground-floor walls are cut through their windows. |
| `05-solid-cut-faces.png` | A close-up of the cut. Every wall is closed by a darker cap with a dark cut edge, and the windows are cut with their walls. |
| `06-switched-off.png` | The box switched off from the menu. It keeps its place, drawn dashed and grey, and crops nothing. |
| `07-only-chosen-elements.png` | *Crop box 2*, made with *New crop box for the selection only* on the front wall, with *Z to* set to 1000 mm. Only the front wall is cut down. The other walls inside the box stay whole. |
| `08-crop-box-list.png` | With nothing selected, Properties list the 3D window's crop boxes, each with its *Crop* and *Show the box* switches, its name and *Delete*. |
| `09-two-boxes-one-frame-hidden.png` | Both boxes on, with box 2's frame hidden. Everything is cut at 1.8 m, and the front wall at 1 m: an element keeps only what is inside every box that crops it. |
| `10-3d-view-crop.png` | *3D View South-East* open in the drawing pane, so the 3D window shows that view's own crop box. The view's drawing (right) is cropped the same way, with the caps drawn. |
| `11-sheet-respects-crop.png` | Sheet A001, which shows *3D View South-East*: the placed view is cropped by the view's box. The 3D window is back on its own boxes. |
| `12-pick-elements.png` | Picking what *Crop box 2* crops. A banner over Properties counts the chosen elements, the chosen walls are highlighted, and the status bar confirms each click. |
| `13-deleted.png` | *Crop box 2* deleted with the Delete key. The list and the browser update; Ctrl+Z brings it back. |
| `14-browser-group.png` | The project browser's *Crop boxes* group with a **+** to add one. A 3D view's box shows its view's name (*Crop box 1 · 3D View South-East*). |

## Known gaps

- Boxes stay square to the model axes: they can't be turned to follow a building set at an angle.
- Caps are a flat, darker shade of the element's colour. They are not hatched with the material's cut pattern.
- A mesh that isn't closed (an open imported mass, for example) is cut without a cap there. Only outlines that close on themselves are filled.
- A 3D view's drawing stays centred on the whole model when it is cropped, so its place on a sheet never jumps. A tightly cropped view can leave white space round it on the sheet, which the sheet's viewport crop can take away.
- Crop boxes affect 3D only. Plans, sections and elevations ignore them. A selected crop box shows its side handles in a plan pane, and they work there, but the plan doesn't draw the box itself.
- The scene is cropped again on every change, including on each frame of a handle drag. This is quick on the sample houses but may lag on very large models.
- The move handle slides the box across the plan only. To move it up or down, type its Z extents or use G with a typed distance.
