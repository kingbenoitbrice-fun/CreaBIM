# Door and window frames, Move by object or face, section headroom

## What each shot shows

- `01-window-frame-3d.png`: a sample window in 3D. It has a frame round the opening (jambs, head and sill member), a glazing bar splitting it into two panes, and glass set back in the middle of the frame depth. The glass is see-through.
- `02-door-frame-leaf-handle-3d.png`: the front door. It has a frame (lining) on three sides with no sill, a leaf flush with the frame face on the side it opens to, and a lever handle with a rose on each face.
- `03-plan-door-and-window.png`: the ground floor plan. The door shows its frame jambs, its leaf drawn open at 90 degrees and its swing arc. The window (top left) shows its cut jambs, the sill member below, the wall faces running across and a glass line.
- `04-section-frames-and-glass.png`: Section A, zoomed in. Windows seen beyond the cut show the frame outline, the glazing bar and the glass instead of a plain rectangle. Where the cut passes through the front wall, the door and window frames show as cut members.
- `05-section-headroom.png`: Section A with a third level at +6000 (the top of the walls, as in the user's project). The top level head now keeps clear space above it, and so does the bottom one below it.
- `06-move-object-mode.png`: the Move tool's panel with the new **Object / Face** switch. In Object mode the whole wall moves, as before.
- `07-move-face-mode-hover.png`: in Face mode, the face under the pointer lights up in blue. Here it is the outside face of GF Wall 1, which is not the selected element.
- `08-move-face-push-pull.png`: after the base click, the face is pushed out by a typed 600. The wall gets 600 mm thicker and its inside face stays put. The readout says "600 mm out" in project units.

## How to try it

- Select a window or door. The Properties panel has a new **Frame** section with Frame width and Frame depth, plus Panes across and Panes up for windows. The sample's windows now have two panes across.
- Press **M** (or the Move button), then click **Face** on the Move panel, or press **F**. Point at a face of a wall, slab or mass and click on it. Move the pointer or type a distance and press Enter. A minus sign pulls the face in. Press F again, or click Object, to move whole elements. The choice is remembered for the next Move.
  - Wall: an end face changes the length (doors and windows keep their place), the top face changes the height, the bottom face changes the base offset, and a side face changes the thickness.
  - Slab: the top or bottom face changes the thickness, and an edge face moves that edge.
  - Mass: the face's vertices move along its normal.
- G with a selected mass face still moves that face freely, as before. The Move tool no longer picks a face implicitly.

## Known gaps

- Face mode works in the 3D view only. In plans and sections, Move with Face chosen moves whole elements, and the status line says so.
- Push and pull covers walls, slabs and masses. Columns, beams, roofs, stairs and doors move as whole objects.
- Arc edges of slabs can't be pushed. Pushing a wall's side doesn't update the walls joined to it beyond what the joins already do.
- The face highlight is drawn over the 3D image, so it shows through anything in front of the face.
- There is no exterior sill nosing yet, no door hand or swing-side choice, and no glazed doors. Doors always swing to the wall's left side, as the plan symbol did before.
- In IFC, the door or window body is a set of extruded boxes (frame, glass, leaf, handle) under one IfcWindow or IfcDoor. The parts have no separate material layers or styles yet.
