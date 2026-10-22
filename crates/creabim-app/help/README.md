# The built-in guide

Each `.md` file here is a page of the Help panel (Help menu, F1). The files
are compiled into the app; `src/help.rs` lists them in order, by section, in
`PAGES`. To add a page, write the file and add a line there.

What the pages can use:

- `# Title` on the first line, `##` and `###` headings, paragraphs, `-` and
  `1.` lists (indent two spaces to nest), `| tables |`, `> **Tip:**` boxes,
  `**bold**` and `*italic*`.
- `` `Ctrl+K` `` is drawn as a key.
- `[Walls](walls)` links to a page by its file name, and
  `[joins](walls#joins)` to a heading on it (the heading in lower case, words
  joined by `-`).
- `![What it shows](img/name.jpg)` shows a picture. Add the file to `img/` and
  to `IMAGES` in `src/help.rs`. Keep pictures small: about 820 px wide, JPEG
  quality 72 (`convert in.png -resize '820x>' -strip -quality 72 out.jpg`).
- A part about a feature that hasn't arrived yet goes between
  `<!-- upcoming: what it waits for -->` and `<!-- /upcoming -->`. It shows as
  "Coming soon". Search for `upcoming:` when the feature lands.

`shortcuts.md` is only the start of its page: the tables are made from the
menus and from `KEYS` in `src/help.rs`. A key the code reads that isn't in a
menu must be added to `KEYS`, or the tests fail.

The tests (`cargo test -p creabim-app help`) check that every page is listed,
every link and picture exists, every tool has a page, and the shortcuts page
lists every key.
