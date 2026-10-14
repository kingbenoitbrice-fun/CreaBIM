# Interface pages

The menus, tool palette, project browser and status bar are web pages shown
in child webviews (wry, the webview layer Tauri uses) over the app window.
The 3D and 2D views stay native underneath. See `src/webui.rs` and
`src/shell.rs`.

- `app.css`, `app.js`: one stylesheet and one script for every pane;
  `<html data-pane>` picks the layout and `data-theme` the colours.
- `icons.js`: icons from [Lucide](https://lucide.dev) (ISC, see
  `LICENSE-lucide`), generated from the `lucide-static` package. To add one,
  put its name in the list and regenerate:

```sh
npm pack lucide-static@0.460.0 && tar xzf lucide-static-0.460.0.tgz
python3 make_icons.py package/icons > icons.js
```

There is no build step: the files are compiled into the app as they are.
Set `CREABIM_CLASSIC_UI=1` (or pick View > Classic panels) for the plain
egui panels.
