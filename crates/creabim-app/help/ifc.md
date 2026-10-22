# IFC

**IFC** (Industry Foundation Classes) is the open file format that BIM programs use to share models. An IFC file says not just what shapes there are, but what they are: this is a wall on the ground floor, made of brick, with these doors in it. CreaBIM uses IFC ids for its own elements, so a model that goes out and comes back keeps its identity.

## Exporting

- *File › IFC4 (.ifc)*: the usual IFC file, for Revit, ArchiCAD, Vectorworks, Bonsai (BlenderBIM), FreeCAD, Solibri and IFC viewers.
- *File › IFC5 (.ifcx, alpha)*: the new JSON-based IFC. Its schema is still an alpha, so other tools may not read it yet.

What goes out: levels (storeys), walls with their location line and layers, slabs, roofs, doors and windows with their hand and swing, openings and shafts, columns, beams, stairs, railings, curtain walls with their mullions and panels, types, materials with their colours, and masses with their IFC class. Guides, sketches and crop boxes stay behind.

## Opening an IFC file

*File › Open…* reads IFC2X3, IFC4 and IFC4X3 files, and `.ifcx`.

- Straight walls, slabs on a storey and the doors and windows in those walls become native, editable CreaBIM elements. Types, assemblies, materials and grid systems come back too.
- Everything else with a shape (furniture, terrain, curved walls…) comes in as a mesh that keeps its IFC class.
- Units, placements and IFC ids are kept, and a plan is made for each storey.

## Materials only

*Materials › Import materials…* reads just the materials of an IFC file, with their categories and surface styles.
