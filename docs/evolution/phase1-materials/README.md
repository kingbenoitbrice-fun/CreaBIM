# Phase 1: the material library, the material browser and importing materials

Reference screenshots, taken from the sample house with library materials added. Its walls are *Brick, common red*, the pergola and canopy *Steel, stainless*, and the south and east ground-floor walls *Glass, tinted*.

**A material library.** CreaBIM now has 52 common building materials, grouped under 17 categories: concrete, brick, block, stone, wood, steel, aluminium, other metal, glass, insulation, gypsum, plaster and render, finishes, roofing, membranes, plastics and earth. Each one has a description, a colour, a cut hatch and surface properties: roughness, metallic and opacity. The categories are the values of IFC's IfcMaterial.Category. Adding a material that the project already has resets it to the library's version.

**Materials have surfaces.** A material now stores its category, description, roughness (0 is mirror-smooth, 1 is matt), metallic and opacity, next to its colour and cut hatch. The 3D view uses them:
- rough materials stay matt
- smooth ones get a highlight and a reflection of the sky
- metals tint their reflections
- materials with an opacity below 1 are see-through (a selected element is drawn opaque)

The 3D view's colours are now encoded for the screen, so materials show the colour you picked instead of a darker one.

**The material browser** (*Materials > Material browser…*) lists the project's materials by category, with how many elements use each one. From it you can:
- add, duplicate, rename and edit a material, with a live preview ball
- select the elements that use a material, or assign it to the selection
- delete a material. If elements still use it, the browser asks what they should be made of instead, and changes them in the same step.

Every change goes through the undo history. A slider drag is one undo step.

**Importing materials** (*Materials > Import materials…*, or the *Import…* button in the browser) reads:
- IFC files: every IfcMaterial, with its category, description and surface style
- FreeCAD material cards (`.FCMat`), in the old INI format and the FreeCAD 1.x YAML format
- glTF (`.gltf`, `.glb`), for example exported from Blender, using pbrMetallicRoughness
- Blender `.blend` files: the Principled BSDF's colour, roughness, metallic and alpha, falling back to the viewport display settings

An imported material whose name is already in the project updates that material in place, so its users follow. New names are added. Categories and hatches are guessed from the name when the file has no category.

