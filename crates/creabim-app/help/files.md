# Opening and saving

## The project file

A CreaBIM project is a `.creabim` file. It holds the model, its views, sheets, styles and templates, and the **undo history**: you can still undo after closing and reopening the project.

- `Ctrl+S` or *File › Save* saves. *Save as…* saves under another name.
- `Ctrl+O` or *File › Open…* opens a `.creabim`, `.ifc` or `.ifcx` file.
- *File › Open recent* lists the last ten projects.
- Dropping a file on the window opens it.

Exports are written next to the project file, with its name.

## Linked files

*File › Import and link…* brings in the objects of a Blender (`.blend`) or FreeCAD (`.FCStd`) file. They stay linked: with *Reload linked files when saved* ticked, saving the file in Blender or FreeCAD updates them here within a second. *Reload a linked file* does it by hand. See [Blender](blender) and [FreeCAD](freecad).

## Exchange formats

| Format | Read | Write | Page |
| --- | --- | --- | --- |
| IFC2X3, IFC4, IFC4X3 (`.ifc`) | yes | IFC4 | [IFC](ifc) |
| IFC5 alpha (`.ifcx`) | yes | yes | [IFC](ifc) |
| Blender (`.blend`) | linked | via glTF | [Blender](blender) |
| FreeCAD (`.FCStd`) | linked | yes | [FreeCAD](freecad) |
| glTF (`.glb`, `.gltf`) | materials | yes | [glTF](gltf) |
| PDF | no | sheets | [PDF, SVG and DXF](dxf-svg) |
| SVG | title blocks | views and sheets | [PDF, SVG and DXF](dxf-svg) |
| DXF | title blocks | no | [PDF, SVG and DXF](dxf-svg) |
| CSV | no | schedules | [Schedules](schedules) |
