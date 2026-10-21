# Wall behaviour: location line, flips, crossings, free ends, level constraints and attached tops

This round covers six points from the 2026-10-21 feedback (pages 6, 7, 13, 15, 16 and 17):

- **Location line.** A wall now has a location line, as in Revit: wall centre, core centre, exterior face, interior face, core exterior face or core interior face. You draw and dimension the wall on that line, and its end grips sit on it. The line stays where it is when the wall gets thicker, gets another assembly or is flipped. The IFC wall axis is the location line, and `IfcMaterialLayerSetUsage` measures its offset from that line.
- **Flips.**
  - A wall flips its exterior and interior about its location line. Its doors and windows turn with it.
  - A door flips its hand (which jamb the hinge is on) and its swing (which side it opens to). A window flips inside/outside, and its sill board moves to the other face.
  - Plans, 3D, the door schedule (a new *Swing* column) and IFC (`OperationType` and the turned fill placement) all follow.
- **Crossings.** Two walls that cross make a clean cross with one hatch, for plain and layered walls. The thicker wall runs through, or the first one drawn if they are as thick. A wall that ends on another wall still makes a tee.
- **Free ends.** You can disallow the join at each end of a wall, from the right-click menu on that end or from *Joins at start / end* in Properties. A disallowed end stays square and is not pulled along when its neighbour moves. Holding Ctrl or Alt while dragging an end grip drags that end free of the walls joined to it.
- **Level constraints.** A wall has a base level and base offset, and a top that is either *Unconnected* (its own height) or *Up to* a level plus an offset. When that level moves or changes, the walls tied to it update in the same undo step. IFC exports the height that results. Files saved before this round load unchanged.
- **Attached top.** *Attach top to roof/slab* makes the wall's top follow the underside of the roof or slab above it, cut to the slope. This works in 3D, in plans and sections, and in IFC, where it is an `IfcBooleanClippingResult` with one `IfcPolygonalBoundedHalfSpace` per roof face. Eave and gable walls stop under the roof, overhang included, and no longer poke through it. The first-floor walls of the layered sample house are attached to its roof.

| Screenshot | What it shows |
|---|---|
| `01-location-line-exterior-face.png` | The north wall of the sample house, set to *Exterior face* and then made 600 mm thick. The outer face stays at Y 6150, and the wall grows inwards. Properties show the new Location line, Flip, Base level, Top and *Joins at* fields. Start/End X/Y are the ends of the location line. The orange flip arrow sits outside the exterior face. |
| `02-wall-flipped.png` | The same wall after *Flip*: it turns about its exterior-face line. It now reads "Exterior on the left", and the flip arrow is inside. |
| `03-door-flip-arrows.png` | The front door selected. Properties read *Hung right, in* and show *Flip hand* and *Flip swing*. In the plan, two flip arrows sit outside the door: one across the wall for the swing and one along it for the hand. |
| `04-door-flipped-hand-and-swing.png` | After clicking both arrows, the door is hinged on the other jamb and opens outwards (*Hung right, out*). The 3D view shows the leaf. |
| `05-wall-tool-location-menu.png` | The Wall tool's option strip gains *Location line* (a menu of the six lines) and *Exterior left* (Space toggles it while drawing). |
| `06-wall-preview-exterior-face.png` | Drawing a wall on its exterior face. The preview shows both faces and a tick on the exterior side. |
| `07-crossing-walls-clean-cross.png` | The new 200 mm wall crossing the 100 mm partition. The thicker wall runs through, and the partition stops on both sides with no overlapping hatch. |
| `08-right-click-disallow-join.png` | Right-clicking near a wall's end offers *Flip wall*, *Disallow join at its end* and *Attach top to roof/slab*. |
| `09-disallowed-end-drags-alone.png` | With the join disallowed (red cross at the end), dragging the end moves it alone. The west wall stays where it was. |
| `10-ctrl-drag-end-free.png` | Ctrl-dragging the other end leaves the east wall and the partition behind. The status bar reads "Stretch GF Wall 3 free". |
| `11-top-constraint-up-to-level.png` | Top set to *Up to First Floor*: a Top offset field appears and the height reads "from its top constraint". |
| `12-level-moved-wall-follows.png` | The First Floor raised to 4500 mm. The tied wall is now 4500 mm tall and reaches the moved floor in 3D, while the unconnected ground-floor walls keep 3000 mm. |
| `13-attached-tops-in-section.png` | Layered sample house, Section A. The first-floor walls are attached to the roof, and their tops follow its underside. |
| `14-eave-wall-cut-to-roof-slope.png` | A close-up of the eave: every layer of the wall stops on the sloped underside of the roof, under the overhang. |

