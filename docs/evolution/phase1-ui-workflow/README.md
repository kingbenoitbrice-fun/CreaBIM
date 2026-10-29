# UI workflow: option strip, quick help, Properties, footings, viewport scale, types

Branch `claude/ui-workflow`. Everything here is in the interface layer: the web bands (`ui/app.js`, `ui/app.css`), `shell.rs`, the egui Properties panel, `sheetview.rs` and the help pages.

## What changed

### The tool options strip

- View switches (*Hidden*, *In colour*, *X-ray*, *Tag openings*, *Tag fixtures*, *Underlay*, *Visibility*, *Style*, *Guides*, *Strip/Pad*…) are now small icon buttons.
- Hover a button for its name, its key and one line on what it does.
- Fields keep a short label: *Width*, *Thickness*, *Cut height*.
- A divider separates the tool's options (left) from the open view's settings (right).
- **Nothing is pushed off-screen.** The page measures every control and Rust decides what fits (`optionbar::overflow`). What doesn't fit goes into **⋯ More**, view settings first and from the right, then tool options. More lists them under *Footing tool* and *This view*, with full labels.
- Lengths in the strip are shown in the project's units, as in Properties. Before, the strip showed `0.60 m` while Properties showed `600 mm`.

### Quick help

- The **?** beside the tool name opens a small popover: the tool's name, its key, two to four lines from its guide page and **Read more in the guide**.
- Properties headings have the same **?**: the element's class (*IfcWall*, *IfcFooting*, *Viewport*…), *View: …* and *3D view*.
- The lines come from the guide itself (`help::quick`): the page's first paragraph cut to two sentences, or its first steps. A help page change updates them, and a test checks that every tool has some.

### Properties: reflows, folds, keeps its width

- **Order:** what it is (class, name, type), then **Geometry and build** (or **Settings** for views, sheets, viewports and annotation), then how views show it (*Visibility*, crop boxes).
- **Folding:** click any section heading to fold it. Folds are remembered between sessions (`settings.json` → `props_open`). Only sections left differently from their default are written.
- **Width:** drag the left edge. The width is remembered, and the panel now keeps it. Before, a wide row (the two-column categories grid, a long drop-down) pushed the panel wider whenever the selection changed. That was the jump in the markup.
- **Reflow:**
  - Text wraps.
  - The visibility categories drop from two columns to one below about 330 px.
  - A labelled drop-down puts its label above it once the panel is narrower than 250 px, instead of squeezing it into single letters.
  - Fixed-width drop-downs shrink to fit.
  - Anything still wider is clipped at the panel edge rather than widening it.

### Footings

- **Keep clicking** walls or columns: the tool stays on.
- **Drag a box** with the Footing tool, as in Select. Every wall it touches gets a strip (with *Strip* picked), or every column a pad (with *Pad*). All of them go in one undo step.
- **Under N selected** in the strip puts footings under the selected walls and columns. It reuses `footings_under`, the same code as the right-click *Footings under selection*. Anything that already has a footing is skipped, and the status says so.

### Viewport scale on sheets

- Select a viewport: *Properties › Scale* has 1:1, 1:2, 1:5, 1:10, 1:20, 1:25, 1:50, 1:100, 1:200 and 1:500, plus a field for any other scale.
- Right-click a viewport: **Scale 1:N ›** with the same list, and **Open the view itself**.
- A new scale grows or shrinks the viewport about its middle (`sheetview::rescaled_center`). The scale is the view's own, so every sheet that places that view follows, in one undo step.
- The view's own Properties use the same picker.

### Types

- **Double-click a type** in the browser, or **Open preview** in its Properties, to see it on its own:
  - a 3D sample (drag it to turn it);
  - its plan and its section;
  - its values at the left, live.
  - Samples exist for every kind: wall, curtain wall, door, window, slab, roof, column, beam, strip and pad footing, and objects.
- **Show in model** in a type's Properties lights every instance in the 3D view and the plans, and zooms both to them. **Fade the rest** dims everything else.
- The selection doesn't change. Untick it, or select something else, to go back.

### Smaller items

- The command palette (`Ctrl+K`) finds elements by name, and selects and frames the one you pick.
- Double-click an element in the browser to frame it.
- The browser has a button that folds every group shut, or opens them all.
- Website links in the guide open in the browser.
- Guide pictures enlarge on click.
- The element count leaves out the units element.
- The 3D view's buttons step aside while an egui window (Types, a type preview, Assemblies, materials) lies over them, instead of drawing on top of it.

## Before and after

At 1280 × 800 with the Footing tool, *X-ray* was cut off and *Tag openings* was out of sight:

