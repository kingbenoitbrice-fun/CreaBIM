// CreaBIM interface pages. The app sends each pane its state with
// CB.state({...}); the page draws it and posts what the user does back as
// JSON with window.ipc.postMessage.
(function () {
  "use strict";

  const pane = document.documentElement.dataset.pane;
  const root = document.getElementById("root");
  let S = null; // the last state
  const local = { open: {}, filter: "", stack: [] };

  function post(msg) {
    window.ipc.postMessage(JSON.stringify(msg));
  }

  function icon(name, cls) {
    const body = window.ICONS[name] || "";
    return `<svg class="i ${cls || ""}" viewBox="0 0 24 24">${body}</svg>`;
  }

  function esc(s) {
    return String(s == null ? "" : s).replace(/[&<>"']/g, (c) => ({
      "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
    }[c]));
  }

  function el(html) {
    const t = document.createElement("template");
    t.innerHTML = html.trim();
    return t.content.firstChild;
  }

  // Keep a text box being typed in when the state redraws.
  function keepFocus(draw) {
    const a = document.activeElement;
    const key = a && a.dataset ? a.dataset.key : null;
    const pos = a && a.selectionStart;
    const val = a && a.value;
    draw();
    if (key) {
      const b = root.querySelector(`[data-key="${CSS.escape(key)}"]`);
      if (b) {
        b.focus();
        if (b.value !== val && b.dataset.live !== "1") b.value = val;
        try { b.setSelectionRange(pos, pos); } catch (e) { /* not a text box */ }
      }
    }
  }

  // ---- controls shared by the top bar and the status bar ----

  function control(c) {
    switch (c.kind) {
      case "number": {
        // a length comes formatted in the project's units, and what is
        // typed in it goes back as text for the app to read
        const text = c.text != null;
        return `<label class="field" title="${esc(c.hint)}"><span class="k" data-scrub="${esc(c.key)}" data-step="${c.step}" data-min="${c.min}" data-max="${c.max}">${esc(c.label)}</span>` +
          `<input class="${text ? "len" : ""}" data-key="${esc(c.key)}" data-step="${c.step}" data-min="${c.min}" data-max="${c.max}" data-v="${c.value}" data-text="${text ? 1 : 0}" value="${text ? esc(c.text) : fmt(c.value, c.step)}">` +
          (c.unit ? `<span class="u">${esc(c.unit)}</span>` : "") + `</label>`;
      }
      case "menu":
        return `<button class="dd ${c.big ? "big" : ""} ${S.popup === c.key ? "open" : ""}" data-popup="${esc(c.key)}" title="${esc(c.hint)}" ${c.enabled === false ? "disabled" : ""}>` +
          (c.icon ? icon(c.icon) : "") + (c.label ? `<span class="k">${esc(c.label)}</span>` : "") +
          `<span class="v">${esc(c.value)}</span>${icon("chevron-down", "chev")}</button>`;
      case "button":
        return `<button class="btn" data-act="${esc(c.id)}" title="${esc(c.hint)}">${c.icon ? icon(c.icon) : ""}${esc(c.label)}</button>`;
      case "toggle":
        return `<button class="toggle ${c.on ? "on" : ""}" data-act="${esc(c.id)}" title="${esc(c.hint)}">${c.icon ? icon(c.icon) : ""}${esc(c.label)}</button>`;
      case "label":
        return `<span class="ctl-label">${esc(c.label)}</span>`;
      case "seg":
        return `<span class="seg">` + c.items.map((i) => `<button class="${i.on ? "on" : ""}" data-act="${esc(i.id)}" title="${esc(i.hint)}">${esc(i.label)}</button>`).join("") + `</span>`;
      case "sep":
        return `<span class="vsep"></span>`;
      case "meta":
        return `<span class="meta">${esc(c.label)}</span>`;
    }
    return "";
  }

  function fmt(v, step) {
    const d = step >= 1 ? 0 : step >= 0.1 ? 1 : 2;
    return Number(v).toFixed(d);
  }

  function setNumber(input, v) {
    const min = parseFloat(input.dataset.min), max = parseFloat(input.dataset.max);
    if (!isNaN(min)) v = Math.max(min, v);
    if (!isNaN(max)) v = Math.min(max, v);
    input.dataset.v = v;
    if (input.dataset.text !== "1") input.value = fmt(v, parseFloat(input.dataset.step));
    post({ t: "set", key: input.dataset.key, value: v });
  }

  // ---- top pane ----

  function drawTop() {
    const menus = S.menus.map((m) => `<button class="menubtn ${S.popup === m.key ? "open" : ""}" data-popup="${esc(m.key)}" data-menubar="1">${esc(m.name)}</button>`).join("");
    const quick = S.quick.map((q) => q === null ? `<span class="qsep"></span>` :
      `<button class="ibtn" data-act="${esc(q.id)}" title="${esc(q.label)}${q.key ? "  (" + esc(q.key) + ")" : ""}" ${q.enabled ? "" : "disabled"}>${icon(q.icon)}</button>`).join("");
    let tools = "";
    let group = null;
    for (const t of S.tools) {
      if (group !== null && t.group !== group) tools += `<span class="vsep"></span>`;
      group = t.group;
      tools += `<button class="tool ${t.active ? "on" : ""}" data-act="${esc(t.id)}" title="${esc(t.label + ": " + t.hint)}">${icon(t.icon)}<span>${esc(t.label)}</span></button>`;
    }
    root.innerHTML =
      `<div class="bar1"><div class="brand"><div class="logo">${icon("box")}</div>CreaBIM</div>${menus}<span class="vsep"></span>` +
      `<div class="quick">${quick}</div><span class="spacer"></span><div class="doc" title="${esc(S.doc.path)}"><b>${esc(S.doc.name)}</b>${S.doc.note ? " · " + esc(S.doc.note) : ""}</div><span class="spacer"></span>${S.right.map(control).join("")}</div>` +
      `<div class="bar2"><div class="tools">${tools}</div><span class="spacer"></span></div>` +
      // the tool options strip: the tool in use and what it can be set to,
      // then the active view's own settings at the right
      `<div class="bar3"><div class="toolname">${S.tool.icon ? icon(S.tool.icon) : ""}<b>${esc(S.tool.label)}</b>` +
      (S.tool.help ? `<button class="qhelp" data-act="help:${esc(S.tool.help)}" title="Help for the ${esc(S.tool.label)} tool (F1)">${icon("circle-help")}</button>` : "") + `</div>` +
      (S.tool_options.length ? `<div class="options">${S.tool_options.map(control).join("")}</div>` :
        `<div class="tool-hint" title="${esc(S.tool.hint)}">${esc(S.tool.hint || "")}</div>`) +
      `<span class="spacer"></span><div class="options">${S.options.map(control).join("")}</div></div>`;
  }

  // ---- left pane: project browser ----

  function drawLeft() {
    const f = local.filter.trim().toLowerCase();
    let html = `<div class="browser"><div class="browser-head"><div class="title">${icon("layout-dashboard")}Project<span class="count">${esc(S.count)}</span></div>` +
      `<label class="search">${icon("search")}<input data-key="filter" data-live="1" placeholder="Find views and elements" value="${esc(local.filter)}"></label></div><div class="tree">`;
    let any = false;
    for (const g of S.groups) {
      const rows = f ? g.rows.filter((r) => r.name.toLowerCase().includes(f)) : g.rows;
      if (f && rows.length === 0) continue;
      any = true;
      const open = f ? true : (local.open[g.key] !== undefined ? local.open[g.key] : g.open);
      const acts = (g.acts || []).map((a) => `<button class="gact" data-act="${esc(a.id)}" title="${esc(a.hint)}">${icon(a.icon)}</button>`).join("");
      const note = g.note ? `<span class="gnote">${esc(g.note)}</span>` : "";
      html += `<div class="group ${open ? "open" : ""}" data-group="${esc(g.key)}">${icon("chevron-right", "chev")}${icon(g.icon)}<span>${esc(g.title)}</span>${note}${acts}<span class="n">${g.rows.length}</span></div>`;
      if (open) {
        // rows of a sortable group can be dragged to another place in it
        const sort = g.sortable && !f ? ` data-sort="${esc(g.key)}"` : "";
        for (const r of rows) {
          html += `<div class="row ${r.sel ? "sel" : ""}" data-id="${r.id}"${sort} title="${esc(r.hint || r.name)}${sort ? " · drag to reorder" : ""}">${icon(r.icon)}<span>${esc(r.name)}</span>${r.active ? `<span class="dot"></span>` : ""}</div>`;
        }
      }
    }
    if (!any) html += `<div class="empty">Nothing matches “${esc(local.filter)}”.</div>`;
    html += `</div></div>`;
    const tree = root.querySelector(".tree");
    const scroll = tree ? tree.scrollTop : 0;
    keepFocus(() => { root.innerHTML = html; });
    const t2 = root.querySelector(".tree");
    if (t2) t2.scrollTop = scroll;
    showDrag();
  }

  // ---- dragging a view to another place in its group ----

  // The row being dragged and where it would land: {id, group, y, on, over, after}.
  function showDrag() {
    const d = local.drag;
    root.querySelectorAll(".row.dragging, .row.drop-before, .row.drop-after").forEach((r) => r.classList.remove("dragging", "drop-before", "drop-after"));
    if (!d || !d.on) return;
    const me = root.querySelector(`.row[data-id="${CSS.escape(d.id)}"]`);
    if (me) me.classList.add("dragging");
    const over = d.over && root.querySelector(`.row[data-id="${CSS.escape(d.over)}"]`);
    if (over) over.classList.add(d.after ? "drop-after" : "drop-before");
  }

  function dragMove(e) {
    const d = local.drag;
    if (!d) return;
    if (!d.on && Math.abs(e.clientY - d.y) < 5) return;
    d.on = true;
    const el = document.elementFromPoint(e.clientX, e.clientY);
    const row = el && el.closest(".row");
    if (row && row.dataset.sort === d.group) {
      const r = row.getBoundingClientRect();
      d.over = row.dataset.id;
      d.after = e.clientY > r.top + r.height / 2;
    }
    showDrag();
  }

  function dragEnd() {
    const d = local.drag;
    local.drag = null;
    document.removeEventListener("mousemove", dragMove);
    document.removeEventListener("mouseup", dragEnd);
    if (!d || !d.on) return showDrag();
    // a click follows the mouse-up on the same row: it isn't a pick
    local.dragged = true;
    setTimeout(() => { local.dragged = false; }, 0);
    showDrag();
    if (!d.over || d.over === d.id) return;
    const rows = [...root.querySelectorAll(`.row[data-sort="${CSS.escape(d.group)}"]`)].map((r) => r.dataset.id);
    let i = rows.indexOf(d.over) + (d.after ? 1 : 0);
    if (rows[i] === d.id) i += 1;
    post({ t: "reorder", id: d.id, before: rows[i] || null });
  }

  // ---- status pane ----

  function drawStatus() {
    root.innerHTML = `<div class="status"><div class="msg"><span class="pulse"></span><span title="${esc(S.message)}">${esc(S.message)}</span></div>` +
      S.controls.map(control).join("") + `</div>`;
  }

  // ---- the 3D view's navigation buttons ----

  function drawNav() {
    let html = `<div class="nav">`;
    for (const b of S.buttons) {
      if (!b) continue;
      html += `<button class="ibtn ${b.on ? "on" : ""}" data-act="${esc(b.id)}" title="${esc(b.hint)}" ${b.off ? "disabled" : ""}>${icon(b.icon)}</button>`;
    }
    // dropdowns: what the 3D view shows
    for (const m of S.menus || []) {
      const label = m.label ? `<span class="v">${esc(m.label)}</span>` : "";
      html += `<span class="qsep"></span><button class="dd ${m.on ? "on" : ""} ${S.popup === m.key ? "open" : ""}" data-popup="${esc(m.key)}" title="${esc(m.hint)}">${icon(m.icon)}${label}${icon("chevron-down", "chev")}</button>`;
    }
    html += `<span class="qsep"></span><button class="dd ${S.popup === S.look.key ? "open" : ""}" data-popup="${esc(S.look.key)}" title="${esc(S.look.hint)}">${icon("axis-3d")}<span class="v">View</span>${icon("chevron-down", "chev")}</button></div>`;
    root.innerHTML = html;
  }

  // ---- command palette ----

  function drawPalette() {
    if (local.key !== S.key) {
      local.key = S.key;
      local.query = "";
      local.hot = 0;
    }
    const words = (local.query || "").toLowerCase().split(/\s+/).filter(Boolean);
    const found = S.items.filter((i) => i.id && words.every((w) => i.label.toLowerCase().includes(w)));
    local.found = found;
    local.hot = Math.max(0, Math.min(local.hot || 0, found.length - 1));
    let html = `<div class="palette"><label class="pq">${icon("search")}<input data-key="palette" data-live="1" placeholder="Type a command, a tool or a view" value="${esc(local.query || "")}"></label><div class="pl">`;
    found.slice(0, 60).forEach((i, n) => {
      html += `<button class="mi ${n === local.hot ? "hot" : ""}" data-act="${esc(i.id)}" data-n="${n}" title="${esc(i.hint)}"><span class="ck">${i.icon ? icon(i.icon) : ""}</span><span class="label">${esc(i.label)}</span>${i.key ? `<span class="key">${esc(i.key)}</span>` : ""}</button>`;
    });
    if (found.length === 0) html += `<div class="empty">Nothing matches.</div>`;
    html += `</div><div class="pfoot"><span>↑↓ choose</span><span>Enter run</span><span>Esc close</span></div></div>`;
    keepFocus(() => { root.innerHTML = html; });
    const input = root.querySelector("input[data-key=palette]");
    if (input && document.activeElement !== input) { input.focus(); input.setSelectionRange(input.value.length, input.value.length); }
    const hot = root.querySelector(".mi.hot");
    if (hot) hot.scrollIntoView({ block: "nearest" });
    post({ t: "size", w: window.innerWidth, h: Math.ceil(root.querySelector(".palette").scrollHeight) + 2 });
  }

  // ---- popup menus ----

  function drawPopup() {
    if (S.key === "palette" && S.items && S.items.length) return drawPalette();
    if (!S.items || S.items.length === 0) {
      root.innerHTML = "";
      local.stack = [];
      local.key = null;
      return;
    }
    if (local.key !== S.key) {
      local.key = S.key;
      local.stack = [];
      local.hot = -1;
    }
    // Follow the open submenu into fresh items, by label.
    let items = S.items;
    let title = null;
    for (const label of local.stack) {
      const sub = items.find((i) => i.items && i.label === label);
      if (!sub) { local.stack = []; items = S.items; title = null; break; }
      items = sub.items;
      title = label;
    }
    const row = (i) => {
      if (i.sep) return `<div class="msep"></div>`;
      if (i.header) return `<div class="mhead">${esc(i.header)}</div>`;
      if (i.items) return `<button class="mi" data-sub="${esc(i.label)}" ${i.enabled ? "" : "disabled"}><span class="ck">${i.icon ? icon(i.icon) : ""}</span><span class="label">${esc(i.label)}</span>${icon("chevron-right", "sub")}</button>`;
      return `<button class="mi" data-act="${esc(i.id)}" data-keep="${i.keep ? 1 : 0}" title="${esc(i.hint)}" ${i.enabled ? "" : "disabled"}>` +
        `<span class="ck">${i.checked ? icon("check") : (i.icon && i.checked !== false ? icon(i.icon) : "")}</span>` +
        (i.icon && i.checked != null ? `<span class="glyph">${icon(i.icon)}</span>` : "") + `<span class="label">${esc(i.label)}</span>${i.key ? `<span class="key">${esc(i.key)}</span>` : ""}</button>`;
    };
    const cols = S.columns > 1 && title === null ? columns(items, S.columns) : null;
    let html = `<div class="mwrap"><div class="menu ${cols ? "cols" : ""}" ${cols ? `style="--cols:${cols.length}"` : ""}>`;
    if (title !== null) html += `<button class="head" data-back="1">${icon("arrow-left")}<span>${esc(title)}</span></button><div class="msep"></div>`;
    if (cols) {
      // a long menu with sections reads as a panel of columns
      for (const col of cols) html += `<div class="col">${col.map(row).join("")}</div>`;
    } else {
      html += items.map(row).join("");
    }
    html += `</div><div class="more">${icon("chevron-down")}</div></div>`;
    root.innerHTML = html;
    if (local.hot >= 0) {
      const b = root.querySelectorAll(".menu .mi:not(:disabled), .menu .head")[local.hot];
      if (b) b.classList.add("hot");
    }
    const m = root.querySelector(".menu");
    m.addEventListener("scroll", showMore);
    local.sentH = local.sentW = null;
    reportSize();
    showMore();
  }

  // Split a menu at its headers into `n` columns of about equal height,
  // keeping the sections in order.
  function columns(items, n) {
    const sections = [];
    for (const i of items) {
      if (i.header || sections.length === 0) sections.push([]);
      if (!i.sep) sections[sections.length - 1].push(i);
    }
    n = Math.min(n, sections.length);
    const h = sections.map((s) => s.length);
    let best = null;
    // every way to cut the sections into n runs (menus are short)
    (function cut(from, left, acc) {
      if (left === 1) {
        const runs = acc.concat([[from, sections.length]]);
        const tall = Math.max(...runs.map(([a, b]) => h.slice(a, b).reduce((x, y) => x + y, 0)));
        if (!best || tall < best.tall) best = { tall, runs };
        return;
      }
      for (let k = from + 1; k <= sections.length - left + 1; k++) cut(k, left - 1, acc.concat([[from, k]]));
    })(0, n, []);
    return best.runs.map(([a, b]) => [].concat(...sections.slice(a, b)));
  }

  // A shadow and an arrow at the bottom while more of the menu is below.
  function showMore() {
    const m = root.querySelector(".menu");
    const more = root.querySelector(".more");
    if (!m || !more) return;
    more.classList.toggle("on", m.scrollTop + m.clientHeight < m.scrollHeight - 2);
  }

  // the page's size changes as the app places it: measure again then
  window.addEventListener("resize", () => {
    if (pane !== "popup" || !root.querySelector(".menu")) return;
    reportSize();
    showMore();
  });

  // The size the menu wants: its content's own width and height, measured
  // apart from the page it is shown in. (Measuring it as placed fed the
  // page's width back into the menu's, so a menu with a scrollbar grew
  // wider every time it was placed.)
  function naturalSize(m) {
    const keep = [m.style.minWidth, m.style.width, m.style.maxHeight, m.style.overflowY];
    m.style.minWidth = "0";
    m.style.width = "max-content";
    m.style.maxHeight = "none";
    m.style.overflowY = "hidden";
    const r = m.getBoundingClientRect();
    [m.style.minWidth, m.style.width, m.style.maxHeight, m.style.overflowY] = keep;
    return { w: Math.ceil(r.width), h: Math.ceil(r.height) };
  }

  function reportSize() {
    const m = root.querySelector(".menu");
    if (!m) return;
    const { w, h } = naturalSize(m);
    // taller than the page it was given: it scrolls, and the scrollbar
    // needs room beside the items
    const bar = h > window.innerHeight - 2 + 1 ? 10 : 0;
    const want = { w: w + 2 + bar, h: h + 2 };
    if (want.w === local.sentW && want.h === local.sentH) return;
    local.sentW = want.w;
    local.sentH = want.h;
    post({ t: "size", w: want.w, h: want.h });
  }

  // Arrow keys, Enter and Esc in an open menu, from this page or passed on
  // from the views.
  function menuKey(k) {
    const buttons = [...root.querySelectorAll(".menu .mi:not(:disabled), .menu .head")];
    if (buttons.length === 0) return;
    if (k === "ArrowDown" || k === "ArrowUp") {
      const n = buttons.length;
      local.hot = local.hot < 0 ? (k === "ArrowDown" ? 0 : n - 1) : (local.hot + (k === "ArrowDown" ? 1 : -1) + n) % n;
      buttons.forEach((b, i) => b.classList.toggle("hot", i === local.hot));
      buttons[local.hot].scrollIntoView({ block: "nearest" });
      showMore();
    } else if (k === "Enter" || k === "ArrowRight") {
      const b = buttons[local.hot];
      if (b && (k === "Enter" || b.dataset.sub)) b.click();
    } else if (k === "ArrowLeft" || k === "Backspace") {
      if (local.stack.length) { local.stack.pop(); local.hot = -1; drawPopup(); }
    } else if (k === "Escape") {
      post({ t: "close" });
    }
  }

  // ---- the Help panel: the guide's pages, written in Markdown ----

  // Every page, from the app: {section, id, title, md}. The shortcuts page
  // is made by the app from its menus and arrives with the state.
  function helpPages() {
    const gen = (S && S.generated) || {};
    return (window.HELP || []).map((p) => gen[p.id] ? Object.assign({}, p, { md: gen[p.id] }) : p);
  }

  // A heading's id, as the app's help::slug makes it.
  function slug(s) {
    let out = "";
    for (const c of s.toLowerCase()) {
      if (/[\p{L}\p{N}]/u.test(c)) out += c;
      else if (out && !out.endsWith("-")) out += "-";
    }
    return out.replace(/-+$/, "");
  }

  function mdLink(text, href) {
    if (/^https?:/.test(href)) return `${text} <span class="url">(${href})</span>`;
    return `<a data-act="help:${href}">${text}</a>`;
  }

  function mdImage(alt, src) {
    const url = S && S.images && S.images[src];
    return url ? `<figure><img src="${url}" alt="${alt}"><figcaption>${alt}</figcaption></figure>` : "";
  }

  // Inline marks, on text already escaped.
  function inline(s) {
    let out = esc(s);
    out = out.replace(/`([^`]+)`/g, (m, c) => `<kbd>${c}</kbd>`);
    out = out.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, (m, alt, src) => mdImage(alt, src));
    out = out.replace(/\[([^\]]+)\]\(([^)]+)\)/g, (m, t, href) => mdLink(t, href));
    out = out.replace(/\*\*([^*]+)\*\*/g, "<b>$1</b>");
    out = out.replace(/\*([^*]+)\*/g, "<i>$1</i>");
    return out;
  }

  const LI = /^(\s*)([-*]|\d+\.)\s+(.*)$/;

  // A list from line i, nested by indent. Returns [html, next line].
  function mdList(lines, i) {
    const first = lines[i].match(LI);
    const indent = first[1].length;
    const tag = /\d/.test(first[2]) ? "ol" : "ul";
    let html = `<${tag}>`;
    let item = null;
    const close = () => { if (item) html += `<li>${inline(item.text)}${item.sub}</li>`; item = null; };
    while (i < lines.length) {
      const l = lines[i];
      if (/^\s*$/.test(l)) {
        const next = lines[i + 1] && lines[i + 1].match(LI);
        if (next && next[1].length >= indent) { i++; continue; }
        break;
      }
      const m = l.match(LI);
      if (m) {
        if (m[1].length < indent) break;
        if (m[1].length > indent && item) {
          const [sub, j] = mdList(lines, i);
          item.sub += sub;
          i = j;
          continue;
        }
        close();
        item = { text: m[3], sub: "" };
        i++;
        continue;
      }
      if (item && /^\s+\S/.test(l)) { item.text += " " + l.trim(); i++; continue; }
      break;
    }
    close();
    return [html + `</${tag}>`, i];
  }

  function cells(l) {
    return l.trim().replace(/^\|/, "").replace(/\|$/, "").split("|").map((c) => c.trim());
  }

  function markdown(text) {
    const lines = text.replace(/\r/g, "").split("\n");
    let html = "";
    let para = [];
    const flush = () => { if (para.length) html += `<p>${inline(para.join(" "))}</p>`; para = []; };
    let i = 0;
    while (i < lines.length) {
      const l = lines[i];
      let m;
      if (/^\s*$/.test(l)) { flush(); i++; continue; }
      if (/^<!--\s*upcoming\b.*-->\s*$/.test(l)) { flush(); html += `<div class="soon"><div class="soon-tag">Coming soon</div>`; i++; continue; }
      if (/^<!--\s*\/upcoming\s*-->\s*$/.test(l)) { flush(); html += `</div>`; i++; continue; }
      if (/^<!--.*-->\s*$/.test(l)) { i++; continue; }
      if ((m = l.match(/^(#{1,4})\s+(.*)$/))) {
        flush();
        const n = m[1].length;
        html += `<h${n} id="h-${slug(m[2])}">${inline(m[2])}</h${n}>`;
        i++;
        continue;
      }
      if (/^!\[[^\]]*\]\([^)]+\)\s*$/.test(l)) { flush(); html += inline(l.trim()); i++; continue; }
      if (/^>/.test(l)) {
        flush();
        const inner = [];
        while (i < lines.length && /^>/.test(lines[i])) inner.push(lines[i++].replace(/^>\s?/, ""));
        html += `<div class="tip">${markdown(inner.join("\n"))}</div>`;
        continue;
      }
      if (LI.test(l)) { flush(); const [h, j] = mdList(lines, i); html += h; i = j; continue; }
      if (/^\|/.test(l)) {
        flush();
        const rows = [];
        while (i < lines.length && /^\|/.test(lines[i])) rows.push(lines[i++]);
        const head = rows.length > 1 && /^[\s|:-]+$/.test(rows[1]) ? cells(rows[0]) : null;
        const body = head ? rows.slice(2) : rows;
        html += `<table>` + (head ? `<thead><tr>${head.map((c) => `<th>${inline(c)}</th>`).join("")}</tr></thead>` : "") +
          `<tbody>${body.map((r) => `<tr>${cells(r).map((c) => `<td>${inline(c)}</td>`).join("")}</tr>`).join("")}</tbody></table>`;
        continue;
      }
      para.push(l.trim());
      i++;
    }
    flush();
    return html;
  }

  // A page's words without the marks (or its title), for searching.
  function plain(md) {
    return md.replace(/^# .*$/m, " ").replace(/<!--.*?-->/g, " ").replace(/!\[[^\]]*\]\([^)]*\)/g, " ")
      .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1").replace(/[#*`>|]/g, " ").replace(/\s+/g, " ").trim();
  }

  function search(q) {
    const words = q.toLowerCase().split(/\s+/).filter(Boolean);
    const out = [];
    for (const p of helpPages()) {
      const text = plain(p.md);
      const low = text.toLowerCase();
      const title = p.title.toLowerCase();
      if (!words.every((w) => low.includes(w) || title.includes(w))) continue;
      let score = 0;
      for (const w of words) {
        if (title.includes(w)) score += 10;
        score += Math.min(5, low.split(w).length - 1);
      }
      // a few words round the first hit
      const at = Math.max(0, low.indexOf(words[0]));
      const from = Math.max(0, at - 50);
      let snip = esc((from ? "…" : "") + text.slice(from, at + 110) + "…");
      for (const w of words) snip = snip.replace(new RegExp(w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "gi"), (m) => `<mark>${m}</mark>`);
      out.push({ p, score, snip });
    }
    return out.sort((a, b) => b.score - a.score);
  }

  function drawHelp() {
    // a page opened from elsewhere (a menu, F1) ends the search
    const nav = S.page + "#" + (S.anchor || "");
    if (local.hnav !== undefined && local.hnav !== nav) local.hq = "";
    local.hnav = nav;
    const pages = helpPages();
    const q = local.hq || "";
    const page = pages.find((p) => p.id === S.page);
    let body = "";
    if (q.trim()) {
      const found = search(q);
      local.hfound = found;
      body = `<div class="hres-n">${found.length ? found.length + (found.length === 1 ? " page" : " pages") : "Nothing found"} for “${esc(q)}”</div>` +
        found.map((f) => `<button class="hres" data-act="help:${esc(f.p.id)}"><b>${esc(f.p.title)}</b><span class="hsec">${esc(f.p.section)}</span><span class="snip">${f.snip}</span></button>`).join("");
    } else if (!page) {
      // the contents: every page by section
      body = `<article class="hdoc"><h1>Help</h1><p>Pick a page, or search the guide above. <kbd>F1</kbd> opens the page about the tool you are using.</p>`;
      let section = null;
      for (const p of pages) {
        if (p.section !== section) {
          if (section !== null) body += `</div>`;
          section = p.section;
          body += `<h3>${esc(section)}</h3><div class="toc">`;
        }
        body += `<a data-act="help:${esc(p.id)}">${esc(p.title)}</a>`;
      }
      body += `</div></article>`;
    } else {
      const i = pages.indexOf(page);
      const prev = pages[i - 1], next = pages[i + 1];
      body = `<div class="crumb"><a data-act="help:contents">Contents</a> › ${esc(page.section)}</div><article class="hdoc">${markdown(page.md)}</article>` +
        `<div class="pn">${prev ? `<a data-act="help:${esc(prev.id)}">← ${esc(prev.title)}</a>` : "<span></span>"}` +
        `${next ? `<a data-act="help:${esc(next.id)}">${esc(next.title)} →</a>` : ""}</div>`;
    }
    const where = (q.trim() ? "?" : "") + S.page + "#" + (S.anchor || "");
    const scroller = root.querySelector(".help-body");
    const keep = scroller && local.hwhere === where ? scroller.scrollTop : null;
    root.innerHTML = `<div class="help"><div class="help-head">` +
      `<button class="ibtn" data-act="help.back" title="Back" ${S.back ? "" : "disabled"}>${icon("arrow-left")}</button>` +
      `<button class="ibtn ${page || q ? "" : "on"}" data-act="help:contents" title="Contents">${icon("book-open")}</button>` +
      `<label class="search">${icon("search")}<input data-key="helpq" data-live="1" placeholder="Search the guide" value="${esc(q)}"></label>` +
      `<button class="ibtn" data-act="help.close" title="Close (F1 or Esc)">${icon("x")}</button></div>` +
      `<div class="help-body">${body}</div></div>`;
    const b = root.querySelector(".help-body");
    if (keep !== null) b.scrollTop = keep;
    else {
      const h = S.anchor && !q.trim() && root.querySelector(`#h-${CSS.escape(S.anchor)}`);
      if (h) h.scrollIntoView({ block: "start" });
      else b.scrollTop = 0;
    }
    local.hwhere = where;
  }

  // ---- drawing and events ----

  const draw = { top: drawTop, left: drawLeft, status: drawStatus, popup: drawPopup, nav: drawNav, help: drawHelp }[pane];

  // Keys typed in the views while the palette is open.
  function paletteKey(k) {
    const found = local.found || [];
    if (k === "ArrowDown" || k === "ArrowUp") {
      local.hot = (local.hot + (k === "ArrowDown" ? 1 : -1) + found.length) % Math.max(found.length, 1);
    } else if (k === "Enter") {
      const i = found[local.hot];
      post(i ? { t: "act", id: i.id, keep: false } : { t: "close" });
      return false;
    } else if (k === "Escape") {
      post({ t: "close" });
      return false;
    } else if (k === "Backspace") {
      local.query = (local.query || "").slice(0, -1);
      local.hot = 0;
    }
    return true;
  }

  window.CB = {
    state(s) {
      S = s;
      if (s.theme) document.documentElement.dataset.theme = s.theme;
      keepFocus(draw);
    },
    nav(k) {
      if (S && S.key !== "palette") menuKey(k);
    },
    feed(f) {
      if (!S || S.key !== "palette") return;
      if (f.text) { local.query = (local.query || "") + f.text; local.hot = 0; }
      for (const k of f.keys) if (!paletteKey(k)) return;
      drawPalette();
    },
  };

  function focusViews(e) {
    // Give the keyboard back to the views unless a text box was clicked.
    const t = e.target;
    if (!(t instanceof HTMLInputElement)) post({ t: "focus" });
  }

  document.addEventListener("mousedown", (e) => {
    post({ t: "down" });
    const g = e.target.closest("[data-scrub]");
    if (g) startScrub(e, g);
    const row = e.button === 0 && e.target.closest(".row[data-sort]");
    if (row) {
      local.drag = { id: row.dataset.id, group: row.dataset.sort, y: e.clientY, on: false };
      document.addEventListener("mousemove", dragMove);
      document.addEventListener("mouseup", dragEnd);
    }
  });

  document.addEventListener("click", (e) => {
    if (local.dragged) return;
    const t = e.target.closest("button, .row, .group, a[data-act]");
    if (!t || t.disabled) return focusViews(e);
    if (t.dataset.popup) {
      const r = t.getBoundingClientRect();
      if (S.popup === t.dataset.popup) post({ t: "close" });
      else post({ t: "popup", key: t.dataset.popup, x: r.left, y: r.bottom + 4, w: r.width, up: pane === "status" ? r.top - 4 : null, top: r.top - 4 });
    } else if (t.dataset.back) {
      local.stack.pop();
      local.hot = -1;
      drawPopup();
      return;
    } else if (t.dataset.sub) {
      local.stack.push(t.dataset.sub);
      local.hot = -1;
      drawPopup();
      root.querySelector(".menu").scrollTop = 0;
      return;
    } else if (t.dataset.act) {
      // a page picked from the search results ends the search
      if (pane === "help" && t.dataset.act.startsWith("help:") && local.hq) {
        local.hq = "";
        drawHelp();
      }
      post({ t: "act", id: t.dataset.act, keep: t.dataset.keep === "1" });
    } else if (t.dataset.group) {
      const g = S.groups.find((x) => x.key === t.dataset.group);
      const was = local.open[g.key] !== undefined ? local.open[g.key] : g.open;
      local.open[g.key] = !was;
      drawLeft();
    } else if (t.dataset.id) {
      post({ t: "pick", id: t.dataset.id, add: e.shiftKey || e.ctrlKey || e.metaKey });
    }
    focusViews(e);
  });

  // Slide between menus while one is open, like a menu bar.
  document.addEventListener("mouseover", (e) => {
    const t = e.target.closest("[data-menubar]");
    if (!t || !S || !S.popup || !S.popup.startsWith("menu:") || S.popup === t.dataset.popup) return;
    const r = t.getBoundingClientRect();
    post({ t: "popup", key: t.dataset.popup, x: r.left, y: r.bottom + 4, w: r.width });
  });

  document.addEventListener("input", (e) => {
    const t = e.target;
    if (t.dataset.key === "filter") {
      local.filter = t.value;
      drawLeft();
    } else if (t.dataset.key === "palette") {
      local.query = t.value;
      local.hot = 0;
      drawPalette();
    } else if (t.dataset.key === "helpq") {
      local.hq = t.value;
      keepFocus(drawHelp);
    }
  });

  document.addEventListener("change", (e) => {
    const t = e.target;
    if (t.dataset.text === "1") {
      post({ t: "set", key: t.dataset.key, text: t.value });
    } else if (t.dataset.key && !["filter", "palette", "helpq"].includes(t.dataset.key)) {
      const v = parseFloat(t.value.replace(",", "."));
      if (!isNaN(v)) setNumber(t, v);
    }
  });

  document.addEventListener("keydown", (e) => {
    const t = e.target;
    if (t instanceof HTMLInputElement && t.dataset.key === "palette") {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        paletteKey(e.key);
        drawPalette();
      } else if (e.key === "Enter" || e.key === "Escape") {
        paletteKey(e.key);
        post({ t: "focus" });
      }
      return;
    }
    if (t instanceof HTMLInputElement && t.dataset.key === "helpq") {
      // Esc clears the search (and then closes), Enter opens the best page
      if (e.key === "Escape") {
        if (t.value) { local.hq = ""; keepFocus(drawHelp); } else post({ t: "act", id: "help.close" });
      } else if (e.key === "Enter" && local.hfound && local.hfound.length) {
        const id = local.hfound[0].p.id;
        local.hq = "";
        post({ t: "act", id: "help:" + id });
      } else if (e.key === "F1") {
        post({ t: "act", id: "help.close" });
      }
      return;
    }
    if (pane === "help" && e.key === "Escape") {
      post({ t: "act", id: "help.close" });
      return;
    }
    if (t instanceof HTMLInputElement) {
      if (e.key === "Enter") { t.blur(); post({ t: "focus" }); }
      else if (e.key === "Escape") { t.value = t.defaultValue; t.blur(); post({ t: "focus" }); }
      else if ((e.key === "ArrowUp" || e.key === "ArrowDown") && t.dataset.step) {
        e.preventDefault();
        const step = parseFloat(t.dataset.step) * (e.shiftKey ? 10 : 1);
        setNumber(t, value(t) + (e.key === "ArrowUp" ? step : -step));
      }
      return;
    }
    if (pane === "popup" && ["ArrowDown", "ArrowUp", "ArrowLeft", "ArrowRight", "Enter", "Escape"].includes(e.key)) {
      e.preventDefault();
      menuKey(e.key);
      return;
    }
    // Anything else is a shortcut for the views.
    post({ t: "key", key: e.key, code: e.code, ctrl: e.ctrlKey || e.metaKey, shift: e.shiftKey, alt: e.altKey, repeat: e.repeat });
    if (e.ctrlKey || e.metaKey || e.key === "F1" || e.key === "F3" || e.key === "Tab") e.preventDefault();
  });

  // A number field's value: lengths keep theirs (metres) beside the text.
  function value(input) {
    return input.dataset.text === "1" ? parseFloat(input.dataset.v) : parseFloat(input.value);
  }

  // Drag a number's label sideways to change it.
  function startScrub(e, label) {
    const input = label.parentElement.querySelector("input");
    const step = parseFloat(label.dataset.step);
    const x0 = e.clientX;
    const v0 = value(input);
    let moved = false;
    e.preventDefault();
    function move(ev) {
      const dx = ev.clientX - x0;
      if (Math.abs(dx) > 2) moved = true;
      if (moved) setNumber(input, v0 + Math.round(dx / 4) * step * (ev.shiftKey ? 10 : 1));
    }
    function up() {
      document.removeEventListener("mousemove", move);
      document.removeEventListener("mouseup", up);
      if (!moved) { input.focus(); input.select(); }
    }
    document.addEventListener("mousemove", move);
    document.addEventListener("mouseup", up);
  }

  document.addEventListener("contextmenu", (e) => e.preventDefault());
  post({ t: "ready" });
})();
