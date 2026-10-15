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
      `<div class="bar2"><div class="tools">${tools}</div><span class="spacer"></span><div class="options">${S.options.map(control).join("")}</div></div>`;
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
      html += `<div class="group ${open ? "open" : ""}" data-group="${esc(g.key)}">${icon("chevron-right", "chev")}${icon(g.icon)}<span>${esc(g.title)}</span>${acts}<span class="n">${g.rows.length}</span></div>`;
      if (open) {
        for (const r of rows) {
          html += `<div class="row ${r.sel ? "sel" : ""}" data-id="${r.id}" title="${esc(r.hint || r.name)}">${icon(r.icon)}<span>${esc(r.name)}</span>${r.active ? `<span class="dot"></span>` : ""}</div>`;
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
      return;
    }
    if (local.key !== S.key) {
      local.key = S.key;
      local.stack = [];
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
    let html = `<div class="menu">`;
    if (title !== null) html += `<button class="head" data-back="1">${icon("arrow-left")}<span>${esc(title)}</span></button><div class="msep"></div>`;
    items.forEach((i, n) => {
      if (i.sep) html += `<div class="msep"></div>`;
      else if (i.header) html += `<div class="mhead">${esc(i.header)}</div>`;
      else if (i.items) html += `<button class="mi" data-sub="${esc(i.label)}" ${i.enabled ? "" : "disabled"}><span class="ck"></span><span class="label">${esc(i.label)}</span>${icon("chevron-right", "sub")}</button>`;
      else html += `<button class="mi" data-act="${esc(i.id)}" data-keep="${i.keep ? 1 : 0}" title="${esc(i.hint)}" ${i.enabled ? "" : "disabled"}>` +
        `<span class="ck">${i.checked ? icon("check") : (i.icon && i.checked !== false ? icon(i.icon) : "")}</span>` +
        (i.icon && i.checked != null ? `<span class="glyph">${icon(i.icon)}</span>` : "") + `<span class="label">${esc(i.label)}</span>${i.key ? `<span class="key">${esc(i.key)}</span>` : ""}</button>`;
    });
    html += `</div>`;
    root.innerHTML = html;
    reportSize();
  }

  function reportSize() {
    const m = root.querySelector(".menu");
    if (!m) return;
    // the page border adds 2px each way
    post({ t: "size", w: Math.ceil(m.scrollWidth) + 2, h: Math.ceil(m.scrollHeight) + 2 });
  }

  // ---- drawing and events ----

  const draw = { top: drawTop, left: drawLeft, status: drawStatus, popup: drawPopup, nav: drawNav }[pane];

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
  });

  document.addEventListener("click", (e) => {
    const t = e.target.closest("button, .row, .group");
    if (!t || t.disabled) return focusViews(e);
    if (t.dataset.popup) {
      const r = t.getBoundingClientRect();
      if (S.popup === t.dataset.popup) post({ t: "close" });
      else post({ t: "popup", key: t.dataset.popup, x: r.left, y: r.bottom + 4, w: r.width, up: pane === "status" ? r.top - 4 : null });
    } else if (t.dataset.back) {
      local.stack.pop();
      drawPopup();
      return;
    } else if (t.dataset.sub) {
      local.stack.push(t.dataset.sub);
      drawPopup();
      return;
    } else if (t.dataset.act) {
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
    }
  });

  document.addEventListener("change", (e) => {
    const t = e.target;
    if (t.dataset.text === "1") {
      post({ t: "set", key: t.dataset.key, text: t.value });
    } else if (t.dataset.key && t.dataset.key !== "filter" && t.dataset.key !== "palette") {
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
    if (pane === "popup" && e.key === "Escape") { post({ t: "close" }); return; }
    // Anything else is a shortcut for the views.
    post({ t: "key", key: e.key, code: e.code, ctrl: e.ctrlKey || e.metaKey, shift: e.shiftKey, alt: e.altKey, repeat: e.repeat });
    if (e.ctrlKey || e.metaKey || e.key === "F3" || e.key === "Tab") e.preventDefault();
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
