# CreaBIM Link for Blender

CreaBIM opens `.blend` files directly and reloads them each time they are
saved, so Blender needs no add-on to send work to CreaBIM. This add-on
covers the other direction and a few conveniences:

- **Load CreaBIM model**: brings in the `.glb` that CreaBIM writes with
  *File > Export glTF (.glb) for Blender*. Each object keeps its CreaBIM id,
  so loading again updates the same objects instead of adding copies, and
  edits saved here update the same walls, slabs, doors and windows in CreaBIM.
- **Follow changes**: reloads the model whenever CreaBIM exports it again.
- **Mark as Wall / Slab / Door / Window / Column…**: sets the
  `creabim_kind` custom property. CreaBIM turns a marked object into that
  element, sized to fit it. Putting objects in a collection named *Walls*,
  *Slabs*, *Doors* or *Windows* (or after an IFC class such as *IfcColumn*)
  does the same without the add-on.
- **Save for CreaBIM**: saves the `.blend`.

## Install

Blender 4.2 or newer: *Edit > Preferences > Get Extensions > Install from
Disk* and pick a zip of the `creabim_link` folder (`zip -r creabim_link.zip
creabim_link`). The panel is in the 3D Viewport's sidebar (N), tab
*CreaBIM*.

## Round trip

1. CreaBIM: *File > Export glTF (.glb) for Blender*.
2. Blender: CreaBIM tab, pick the `.glb`, *Load CreaBIM model*.
3. Edit, add and mark objects, then save the `.blend`.
4. CreaBIM: type the `.blend` path in the File box, *File > Import and
   link…*. Later saves are picked up automatically.

`test_roundtrip.py` runs steps 2 and 3 headless (with Blender's Python or
the `bpy` wheel), stretching a wall by a metre and adding a slab; then
`creabim link house.creabim edited.blend out.creabim` shows `1 added, 1
updated`, and the wall schedule shows the wall a metre longer.
