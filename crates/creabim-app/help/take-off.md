# Take-off

The **take-off** measures what the model is made of: how many square metres of plaster, cubic metres of concrete, metres of beam. Give a material a **rate** and the take-off prices it. Like a schedule, it is read live from the model, so it changes as you draw.

## What it measures

Walls, slabs, roofs, ceilings, columns, beams and footings each give one line for each material they are built of:

- an element with an [assembly](assemblies) gives a line for each layer (an air gap with no material is left out), as thick as the layer;
- anything else gives one line for its own material.

| Element | Area | Volume | Length |
|---|---|---|---|
| Wall | its face, less its doors and windows | area × thickness | along the wall |
| Slab, ceiling | plan area, less holes | area × thickness | - |
| Roof | the area of its slopes | area × thickness | - |
| Column | - | section × height | its height |
| Beam | - | section × length | its length |
| Footing | its plan area | area × thickness | a strip's length |

Quantities are in metres, square metres and cubic metres.

## Rates

A rate is a price for each square metre, cubic metre, metre or piece of a material, in whatever currency you price the project in. Set it:

- in the *Materials* window: *Rate*, a number and what it is *per*; *Clear* takes it away;
- or in the *Take-off* table itself, in the *Rate* and *Per* columns.

A material without a rate is measured but left out of the priced total.

## The Take-off table

*File › New schedule › Take-off* (or *Create › Schedule › Take-off*). It opens like a schedule, with a row for each material: the elements it is in, its area, volume and length, its rate, and what it costs. The total cost is at the bottom.

- Tick *Group by level* in its *Properties* to list each level's materials under a heading with the level's totals.
- Click a material's name to select everything made of it.
- Place it on a sheet like any schedule. See [Sheets](sheets).

## To Excel

*File › Export › Schedules to Excel (.xlsx)*, or **Export Excel** on any schedule, writes a workbook: a *Summary*, a sheet for each schedule, then *Take-off* (totals by material and by level, priced) and *Take-off lines* (every element's materials, layer by layer, with a filter on each column). Numbers go in as numbers, so the spreadsheet can sum and sort them.

> **Tip:** From the command line, `creabim xlsx house.creabim house.xlsx` writes the same workbook.
