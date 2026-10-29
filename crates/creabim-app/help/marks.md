# Marks and numbering

A **mark** is the short code that identifies a door, a window or another element on drawings and in schedules: D01, W-101, C3. Its [tags](tags) and the [schedules](schedules) show it, so they always agree.

## Typing a mark

Select the element and type in **Mark** in *Properties*, under its name. Leave the field empty and the element takes the number the project's rules give it; the empty field shows that number in grey.

If another element already has the same mark, *Properties* says so under the field. **Select them** picks the others.

## Numbering rules

**File > Mark numbering…** sets how elements without a typed mark are numbered. Each kind of element has its own rule:

- **Number**: number this kind automatically. When it is off, only typed marks show, and a tag shows what it showed before (a wall's material and thickness, for example).
- **Prefix** and **Sep.**: the letters before the number, and what goes between them: `D` and nothing gives D01, `D` and `-` gives D-01.
- **Digits**: how many digits the count has, with leading zeros.
- **By level**: put the level's number before the count, and start the count again on each level. Level 1, door 1 is D-101; level 2, door 3 is D-203.
- **Order**: *Model order* counts in the order the elements were made, lowest level first. *Clockwise per level* goes round each level from the top of the plan.
- **From**: the first number.

**Ground level number** is the number of the lowest level at or above ±0. Levels above count up from it and levels below count down, so a site level under the ground floor is level 0.

The **D01, W01** and **D-101, W-101** buttons set the usual two schemes. The rules belong to the project. **Save rules…** writes them to a file and **Load rules…** reads one, so an office can use the same numbering in every project.

## Renumbering

The rules number elements live: add a door and the doors after it move up by one. When the numbers are settled, **Renumber** writes them onto the elements as their marks, so later changes leave them alone and new elements take the next free numbers. Pick one kind or every kind. With **Replace typed marks too**, every element is numbered afresh from the rules.

**Marks from names** is for models whose marks were written into the names, such as "D-101 sliding doors": each element takes the code at the start of its name as its mark.

## Checking for duplicates

The bottom of the dialog lists every mark that more than one element has, with a **Select** button for each. Objects of one type share their type's mark on purpose, so they are not counted twice.

## In IFC

A mark is written as the element's IFC **Tag** and read back from it when an IFC file is imported.
