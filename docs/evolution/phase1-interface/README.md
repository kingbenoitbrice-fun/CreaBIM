# Phase 1: the new interface

Reference screenshots for the new interface, taken from the sample house.

The menus, tool palette, project browser, status bar and every dropdown and right-click menu are now web pages, drawn by the same webview Tauri uses (wry: WebView2 on Windows, WebKitGTK on Linux). They sit as child windows over the app window, and the 3D view and the 2D panes stay native underneath, so drawing and orbiting are as fast as before.

| Screenshot | What it shows |
| --- | --- |
| [01-overview-dark.png](01-overview-dark.png) | The whole window in the dark theme, with a wall selected. Top: the menus, then Undo, Redo and the edit buttons as icons, the file name and how many elements are selected, and the view picker. Below: the tool palette with icons, grouped (select, building elements, sections, annotation, sketching), and the active view's options. Left: the project browser. Bottom: the status bar with snaps, workplane and pivot. |
| [02-file-menu.png](02-file-menu.png) | The File menu: new, open, recent files, save, linked files and the exports, with icons and shortcuts. Moving the pointer to Edit or View while a menu is open switches to that menu. Submenus (New schedule, Open recent) open in place with a back arrow. |
| [03-view-picker.png](03-view-picker.png) | The view picker at the top right: every view and sheet, the open one ticked. |
| [04-right-click-menu.png](04-right-click-menu.png) | Right-click on a wall in 3D with the Select tool. The menu is the same as before, in the new look; it opens in plans and sections too. |
| [05-snaps.png](05-snaps.png) | The snap list opened from the status bar. Each kind can be ticked without the list closing; *Snapping* (F3) switches them all. |
| [06-wall-options.png](06-wall-options.png) | The Wall tool picked: its *Thickness* field appears with the view's *Cut height*. Drag a field's label sideways to change the value, or click it and type. |
| [07-browser-search.png](07-browser-search.png) | Typing "win" in the browser's search box keeps the matching views and elements. Clicking a row selects it; Shift or Ctrl adds to the selection; clicking a view or sheet opens it. |
| [08-light.png](08-light.png) | *View > Light*: the same interface in the light theme. The properties panel follows. |
| [09-classic-panels.png](09-classic-panels.png) | *View > Classic panels* goes back to the plain built-in panels (and the same menu brings the new ones back). The app also falls back to them by itself where the webview can't start. |

## How to try it

- Everything in the top bar, the browser and the status bar works with the mouse alone. Keyboard shortcuts still work after clicking in them: the keyboard goes back to the views.
- Pick the theme in *View > Dark / Light*.
- Set `CREABIM_CLASSIC_UI=1` to start with the classic panels.

## Known gaps

- The properties panel is still drawn by egui, restyled to match the new look. Moving it to the web interface comes next.
- The bars that appear inside the views (the slab tool's modes, the move/rotate/scale panel, the sketch tools) are still egui.
- On Linux the panes need X11, so the app runs through XWayland on Wayland desktops.
- Only tried under Linux here. The Windows build compiles in CI but hasn't been run on a Windows machine yet.
- The theme choice isn't remembered between runs.
- A plan opened in its own window keeps the old right-click menu.
- The project browser can't rename, reorder or hide elements yet.
- Typing a path into the *File* box is only in the classic panels; the new ones use the file picker.
