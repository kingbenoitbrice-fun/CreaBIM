# Menus that fit, alignment tracking, the Levels window and the project browser

Fixes for the round of feedback of 15 October: the View menu that "stretches
endlessly", drawing a slab that wouldn't line up with its start, controlling
levels outside sections and elevations, and where new plans go in the browser.

## The shots

| Shot | What it shows |
| --- | --- |
| `00-browser-grouped-by-kind.png` | The project browser lists views by kind: Floor plans (marked "By level"), 3D views, Sections, Elevations, Schedules, then Sheets and Levels (highest first). |
| `01-view-menu-after.png` | The View menu: as wide as its longest item and much shorter. Docked layout, Camera (orbit, orthographic, look from), Ground grid (on/off and spacing) and Theme (with Next theme) are now submenus. |
| `02-long-menu-capped-scrolls.png` | A long menu (View › 3D view shows) in a short window: capped at the window, scrolls with a "more below" arrow, and only a scrollbar's width wider. |
| `03-level-tool-opens-levels-window.png` | Picking the Level tool over a plan opens the Levels window (its tooltip and the status bar say why). |
| `04-levels-window-new-level.png` | "Add level above the top": Level 2 at +6000 with its plan, which lands in Floor plans after First Floor plan, by level. The window now also shows the gap "To next" level, with a Fit button when the storey height differs from it. |
| `05-plans-dragged-your-order.png` | First Floor plan dragged above Ground Floor plan. The group now reads "Your order" and gets a sort button. |
| `06-sort-by-level.png` | The sort button ("Sort by level") puts the plans back in level order. |
| `07-slab-closing-corner-tracked.png` | The case from the feedback: drawing a slab polygon, the last corner snaps where the Y axis of the previous corner crosses the horizontal through the start ("On Y axis · in line with the start"), with dashed alignment lines. |
| `08-slab-closed-square.png` | The same slab closed: square corners. |
| `09-wall-chain-alignment.png` | A chain of walls: left, held in line with the start of the chain on its own; right, the crossing with the last corner's Y axis. |

## How to try it

- **Menus**: open View, File, Create, and View › 3D view shows on a short window. Slide between menus with the pointer.
- **Alignment tracking**: with the Slab (polygon), Wall, Sketch, Beam, Railing or Roof tools, place a few points, then move the pointer level with or plumb below an earlier point. The start, every corner of the shape, and any point snapped to and rested on for a moment while drawing (the last four) are tracked. Where two alignments cross, or one crosses an edge, is a snap point too. It works in plans, in sections and elevations (sketches), and in the 3D view on the working level (slab outlines, wall chains, sketches). It is one more switch in the snap list ("Alignment with the shape's points"); F3 and Alt turn it off with the rest.
- **Levels window**: View › Levels…, File › Levels…, Create › Levels…, the ⋯ button of the Levels group in the browser, Properties › Levels… on a selected level, or the Level tool used anywhere but a section or elevation.
- **Browser order**: drag a view up or down within its group. The group then keeps your order (saved with the project); the sort button next to its count gives back the automatic order.
- **Plans follow their level's name**: rename a level (Levels window, or its name in Properties): plans named after it ("Level 2 plan", "Level 2 plan - furniture") follow ("Roof plan"); plans you named yourself are left alone.

## Design choices

- **Why the menu stretched**: the popup page measured the menu as it was placed. The menu was at least as wide as the page, so the width it reported was the page's own width plus room for the scrollbar, and every placement made it 10 px wider. The page now measures the menu's natural size apart from the page it is shown in, so the size no longer depends on where it is shown.
- **Derived plan names**: no flag is stored. A plan "is named after" its level when its name is the level's name, or the level's name followed by a space and more words. Renaming a level renames those plans by swapping that prefix, in the same undo step. "Level 20 plan" is not named after "Level 2", and a plan you renamed to something else no longer matches, so it is left alone. Older files need no migration. Renaming a level in Properties now commits when the field loses focus (like the Levels window), so the plans follow in one undo step.
- **Browser order** is a new optional `browser_order` on views (`#[serde(default)]`, not written when automatic). Plans' automatic order is by level elevation; other groups keep the order the views were made in. A view made while a group has a hand order goes to the end of it until you sort.
- **Levels window**: PR #17 already had a Levels manager (add, add at an elevation, duplicate, rename, set elevation and height, make or open the plan, delete with a choice of where elements go), so this builds on it rather than adding a second one: the Level tool opens it outside sections and elevations, it shows the gap to the next level with Fit, and its hints explain storey height. Levels aren't reordered by hand: their order is their elevation, so changing an elevation reorders them.

## Known gaps

- The rename step isn't screenshotted: typing into egui text fields doesn't work under the headless X server used for screenshots (no window manager or input method). The renaming is covered by tests (`plans_the_user_named_keep_their_names_when_the_level_is_renamed`).
- In the 3D view, tracking covers slab outlines, wall and beam chains and sketches; guides drawn on a workplane don't track yet. Tracking uses the model's X and Y axes, so on a tilted workplane the alignment lines are its X and Y projected onto it.
- Points rested on are acquired after 0.4 s on a point snap, only in 2D panes.
- The classic egui panels' outliner still lists views in one group.
