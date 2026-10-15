# Layered assemblies and walls that stay joined

Walls, slabs and roofs can now be built of an **assembly**: a named, reusable list
of layers (material, thickness, function) kept in the project, as wall, floor and
roof types work in Revit and ArchiCAD. Plans and sections hatch each layer with its
material, the 3D view draws one solid per layer, and IFC carries the assemblies as
`IfcMaterialLayerSet` with `IfcMaterialLayerSetUsage`.

The second part fixes how walls join. Before, moving a wall a little left a notch at
its corners (the user's screenshot), because the walls next to it did not move with
it and a corner only joined when two ends met exactly. Now the joined walls follow
and stretch, and a corner still mitres when two ends are a little apart.

| Screenshot | What it shows |
|---|---|
| `01-assembly-editor.png` | The Assemblies window: the assemblies grouped by walls, floors and roofs on the left. The layer table runs from the exterior face to the interior face, with function, material, thickness, reordering and removal for each layer, and a hatched preview with the core bracketed in orange. The window also shows how many elements use the assembly and can select them or assign it to the selection. |
| `02-plan-layered-wall-properties.png` | Ground floor plan of the layered house, zoomed to a corner. Brick, cavity, insulation, block and plaster are each hatched, and the corner is mitred layer by layer. The selected wall's Properties show the Assembly picker, a strip of its layers, *Edit layers…*, the thickness taken from the layers and *Exterior on the left*. |
| `03-section-wall-slab-roof.png` | Section A at the eave. The wall's layers, the floor (tile, screed, EPS, concrete) running to the inner face of the wall, and the roof's tile, battens/air gap, membrane, wood fibre, rafters and plasterboard, each with its own hatch. |
| `04-3d-wall-layers.png` | 3D view with the roof hidden. The tops of the walls show the separate layer solids (brick red, cavity left open, yellow insulation, grey block, white plaster), turning the corners and lining the window reveals. |
| `05-3d-roof-assembly.png` | The roof selected. Its Properties show the "Pitched roof 288, tiled" assembly and its layers; the thickness comes from them. |
| `06-move-joined-wall-before.png` | Before: the east ground-floor wall selected. |
| `07-move-joined-wall-after.png` | After dragging it 1.35 m east: the north and south walls stretch to follow it, the corners stay mitred and the dimensions update. The move is one undo step. |
| `08-moved-corner-detail.png` | A close-up of the moved north-east corner: the layers stay clean and mitred, with no notch. |
| `09-wall-schedule-assemblies.png` | The wall schedule gains an Assembly column when any wall has one. Layered walls are listed by their core material and their total thickness. |

## How to try it

1. Write the sample with `creabim sample-layered house.json` and open it with
   *File › Open…*. Or open any project and choose *Materials › Add starter
   assemblies* (an exterior brick cavity wall, an interior block wall, a concrete
   floor and a tiled pitched roof).
2. Open *Materials › Assemblies…*, or the **…** button on the *Assemblies* group in
   the project browser, to create, duplicate, edit or delete assemblies. Edits
   resize every element using the assembly.
3. Select walls, slabs or roofs and pick an assembly with *Materials › Assign
   assembly*, or with the Assembly picker in Properties. "None" goes back to a
   single material.
4. A wall's layers run from its exterior face, which is on the wall's right as drawn
   (the outside of a wall drawn anticlockwise). Tick *Exterior on the left* to flip a
   wall. Mirroring a wall flips it too.
5. Drag or move (M) a wall that is joined to others. Its corners follow; walls
   meeting it in a T slide along it and stretch to stay on it; a wall moved as a
   whole keeps its T ends on the walls it sits on.
6. *File › Export › IFC4* writes an `IfcMaterialLayerSet` for every assembly. Walls
   and slabs get an `IfcMaterialLayerSetUsage` (AXIS2 for walls, AXIS3 for slabs);
   roofs get the layer set itself. Opening the file again brings the assemblies
   back. IFCX (*IFC5 alpha*) round-trips them as well.

## Data

- `ElementKind::Assembly` is a new element holding a usage (wall, floor or roof),
  the layers and a description. Its IFC class is `IfcMaterialLayerSet`, so it shows
  in the project browser under *Assemblies*.
- `Wall`, `Slab` and `Roof` gain `assembly`, and `Wall` gains `flip`. Both are left
  out of the file when unset, so older single-material projects open unchanged (the
  test `single_material_elements_from_older_files_still_read` covers this). An
  element's `thickness` is kept equal to its assembly's total, so code that only
  knows thickness still works.
- The **core** runs from the first to the last Structure layer, or covers every
  layer when there is no Structure layer. Where two layered walls meet in a T, the
  core runs on to the other wall's core and cuts through its finish layers. An L
  corner is mitred layer by layer.
- Moved walls are followed by `creabim_core::joins::follow`, run in every live drag
  or move. Two ends make a corner when they lie within a wall's thickness of where
  the centrelines cross, so a nudge no longer opens a gap.

## Known gaps

- There is no location line: walls are placed on their centreline. Use flip to
  choose which face is the exterior.
- Typing new start or end coordinates in Properties moves only that wall. Joined
  walls follow drags and moves, not typed values.
- Walls on another level are not joined. Moving a ground-floor wall leaves the
  first-floor wall above it where it was.
- A new wall drawn with the Wall tool starts with a single material. The tool
  options have no assembly picker yet.
- In a section the cavity (air gap) is left open at the ends of a wall.
- In the IFC export each element is still a single body. Only the material layer
  set describes the layers, not one solid per layer.
- Roofs come back from IFC as masses, as before, though their assembly is imported.
- The schedule's material and thickness cells stay editable for layered walls. The
  thickness snaps back to the assembly's total, and changing the material there
  does not change the layers.
- The title of the Assemblies window can sit under the 3D view's navigation buttons
  when the two overlap, because the buttons are a web pane above the native window.
