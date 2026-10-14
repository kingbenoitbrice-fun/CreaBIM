# Materials on doors, windows, columns, beams, roofs, stairs and railings

Follow-up to the material library (`phase1-materials`). Before this, only walls,
slabs and masses could take a material: the material browser's counts, its
"Assign to the selected element" button, the *Materials › Assign material* menu and
deleting a material in use all skipped every other element. Doors and windows had
no material at all.

| Screenshot | What it shows |
|---|---|
| `01-door-material.png` | The Front Door selected: its properties now have a Material picker. |
| `02-assign-to-column.png` | A column placed with the Column tool; *Materials › Assign material* now offers the project's materials for it. |
| `03-browser-counts.png` | The material browser counts the door (Timber, 4) and the column (Concrete, 3) among each material's users. |

## How to try it

1. Select a door or window and pick a material in its properties, or use
   *Materials › Assign material*.
2. Place a column, beam, roof, stair or railing and do the same.
3. Open *Materials › Material browser…*: the counts, "Select them" and deleting a
   material with a replacement include these elements.
4. Export IFC4 or IFCX and open it again: doors and windows keep their material.

## Data

`Opening` gains `material`, left out of the file when empty, so older projects open
unchanged. The five building elements already stored a material; the library's
`material_slot` and `set_material` now cover them and openings, and
`material_of` goes through `material_slot`, so every exporter and drawing sees the
same answer.

## Known gaps

- A door or window draws in a single colour: frame and glass can't take different
  materials yet.
- The 3D view's navigation buttons draw on top of the material browser where the
  two overlap (the buttons are a web pane above the native window).