## How to try it

1. Choose *File › New sample house* (or *File › New layered sample house* for the roof attachment) and open the Ground Floor plan.
2. Location line:
   - Select a wall and pick a *Location line* in Properties, then change its thickness: the chosen line stays put.
   - With the Wall tool, pick the line in the option strip before drawing.
3. Flips:
   - Wall: click *Flip* in Properties, click the orange arrow by the wall in plan, press Space, or right-click and choose *Flip wall*.
   - Door: select it. Space or the across-the-wall arrow flips the swing, and Shift+Space or the along-the-wall arrow flips the hand.
   - Window: Space or its arrow turns it inside/outside.
4. Crossings: draw a wall straight through another one.
5. Free ends:
   - Right-click near a wall's end and choose *Disallow join at its end*, or clear *Joins at start/end* in Properties.
   - Or hold Ctrl or Alt while dragging an end grip.
6. Level constraints:
   - In Properties set *Top* to *Up to First Floor* and give it a *Top offset*.
   - Then select the First Floor level and change its elevation: the wall's height follows.
   - Undo puts both back in one step.
7. Attached top:
   - In the layered house the first-floor walls are already attached. Open Section A, or orbit the 3D view to a gable.
   - On any wall with a roof or slab above it, use *Attach top to roof/slab* (Properties or the right-click menu) and *Detach*.
8. *File › Export › IFC4*: the axis follows the location line, door `OperationType` follows the hand, and attached walls are clipped.

## Tests added

- **Core:**
  - Walls tied to a level follow it, through undo, and become unconnected when that level is deleted.
  - A thicker or re-assembled wall keeps its location line, and a flip turns about it.
  - Files saved before this round still load (serde defaults for walls and openings).
  - An attached top follows a pitched roof's faces.
  - Crossing walls make a cross; a wall ending on another is a tee, not a cross.
  - A disallowed join leaves the end square and free.
- **Geometry:**
  - A crossing wall leaves a gap.
  - An attached top follows the slope.
  - Doors flip hand and facing.
  - A window faces out of the exterior unless flipped.
  - A plan fills a crossing once, for plain and layered walls.
- **IFC** (`crates/creabim-ifc/tests/walls.rs`):
  - The axis is the location line, and it round-trips.
  - Attached walls are clipped by the roof.
  - A level-tied height is exported.
  - Doors say which way they are hung.
- **App:** a wall end is gripped on its location line and may be left unjoined.

## Known gaps

- Flipping a wall, or changing its thickness, about a line other than its centre moves its centreline. The walls joined to it are not stretched to the new corner, so a corner can open or overlap until one of them is touched.
- The core-line locations (core centre, core faces) are kept when the wall's thickness or assembly changes. They are not kept when the layers inside an assembly are edited.
- Door hand and swing are exported to IFC but not read back on import. An imported door comes back with the default hand and swing.
- Where an attached wall is only partly under the roof and the roof rises above the wall's own height, the uncovered part keeps the wall's height in 3D. In IFC that part is still extruded to the roof's highest point.
- Crossing gaps are worked out per level. Walls on different levels that overlap in plan are not cut against each other.
- The plan cut does not take the attached top into account. A wall whose attached top drops below the cut height is still drawn as cut.
- *Attach top to roof/slab* in the right-click menu is offered for any selected wall. When nothing is overhead it only says so in the status bar; Properties disables the button instead.
- No new icons were added. The flip arrows are drawn in the view, and the menu items reuse the wall, door and link icons.
