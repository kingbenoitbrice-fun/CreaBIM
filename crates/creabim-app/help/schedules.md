# Schedules

A **schedule** is a table of elements, made from the model: every door with its mark, size and swing; every wall with its type, length and area. Like other views, it is always up to date.

## Making one

*File › New schedule* (or *Create › Schedule*), then *Doors*, *Windows*, *Walls*, *Slabs*, *Columns and beams*, *Furniture* or *Fixtures*. It opens in the 2D pane and appears under *Schedules* in the browser.

## Furniture and fixtures

*Furniture* and *Fixtures* schedules count [objects](furniture) rather than list them. Each line is one type on one level: its mark (as its [tags](tags) read), type, level, mounting height and IFC class, and how many there are. Lines are grouped under a heading for each category, such as *Electrical*, *Lighting* or *Bathroom*, with the category's count. The total is at the bottom.

- **Furniture** is everything IFC calls furniture: sofas, beds, tables, chairs, wardrobes, kitchen cabinets.
- **Fixtures** is everything else: sanitary ware, appliances, and the [electrical, plumbing and HVAC fixtures](fixtures).

Click a line's mark to select every object of that type. Tick *Group by type* to list each object on its own row under its type instead, with its level and height.

## Using it

- **Edit in the table**: change a window's width or a wall's material right in a cell. The model, the plans and the 3D view change with it.
- Click a row's mark to select that element.
- Totals are at the bottom.
- Tick *Group by type* in the schedule's *Properties* for a heading row per [type](types), with its count. Click a heading to select all of its rows.
- **Export CSV** on the title line saves the table for a spreadsheet.

Place a schedule on a sheet like any view. See [Sheets](sheets).
