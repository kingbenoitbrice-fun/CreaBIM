# Copy, paste, delete and undo

## Undo and redo

Every change can be undone: `Ctrl+Z` undoes, `Ctrl+Y` (or `Ctrl+Shift+Z`) redoes. The *Edit* menu says what will be undone ("Undo Move wall"). A drag, a move or a change to a value is one step. The history is saved in the project file, so it is still there after you reopen it.

## Copy, cut and paste

- `Ctrl+C` copies the selection, `Ctrl+X` cuts it, and `Ctrl+V` pastes it back in the same place. Walls keep their doors and windows.
- Then press `M` or `G` to move the pasted copy where it belongs.
- The copy is text, so you can paste it into a second CreaBIM window.

`Shift+D` (duplicate) copies and starts moving the copy in one go. In Move, `Ctrl` leaves a copy behind at each click.

## Delete

`Del` deletes what is selected. Things that belong to it go too: a wall's doors and windows, an element's tags and attached dimensions. The *Delete* button in *Properties* counts them ("Delete (and 2 attached)"). `Ctrl+Z` brings everything back.

## Hide and show

To get something out of the way for a moment:

- `H` hides the selection, in 3D and the 2D views.
- `Shift+H` shows only the selection.
- `Alt+H` shows everything again.

This is for working only: sheets and exports still show everything, and hidden elements come back when you reopen the project. To leave things out of a view for good, use [visibility](visibility).
