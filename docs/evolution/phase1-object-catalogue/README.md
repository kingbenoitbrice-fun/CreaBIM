# Phase 1: furniture and fixtures, and the object catalogue

CreaBIM can now place objects: sinks, WCs, baths, showers, kitchen units,
sofas, chairs, tables, beds, wardrobes and desks. Each object is an
instance of an object type. The type holds the shape (parts, each in its
own material), the category, the IFC class and whether the object stands
against a wall or on its own. A placed object only says where it is, how
it is turned and how far above its level it sits.

- **Starter catalogue**: 19 objects in bathroom, kitchen, living, dining,
  bedroom and office, with their materials (sanitary ceramic, chrome,
  brushed steel, oak, walnut, laminate, a sage kitchen front, quartz
  worktop, fabrics, linen, appliance enamel).
- **Catalogue window** (Create › Catalogue…, or the sofa tool): browse by
  category with pictures, search, and click an object to place it. It has
  three groups: *Starters*, *Yours* (your own catalogue) and *In this
  project* (the project's object types, including those read from IFC).
- **Object tool**: click in a plan or the 3D view. Sinks, WCs, baths,
  kitchen units, beds and sofas back onto the nearest wall face within
  600 mm. Other objects, or any object with *Free of walls* on, go where
  you click at the turn set in the bar. Space turns the next one a
  quarter. Plan and 3D show the object under the pointer before you click.
- **Plans** draw each object's 2D symbol, with Projection lines, or Hidden
  lines when the object sits above the cut, like a wall cabinet. **Sections,
  elevations and 3D** show the full 3D parts in their materials.
- **Your own objects**: *Import from Blender, FreeCAD or glTF…* reads a
  `.blend`, `.FCStd`, `.gltf` or `.glb` file as one object. Each mesh
  becomes a part in its material. The object is set on the floor with the
  middle of its back edge at the origin, and its category, IFC class and
  mounting are guessed from the names. *Save selection to catalogue*
  keeps a placed object's type, masses, or any other elements as their
  3D shapes. Your catalogue is one `.json` file per object in
  `<config>/CreaBIM/catalogue`, and is shared by every project. The
  project file (`.creabim`) carries the types of the objects placed, so
  it opens anywhere.
- **Properties**: a placed object has its height above its level and its
  turn. Below them are its type's category, placement, IFC class and
  predefined type, and each part's material. Edits are ordinary undoable
  transactions, and changing the type changes every object of it. The
  Types window has an *Objects* group.
- **Move and Align**: an object keeps its type's shape, so pushing or
  pulling one of its faces moves the whole object. A side face slides
  it, and the top or bottom raises or lowers it. Align's Stretch therefore
  puts a sofa's back flush with a wall.
- **IFC4**: object types export as IfcFurnitureType,
  IfcSanitaryTerminalType, IfcElectricApplianceType or
  IfcSystemFurnitureElementType with their predefined type (SOFA, BED,
  WASHHANDBASIN, TOILETPAN, SINK, FRIDGE_FREEZER...). Each type carries two
  representation maps: a *Body* map of styled face sets, one per part,
  and a *FootPrint* map of the plan symbol. Its materials go on an
  IfcMaterialConstituentSet. The placed objects are occurrences that use
  IfcMappedItem, typed by IfcRelDefinesByType. Importing reads all of this
  back. CI now exports the furnished sample house and checks it with
  IfcOpenShell.

## The screenshots

1. `01-furnished-ground-floor-plan.png`: the furnished sample house's
   ground floor (File › New furnished sample house). Living room on the
   left with sofa, armchair, coffee table and desk. Kitchen on the right
   with units, cooker, sink, fridge and dining table, all drawn as their
   plan symbols.
2. `02-furnished-house-in-3d.png`: the same house in 3D with its walls
   hidden. Upstairs are the beds, bedside tables, wardrobe, bath, WC,
   basin and glass shower. Downstairs are the kitchen, dining chairs and
   sofa.
3. `03-catalogue-window.png`: the Catalogue window with the starter
   pictures (rendered in the app from the objects' own parts and
   materials).
4. `04-catalogue-living-with-imported-gltf-lamp.png`: the Living
   category. *Yours* holds a floor lamp brought in from a glTF file (the
   format Blender exports), with base, pole and shade in their own
   materials.
5. `05-object-tool-preview-in-plan.png`: the Object tool with the lamp
   under the pointer. The tool bar shows what is placed, the turn,
   *Turn 90°* and *Free of walls*.
6. `06-placed-lamp-properties.png`: the placed lamp in Properties: type,
   height above level, turn, and its type's category, placement, IFC class
   and the three parts' materials.
7. `07-washbasin-backs-onto-wall.png`: a washbasin found the north wall's
   inside face and turned its back to it before the click. Properties
   shows the basin placed earlier: IfcSanitaryTerminal WASHHANDBASIN,
   ceramic and chrome.
8. `08-types-window-objects.png`: the Types window's Objects group, with
   how many of each are placed, and the sofa type's parts.
9. `09-furniture-in-section.png`: Section A through the house, with the
   wardrobe and bed upstairs and the sofa, coffee table and lamp
   downstairs, drawn from their 3D parts.

## Try it

- File › New furnished sample house, then Create › Catalogue… and click
  anything to place it. Space turns it; Esc ends.
- Import your own: in the Catalogue window, *Import from Blender, FreeCAD
  or glTF…*. From a terminal:
  `creabim catalogue-import chair.blend <config>/CreaBIM/catalogue`.
- `creabim catalogue` lists the starters, `creabim catalogue <dir>`
  writes them as catalogue files, and `creabim sample-furnished
  out.creabim.json` writes the furnished house (then
  `creabim ifc out.creabim.json out.ifc`).

## Known gaps

- Pasting objects into another project drops them when that project
  lacks their type. Use the catalogue instead.
- IFC5 (`.ifcx`) export leaves objects out. IFC4 has them.
- There is no furniture schedule yet.
- IFC import takes a mapped item's transformation as the identity, which
  is what CreaBIM and most tools write. Scaled or mirrored mapped items
  come in unscaled.
- A Blender object is one part in its first material. Faces that use
  other material slots take that first one.
- The name guess only knows English words. Category, IFC class and
  mounting can be set in Properties once the object is placed.
- The user catalogue folder is read once per session, and again after an
  import, save or delete in the window. A file added to it by hand shows
  after a restart.
