# Phase 1: Help and the user guide

These screenshots show the new Help panel and the guide inside it. The guide is written for people who are new to CreaBIM or to BIM. The shots were taken on the sample house.

| Screenshot | What it shows |
| --- | --- |
| [01-help-menu.png](01-help-menu.png) | The new **Help** menu has *Help for what you are doing* (F1), *Contents*, *Getting started*, *The window at a glance*, *Keyboard shortcuts*, *BIM words explained* and *About CreaBIM*. |
| [02-getting-started.png](02-getting-started.png) | The *Getting started* tour. It builds a first house in eight short steps: project, levels, walls, doors and windows, floor and roof, views, sheet, then save and export. |
| [03-f1-wall-tool.png](03-f1-wall-tool.png) | Pressing **F1** with the Wall tool active opens the *Walls* page, with a picture of the tool's options. F1 always opens the page for what you are doing: the edit in progress, then the active tool, then the selected element, otherwise the contents. Pressing F1 again closes the panel. |
| [04-contents.png](04-contents.png) | *Help › Contents* lists all 55 pages in eight sections: Getting started, Selecting and editing, Building the model, Views and drawings, Annotation, Drawing aids, Files and exchange, and Reference. |
| [05-search.png](05-search.png) | Searching the guide for "flip door". Results come up as you type. A page whose title matches comes first, then the pages that use the words most, and the matching words are highlighted. Enter opens the first result and Esc clears the search. |
| [06-search-result.png](06-search-result.png) | The *Doors and windows* page opened from that result. The trail at the top leads back to Contents, and Back returns to the previous page. Keys are drawn as key caps. |
| [07-shortcuts.png](07-shortcuts.png) | *Keyboard shortcuts*. This page is built when it opens from the app's own menus and key list, so it cannot go out of date. |
| [08-help-button.png](08-help-button.png) | The small **?** next to the tool's name in the tool options strip opens that tool's page, here *Selecting*. |

## How to try it

- Press **F1** anywhere, or use the **Help** menu. Pick a tool such as Slab and press F1 again: the *Slabs* page opens. Press F1 a second time to close it.
- Click the **?** beside the tool name in the options strip.
- Type in *Search the guide* at the top of the panel. You can also press **Ctrl+K**: every guide page appears in the command palette under its section.
- Links inside a page open other pages, or a heading on another page. Drag the panel's left edge to resize it; the width is remembered.
- With the classic panels, the guide opens in a plain window.

## For maintainers

- The pages are Markdown files in `crates/creabim-app/help/`, and `help/README.md` explains how to write them. The pages and pictures are built into the app. The pictures are JPEGs under 160 KB each, about 1.2 MB in total.
- Tests check that:
  - every link and picture on every page exists, including links to headings (`page#heading`);
  - every Help menu entry, palette entry, tool page and selected-element page exists;
  - every tool has a page;
  - the shortcuts page lists every key the app's code reads, and lists nothing the app does not read;
  - every menu shortcut appears on the shortcuts page.
- Content that is not merged yet goes between `<!-- upcoming: … -->` and `<!-- /upcoming -->`. The panel shows it as a "Coming soon" box. The *Furniture and fixtures* page is one of these until the object catalogue branch merges.

## Known gaps

- *Furniture and fixtures* is a placeholder until the catalogue work merges. Its page needs rewriting then.
- The pictures are whole-window screenshots, so they are small in a narrow panel. Widening the panel helps.
- Links to outside websites are shown as plain text, not as clickable links.
- The shortcuts page is grouped by where each key works (views, selecting, moving, drawing, menus). It does not yet list which keys belong to each separate tool.
- Esc closes the panel only when the panel has the keyboard. Otherwise Esc goes to the views, as before.
- In the classic panels the guide is plain text, without pictures or clickable links.
- Not yet checked on Windows: WebView2 limits a page's size, so each page's pictures are sent separately when the page opens, but this has only been run on Linux.