| Before | After |
| --- | --- |
| ![](01a-before-1280-footing.png) | ![](01b-after-1280-footing.png) |
| ![](02a-before-1920-footing.png) | ![](02b-after-1920-footing.png) |
| ![](03a-before-1280-select.png) | ![](03b-after-1280-select.png) |
| ![](04a-before-1920-select.png) | ![](04b-after-1920-select.png) |

At 1280 the old Properties panel was about 410 px wide, because the two-column categories grid forced it. Now it is 290 px by default and stays at the width you give it.

Properties, as marked up, against the Tropical Villa at a narrow and a wide panel:

| Before (markup) | After, narrow (220 px) | After, wide |
| --- | --- | --- |
| ![](19a-before-properties-markup.png) | ![](19b-villa-properties-narrow.png) | ![](20-villa-properties-wide.png) |

- ![A section folded](21-villa-section-folded.png)
- ![Visibility in two columns when wide](22-villa-view-two-columns.png)
- ![Visibility in one column when narrow](23-villa-view-one-column.png)
- ![The villa at 1920](24-villa-1920.png)

The strip and its help:

- ![A 1024-wide window: view settings in More](05-narrow-window-more.png)
- ![More open](06-more-menu-open.png)
- ![An icon's tooltip](07-icon-tooltip.png)
- ![The tool's quick help](08-tool-quick-help.png)
- ![Quick help on a Properties heading](09-properties-quick-help.png)

Footings, viewport scale, types:

- ![A box over the simple house: five strips in one step](10-footing-box-five-strips.png)
- ![A viewport's scale in Properties](11-viewport-scale-in-properties.png)
- ![The scale list](12-viewport-scale-dropdown.png)
- ![At 1:50, grown about its middle](13-viewport-at-1-50.png)
- ![Scale from the right-click menu](14-viewport-scale-context-menu.png)
- ![A type's preview](15-type-preview.png)
- ![Turned](16-type-preview-turned.png)
- ![Show in model](17-show-in-model.png)
- ![Fade the rest](18-show-in-model-fade-the-rest.png)

## How to try it

1. *File › New starter house*, then pick the **Footing** tool. Narrow the window and watch the view settings go into **⋯ More**. Click **?** next to *Footing*.
2. *File › New simple sample house*, then use the Footing tool and drag a box over the house. You get five strips; `Ctrl+Z` takes them all back.
3. Open a sheet, select a viewport, pick a scale in *Properties*, or right-click it and use **Scale**.
4. *File › New typed sample house*. Double-click *Exterior wall 365, brick cavity* in the browser's *Types* group, then tick **Show in model** and **Fade the rest** in its Properties.
5. Open `TropicalVilla_REV00.creabim`, select a wall and drag the Properties edge from narrow to wide. Fold *Geometry and build*, then restart the app: it stays folded, at the same width.

## Tests added

- `optionbar`:
  - everything fits;
  - view items go first;
  - then tool items from the right;
  - a trailing divider goes too;
  - controls are marked only when the measure matches.
- `help`:
  - `every_tool_has_a_few_lines_of_quick_help`;
  - `quick_help_is_the_first_paragraph_or_the_first_steps`.
- `levelcopy`: `footings_go_under_the_selected_walls_and_columns` (one undo step, skips walls that have one, no curtain walls).
- `sheetview`: `a_new_scale_grows_the_view_about_its_middle`.
- `typepreview`:
  - `every_kind_of_type_draws_a_sample_on_its_own`;
  - `the_sample_is_the_type_and_only_it`.
- `files`:
  - `folded_properties_sections_round_trip_and_none_writes_nothing`;
  - the old-settings test checks `props_open` defaults empty.

## Known gaps

- At window widths under about 1100 px, the menu row and the tool row are wider than the window and are cut at the right. Only the options strip has an overflow menu.
- The quick-help popover in Properties is an egui popup, so it is styled like egui rather than like the web popover.
- Through several scale changes, a viewport's centre can drift by a millimetre or so. Its middle is taken from the drawing's extent, which includes symbols and text drawn at a fixed paper size.
- The type preview's plan shows the sample section's marks, labelled *Preview*.
- The 3D preview is a shaded axonometric drawing that turns as you drag. It is not the live GPU view.
- In a type's Properties, the class line repeats inside *Settings*.
- *Assign as…* and *Booleans* are not foldable sections yet.
- Box selection while in the tool exists only for footings. Other tools still treat a drag as drawing.
- The shortcuts page still has no table per tool. Tools have no single-key shortcuts; the keys used while drawing are under *While drawing*.
