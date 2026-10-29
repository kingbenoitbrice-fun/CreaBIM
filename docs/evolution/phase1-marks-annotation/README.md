# Phase 1: marks and annotation

This round covers the editable marks of P3, the leaders, spot levels and dimension breaks of P5 (items 2, 4 and 5), and the level heads of P2 (item 3), all from the drawing-quality spec. The project holds the conventions as data: how marks are numbered, which pattern a spot level reads, whether a dimension style breaks at tags, and where each view puts a level's head. The code only applies them.

## Screenshots

| Shot | What it shows |
| --- | --- |
| `01-plan-marks-leaders.png` | Starter house plan with level-based marks: doors D-101 to D-110, windows W-101 to W-108. Some tag leaders are bent (D-103, D-104 and D-107 end in a dot). There is a level marker (±0.00 FFL) on the floor slab, and a note whose leader bends at an elbow and ends in an open dot. |
| `02-mark-numbering-dialog.png` | *File › Mark numbering…*. One rule per kind sets the prefix, separator, digits, level-based numbering (D-101), order (model order or clockwise round each level) and the first number. It also has the ground level number, two ready-made rules (D01/W01 and D-101/W-101), Save and Load for the rules, Renumber, Marks from names, and the list of duplicate marks. |
| `03-properties-duplicate-mark.png` | Two windows were both typed as W-1A. *Properties* shows the Mark field and a warning, "W-1A is also the mark of 1 other element", with a **Select them** button. |
| `04-section-spot-levels-level-heads.png` | Section A-A. A spot level reads `±0.00 FFL` from its pattern, with a triangle on the floor. The Mezzanine (+300) and Ground Floor (±0) heads are too close to fit, so they spread apart up and down, each with a dog-leg. |
| `05-level-head-dragged-per-view.png` | The Mezzanine head has been dragged by its grip, in this view only. **Put it back** appears next to **Head in Section A-A**. The Ground Floor head returns to its usual place because nothing crowds it any more. |
| `06-dimension-breaks-level-marker.png` | The plan's dimensions use the style "Chain, breaks at tags". The 5000 line opens around the D-102 bubble, and the witness lines open around W-101 and W-102. The plan's level marker (±0.00 FFL) is a target with a leader. |
| `07-tag-leader-grips.png` | A selected tag (D-104) has a round grip for the elbow and a square grip for the end. Here the elbow has been dragged out and the end moved onto the door leaf. *Properties* now offers **Straighten the leader** and **Leader back to the centre**. |
| `08-villa-marks-from-names.png` | TropicalVilla_REV00 after **Marks from names**. The villa's own D-0xx, D-1xx, D-2xx and W-1xx marks come from its element names and are now typed marks, so its tags show them. |
| `09-villa-mark-numbering-duplicates.png` | The villa's dialog lists 3 duplicate marks: the roof segments R1, R2 and R3 share their type marks. |
| `10-villa-door-schedule.png` | The villa's door schedule reads the same marks. |

## How to try it

- **Marks:** select a door, window, column or other markable element. *Properties* shows **Mark**, and when the field is empty its hint is the number the rules give. Type a mark to fix it. Two elements with the same mark get a warning, and the dialog lists them too.
- **Numbering rules:** use *File › Mark numbering…*. Tick **By level** for D-101 numbering: the lowest level at or above ±0 takes the ground number, and each level above takes the next. Choose **Clockwise** to number round each level from north. **Save rules…** and **Load rules…** move the rules between projects as JSON.
- **Renumber:** writes the numbers the rules give onto every element that has no mark of its own, so they stay put when more elements are added. Tick **Replace typed marks too** to renumber everything. Both are one undo step.
- **Marks from names:** takes marks such as D-101 or W2 out of element names. This suits imported files like the villa, which keep their marks in their names.
- **IFC:** marks are written to IfcElement Tag and read back. Bare numbers from other programs, which are usually ids, are not read as marks.
- **Leaders:** select a tag or note and drag its round grip to bend the leader, or its square grip to move where it ends. The **Leader end** choice in *Properties* overrides the style: arrow, dot, open dot and the rest.
- **Spot levels:** **Spot level**, under *Create* or in the toolbar's annotation group. In a section or elevation, click a point and then where the text goes; it reads the point's height. In a plan it reads the top of the slab under the click, or the plan's level when there is no slab. The **Reads** pattern in *Properties* takes `{elevation}`, `{m}`, `{m3}`, `{mm}` and `{level}`, for example `{elevation} FFL`.
- **Dimension breaks:** in *Annotation styles*, a dimension style with **Break dimension lines at tags** opens its lines where tag bubbles and note text sit across them.
- **Level heads:** heads that are too close spread apart by themselves. Dragging a level's line or head still moves the level. To move only the head, select the level and drag the square grip on its head. **Head in** (the view's name) in the level's *Properties* hides the head in that view, and **Put it back** undoes a drag.

## Known gaps

- IFC5 (`.ifcx`) export does not carry marks yet. IFC4 does.
- Object categories (furniture, fixtures) have no prefixes in the mark rules yet. A mark typed on an object or its type does show in tags and schedules.
- Spot levels work in plans, sections and elevations, but not in axonometric views.
- The spot level tool takes its pattern from the last spot level made. The option bar has no pattern field, so the pattern is changed in *Properties*.
- Level heads spread apart only up and down, and only in sections and elevations.
- Dimension lines break only at tags and notes, not at room labels or other text. The label decluttering from the drawing-quality branch may still move a tag off a dimension line; the breaks are worked out after it has done so.
- The duplicates list in the Mark numbering dialog is the only place that lists all duplicates. There is no separate model-check window.
- No new keyboard shortcuts were added.
- The project browser search does not find elements by mark.