| Screenshot | What it shows |
| --- | --- |
| [01-library-menu.png](01-library-menu.png) | *Materials > Add from the library*: the library's categories, each a submenu of its materials with an *Add them all* item. |
| [02-library-tab.png](02-library-tab.png) | The browser's *Library* tab: the 52 materials by IFC category, with swatch, description, cut hatch and an *Add* button. *Add all* adds a whole category. |
| [03-project-materials.png](03-project-materials.png) | The *In this project* tab: the project's materials by category, with user counts. *Steel, stainless* is selected: metallic 1, roughness 0.25. The preview ball shows a tinted highlight. *Select them* selects its 3 users. |
| [04-glass-editor.png](04-glass-editor.png) | *Glass, tinted* in the editor: opacity 0.45, roughness 0.05. The preview ball is see-through over a checkerboard. |
| [05-assign-menu.png](05-assign-menu.png) | A wall selected, then *Materials > Assign material*: every project material, and *By type (no material)*. The properties panel shows the wall's current material. |
| [06-3d-result.png](06-3d-result.png) | The result in 3D: brick walls, a steel pergola and canopy, and the glass ground-floor wall with the room, the canopy and the door seen through it. |
| [07-delete-in-use.png](07-delete-in-use.png) | *Delete* on *Brick, common red*, which 6 elements use: the browser asks what they should be made of instead and reassigns them in the same undo step. |
| [08-properties-panel.png](08-properties-panel.png) | A material selected in the project browser: the properties panel has the same editor, its user count and *Open in the material browser*. |
| [09-import-fcmat.png](09-import-fcmat.png) | Two FreeCAD cards imported: *Concrete-Generic* (old INI format) and *Glass-Generic* (YAML format). The glass card's transparency 0.7 became opacity 0.3, and its shininess 0.95 became roughness 0.05. Category and hatch were guessed from the card. |
| [10-import-gltf.png](10-import-gltf.png) | A glTF file exported from Blender: five materials (*Terracotta render*, *Oiled oak*, *Corten steel*, *Polished brass*, *Blue glazing*). Their base colours were converted from linear to sRGB, and their roughness and metallic were kept. |
| [11-import-ifc.png](11-import-ifc.png) | The materials of a Revit IFC export (web-ifc's example): *Concrete, Cast-in-Place gray* (category concrete, with its rendering colour) and *Metal - Steel - 345 MPa* (metallic, from its `.METAL.` reflectance). |
| [12-import-blend.png](12-import-blend.png) | A `.blend` file's materials: *Wood* was added (alpha 0.5, roughness 0.4, from its viewport settings), and the project's *Concrete* was updated from the file's material of the same name. |

Screenshots 09 to 12 were taken with the classic panels (`CREABIM_CLASSIC_UI=1`), whose *Import materials from the path in the File box* item works without a file picker. The test display has no file picker.

## How to try it

- *Materials > Add from the library* adds one material or a whole category. The browser's *Library* tab does the same, with descriptions.
- Select walls, slabs or masses, then use *Materials > Assign material*. You can also click *Assign to the selection* in the browser, or use the Material dropdown in the properties panel.
- *Materials > Material browser…* edits the project's materials. Selecting a material in the project browser on the left shows the same editor in the properties panel.
- *Materials > Import materials…* picks one or more `.ifc`, `.FCMat`, `.gltf`, `.glb` or `.blend` files. Opening (*File > Open…*) or dropping a `.FCMat`, `.gltf` or `.glb` file also imports its materials into the open project.
- Every step undoes with Ctrl+Z.

## Data and exchange

- `Material` gains `category`, `description`, `roughness` (default 0.8), `metallic` (default 0) and `opacity` (default 1). Each is optional in files and left out when it has its default value. Older projects open unchanged, and a project whose materials have default surfaces saves the same as before.
- IFC4 export writes:
  - IfcMaterial's Category and Description. CreaBIM's cut hatch goes at the start of the description, as "CreaBIM hatch: Brick." The rest of the description follows it.
  - an IfcSurfaceStyleRendering with the colour, transparency, an IfcSpecularRoughness and the reflectance method (`.METAL.`, `.GLASS.` or `.NOTDEFINED.`)
  - materials no element uses
- IFC4 import reads all of the above back, and reads IfcSurfaceStyleShading or IfcSurfaceStyleRendering from other tools. The library makes the round trip through IFC4 unchanged (tested).
- IFC5 (IFCX) writes and reads an opacity attribute (`bsi::ifc::presentation::opacity`) when a material is see-through.
- glTF export (*File > Export > glTF*) writes each material's roughness and metallic factors, and its opacity with `alphaMode: BLEND`, so Blender gets the same surfaces.

## Known gaps

- No textures. glTF base colour textures and Blender image textures are ignored; a material with only a texture comes in white or at its base factor.
- Blender: only the Principled BSDF's unlinked inputs are read. A node tree that drives the colour from other nodes falls back to the viewport display colour.
- FreeCAD: only the BasicRendering/Rendering colours, shininess and transparency are read. Render-workbench appearance models and physical properties (density, strength) are not kept.
- IFC: IfcSpecularExponent is not turned into a roughness (its scale differs between tools). Layer sets, profile sets and constituent sets aren't read as separate materials. A full IFC open (*File > Open…*) still only creates the materials that elements use; *Import materials…* reads them all.
- The cut hatch has no IFC attribute, so it is kept at the start of IfcMaterial.Description. Other tools show that text.
- Importing a material whose name the project already has replaces its colour and surface (undoable). There is no "keep both" choice yet.
- See-through faces are drawn after the opaque ones but are not sorted. Two glass walls behind each other can show in the wrong order.
- The 3D view's new colour encoding makes the whole 3D view lighter than before. Grid lines and other line overlays keep their old look.
- Only walls, slabs and masses take a material. Doors, windows, roofs, columns and beams have none of their own yet.
- The test display has no file picker, so the import dialog itself is not in the screenshots.
