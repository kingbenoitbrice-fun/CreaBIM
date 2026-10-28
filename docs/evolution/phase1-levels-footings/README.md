# Footings, copying between levels, and columns over several levels

This work adds three things:

- **Foundation footings.** A strip footing runs under a wall, and a pad footing sits under a column or stands on its own. A footing placed under a wall or column follows it.
- **Copy to levels and Move to level.** Something drawn on one floor can be put on other floors without drawing it again.
- **Column base and top constraints.** A column can stand on one level and run up to another. It stretches when either level moves.

| Screenshot | What it shows |
|---|---|
| `01-pad-footing-under-porch-column.png` | The typed sample house's ground floor plan. The strip footings under the walls and the pad under the porch column are drawn dashed, because they are below the cut. The pad is selected. Properties shows its type ("Pad footing 1200 × 1200 × 400"), its width, length and thickness, *Top below column base* 500 mm, the host ("Under Porch Column: it follows it when it moves…"), *Let go of it*, and its top and underside elevations. In 3D, the pad is highlighted under the column. |
| `02-strip-footing-in-section.png` | Section A with the strip under GF Wall 1 selected: it is cut below the wall, with its handles. Properties gives *Top below wall base* 300 mm and "Strip 10000 mm long: top at -300 mm, underside at -600 mm". The pad under the porch column is shown in view, to the left. |
| `03-footings-from-below-in-3d.png` | The 3D view orbited below the ground. The strips run under the walls and meet at the corners, the selected strip is orange, and the pad is under the column. The storefront curtain wall has no strip. |
| `04-footing-tool-drawing-a-strip.png` | The Footing tool. Its option strip has *Type*, *Strip* / *Pad*, *Width* and *Thickness*. A free strip is being drawn with two clicks, with a length readout and snaps. The hint says: click a wall for a strip under it, or a column for a pad under it. |
| `05-edit-menu-level-commands.png` | The Edit menu with the first floor selected. It has *Paste aligned to level ›* (one item per level), *Footings under selection*, *Copy to levels…* and *Move to level…*. The same items are on the right-click menu. |
| `06-copy-to-levels-window.png` | *Copy to levels*: what is copied and how, every level with its elevation, the current level greyed out, *All above* / *All below* / *None*, and *Copy to 3 levels*. |
| `07-first-floor-copied-to-three-levels.png` | The first floor (walls, windows, slab and the stair shaft) copied to Levels 2, 3 and 4 in one step: "Copied to Level 2, Level 3, Level 4: 36 elements … Ctrl+Z undoes it in one step." The copied shaft's top was moved up the same number of storeys (Level 2 to Level 3). |
| `08-column-from-level-2-up-to-level-4.png` | A column drawn on Level 4, then given *Base level* Level 2 and *Top level* Level 4 in Properties: "Base at +6000 mm, top at +12000 mm". Properties shows *Base level*, *Base offset*, *Top level* and *Top offset*. |
| `09-column-cut-in-the-level-it-passes-through.png` | The Level 3 plan. The column passes through this level, so it is drawn cut (filled) here too. |
| `10-column-grips-in-elevation.png` | The South elevation with the column selected. It has a handle at its base on Level 2 and one at its top on Level 4. |
| `11-column-stretches-when-level-4-moves.png` | Level 4 dragged up to +13300 in the elevation. The column's top follows it ("top at +13300 mm"), and so does everything on Level 4. |

## How to try it

1. *File › New typed sample house*. The ground floor walls already have strip footings and the porch column has a pad. Pick one in a plan (dashed), a section or 3D, and change its size or depth in Properties or with its handles. Move a wall and its strip follows it.
2. *Create › Footing* (or the Footing button). Pick *Strip* or *Pad* in the option strip, then click a wall or column to put a footing under it, or click in empty space to place one on its own. Footing types are under *Materials › Types… › Footings*.
3. Select walls and columns, then use *Footings under selection* (right-click or Edit) to give each one a footing.
4. Add a few levels (*Create › Level above the top one*). Box-select a floor in its plan, then use *Edit › Copy to levels…*, tick levels and press *Copy*. `Ctrl+Z` undoes it all. *Move to level…* moves the selection instead. *Edit › Paste aligned to level* pastes what `Ctrl+C` copied onto a chosen level.
5. Select a column and set its *Base level* and *Top level* in Properties. Changing the base level keeps the top where it is. Drag a level line in a section or elevation and the column stretches with it.
6. *File › IFC4* writes footings as `IfcFooting` (`STRIP_FOOTING` / `PAD_FOOTING`) with `IfcFootingType`. Opening the file again brings them back as footings, still under their walls and columns.

The Help pages are *Footings* (new), *Levels › Copying and moving to other levels* and *Columns and beams › Over several levels*.

## Known gaps

- A strip is always centred under its wall. There is no eccentric offset yet.
- Curtain walls get no strip footing.
- Tags, grids and dimensions in the selection are not copied to other levels. Their views stay on the original level.
- A copied wall keeps its top constraint, moved by storeys. A wall whose top was *Unconnected* keeps its fixed height, so where storey heights differ the copies may not meet the slab above. In shot 11, Level 3's walls stay 3 m tall after Level 4 moves up.
- A hosted footing ignores plan moves of its own (it follows its host). To move it alone, press *Let go of it* first.
- Columns come back from IFC4 as meshes, without their types. A pad under one keeps its stored position, and the sample's porch column is left untyped for that reason.
- Number fields in the option strip of the Footing tool are in metres, like those of the other member tools.
