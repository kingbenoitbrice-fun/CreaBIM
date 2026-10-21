//! Title blocks drawn elsewhere, read into a [`SheetTemplate`]: the lines,
//! polylines, circles, arcs and text of a DXF file, or of an SVG drawing.
//! Text can carry `{fields}` (`{sheet}`, `{number}`, `{project}`,
//! `{scale}`, `{date}` or fields of your own), filled in on each sheet.
//!
//! A drawing about the size of a sheet is taken as a whole sheet frame:
//! it keeps its place from the lower-left corner and brings its own
//! border. A smaller one is taken as a title block and moved into the
//! sheet's lower-right corner, inside the margin.
//!
//! Blocks (DXF INSERTs), hatches, images and SVG curves beyond straight
//! segments are not read; curves come in as their end points joined.

use creabim_core::titleblock::{Corner, SheetTemplate, TextAlign, TitleItem};
use glam::DVec2;

/// Margin between a title block and the sheet's edge, mm.
const MARGIN: f64 = 10.0;
/// Drawings this wide or wider are whole sheet frames, mm.
const SHEET_SIZED: f64 = 250.0;

/// Read a title block from a `.dxf` or `.svg` file.
pub fn read(path: &str) -> Result<SheetTemplate, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("couldn't read {path}: {e}"))?;
    let lower = path.to_lowercase();
    let items = if lower.ends_with(".dxf") {
        dxf_items(&text)?
    } else if lower.ends_with(".svg") {
        svg_items(&text)?
    } else {
        return Err("title blocks are read from .dxf or .svg files".into());
    };
    from_items(items)
}

/// A template from drawn items in paper mm, y up, placed as a whole sheet
/// or a title block by its size.
pub fn from_items(items: Vec<TitleItem>) -> Result<SheetTemplate, String> {
    if items.is_empty() {
        return Err("found no lines or text to make a title block from".into());
    }
    let mut t = SheetTemplate {
        corner: Corner::BottomRight,
        items,
        border: true,
        fields: vec![],
        paper: None,
    };
    let (lo, hi) = t.bounds().unwrap_or_default();
    let size = hi - lo;
    if size.x >= SHEET_SIZED {
        // a whole sheet: its own frame, from the lower-left corner
        t.corner = Corner::BottomLeft;
        t.border = false;
        // its paper: the drawing and as much again as its lower-left
        // margin, or the standard size that is that near enough
        let guess = DVec2::new(hi.x + lo.x.max(0.0), hi.y + lo.y.max(0.0)).max(size);
        let standard = creabim_core::Sheet::SIZES
            .iter()
            .map(|(_, w, h)| DVec2::new(*w, *h))
            .min_by(|a, b| a.distance(guess).total_cmp(&b.distance(guess)))
            .filter(|s| s.distance(guess) < 0.12 * s.length());
        t.paper = Some(standard.unwrap_or(guess));
    } else {
        t.fit_to_corner();
        let inset = DVec2::new(-MARGIN, MARGIN);
        for it in &mut t.items {
            match it {
                TitleItem::Line { points, .. } | TitleItem::Fill { points, .. } => {
                    points.iter_mut().for_each(|p| *p += inset)
                }
                TitleItem::Text { at, .. } => *at += inset,
            }
        }
    }
    // fields of its own: the ones its text uses beyond the built-in ones
    let builtin: Vec<&str> = creabim_core::titleblock::BUILT_IN_FIELDS
        .iter()
        .map(|f| f.0)
        .collect();
    t.fields = t
        .used_fields()
        .into_iter()
        .filter(|f| !builtin.contains(&f.as_str()))
        .map(|f| (f.clone(), String::new()))
        .collect();
    t.validate()?;
    Ok(t)
}

// ---- DXF ---------------------------------------------------------------------

/// Millimetres per drawing unit for a DXF `$INSUNITS` code.
fn dxf_unit(code: i64) -> f64 {
    match code {
        1 => 25.4,
        2 => 304.8,
        5 => 10.0,
        6 => 1000.0,
        _ => 1.0,
    }
}

/// The lines, polylines, circles, arcs and text of a DXF file's
/// entities, in mm.
pub fn dxf_items(text: &str) -> Result<Vec<TitleItem>, String> {
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    if lines.len() < 2 {
        return Err("that is not a DXF file".into());
    }
    let pairs: Vec<(i64, &str)> = lines
        .chunks(2)
        .filter(|c| c.len() == 2)
        .filter_map(|c| Some((c[0].parse::<i64>().ok()?, c[1])))
        .collect();
    if pairs.is_empty() {
        return Err("that is not a DXF file".into());
    }
    // units from the header
    let mut k = 1.0;
    for w in pairs.windows(2) {
        if w[0] == (9, "$INSUNITS") {
            k = dxf_unit(w[1].1.parse().unwrap_or(4));
        }
    }
    // the entities: each starts at a 0 group
    let start = pairs
        .iter()
        .position(|p| *p == (2, "ENTITIES"))
        .ok_or("the DXF file has no ENTITIES section")?;
    let mut out = vec![];
    let mut i = start + 1;
    let num = |v: &str| v.trim().parse::<f64>().unwrap_or(0.0);
    let mut poly: Option<(Vec<DVec2>, bool)> = None;
    while i < pairs.len() {
        let (code, name) = pairs[i];
        if code != 0 {
            i += 1;
            continue;
        }
        if name == "ENDSEC" {
            break;
        }
        let mut j = i + 1;
        while j < pairs.len() && pairs[j].0 != 0 {
            j += 1;
        }
        let g = &pairs[i + 1..j];
        let get = |c: i64| g.iter().find(|p| p.0 == c).map(|p| num(p.1));
        let p = |x: i64, y: i64| DVec2::new(get(x).unwrap_or(0.0), get(y).unwrap_or(0.0)) * k;
        match name {
            "LINE" => out.push(TitleItem::Line {
                points: vec![p(10, 20), p(11, 21)],
                width: 0.25,
            }),
            "LWPOLYLINE" => {
                let xs = g.iter().filter(|q| q.0 == 10).map(|q| num(q.1));
                let ys = g.iter().filter(|q| q.0 == 20).map(|q| num(q.1));
                let mut pts: Vec<DVec2> = xs.zip(ys).map(|(x, y)| DVec2::new(x, y) * k).collect();
                let closed = get(70).is_some_and(|f| (f as i64) & 1 == 1);
                if closed && !pts.is_empty() {
                    pts.push(pts[0]);
                }
                let width = get(43).map_or(0.25, |w| (w * k).max(0.13));
                if pts.len() > 1 {
                    out.push(TitleItem::Line { points: pts, width });
                }
            }
            "POLYLINE" => {
                let closed = get(70).is_some_and(|f| (f as i64) & 1 == 1);
                poly = Some((vec![], closed));
            }
            "VERTEX" => {
                if let Some((pts, _)) = &mut poly {
                    pts.push(p(10, 20));
                }
            }
            "SEQEND" => {
                if let Some((mut pts, closed)) = poly.take() {
                    if closed && !pts.is_empty() {
                        pts.push(pts[0]);
                    }
                    if pts.len() > 1 {
                        out.push(TitleItem::Line {
                            points: pts,
                            width: 0.25,
                        });
                    }
                }
            }
            "CIRCLE" | "ARC" => {
                let c = p(10, 20);
                let r = get(40).unwrap_or(0.0) * k;
                let (a0, a1) = if name == "ARC" {
                    let (a, b) = (get(50).unwrap_or(0.0), get(51).unwrap_or(360.0));
                    (a, if b < a { b + 360.0 } else { b })
                } else {
                    (0.0, 360.0)
                };
                if r > 0.0 {
                    let n = (((a1 - a0) / 10.0).ceil() as usize).max(4);
                    let points = (0..=n)
                        .map(|s| {
                            let t = (a0 + (a1 - a0) * s as f64 / n as f64).to_radians();
                            c + DVec2::from_angle(t) * r
                        })
                        .collect();
                    out.push(TitleItem::Line {
                        points,
                        width: 0.25,
                    });
                }
            }
            "TEXT" | "MTEXT" => {
                let raw: String = if name == "MTEXT" {
                    // long text comes in 3 groups before the last 1
                    let mut s: String = g
                        .iter()
                        .filter(|q| q.0 == 3)
                        .map(|q| q.1)
                        .collect::<Vec<_>>()
                        .concat();
                    s.push_str(g.iter().find(|q| q.0 == 1).map_or("", |q| q.1));
                    mtext_plain(&s)
                } else {
                    g.iter().find(|q| q.0 == 1).map_or("", |q| q.1).to_string()
                };
                let size = get(40).unwrap_or(2.5) * k;
                // left, centre, right; TEXT aligns on its second point then
                let h = get(72).unwrap_or(0.0) as i64;
                let align = match (name, h, get(71).map(|v| v as i64)) {
                    ("MTEXT", _, Some(2 | 5 | 8)) => TextAlign::Middle,
                    ("MTEXT", _, Some(3 | 6 | 9)) => TextAlign::End,
                    ("TEXT", 1 | 4, _) => TextAlign::Middle,
                    ("TEXT", 2, _) => TextAlign::End,
                    _ => TextAlign::Start,
                };
                let base = if name == "TEXT" && h != 0 && get(11).is_some() {
                    p(11, 21)
                } else {
                    p(10, 20)
                };
                let angle = get(50).unwrap_or(0.0).to_radians();
                // DXF places text by its baseline (MTEXT by its top);
                // a title item by its middle
                let up = DVec2::from_angle(angle).perp();
                let at = if name == "MTEXT" {
                    base - up * size * 0.5
                } else {
                    base + up * size * 0.5
                };
                for (n, line) in raw.split('\n').enumerate() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    out.push(TitleItem::Text {
                        at: at - up * size * 1.5 * n as f64,
                        size: size.max(0.5),
                        text: line.to_string(),
                        align,
                        angle,
                    });
                }
            }
            _ => {}
        }
        i = j;
    }
    Ok(out)
}

/// MTEXT without its formatting codes; `\P` breaks lines.
fn mtext_plain(s: &str) -> String {
    let mut out = String::new();
    let mut kept: Vec<bool> = vec![];
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => match chars.next() {
                Some('P') => out.push('\n'),
                Some('\\') => out.push('\\'),
                // codes with an argument run to a semicolon
                Some('f' | 'F' | 'H' | 'C' | 'T' | 'Q' | 'W' | 'A' | 'p') => {
                    for d in chars.by_ref() {
                        if d == ';' {
                            break;
                        }
                    }
                }
                _ => {}
            },
            // braces group formatting, unless they hold a {field}
            '{' => {
                let field = chars.peek().is_some_and(|n| *n != '\\');
                kept.push(field);
                if field {
                    out.push('{');
                }
            }
            '}' => {
                if kept.pop().unwrap_or(false) {
                    out.push('}');
                }
            }
            c => out.push(c),
        }
    }
    out
}

// ---- SVG ---------------------------------------------------------------------

/// A 2D affine map: `[a, b, c, d, e, f]` as in SVG's `matrix()`.
type Affine = [f64; 6];

const IDENTITY: Affine = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn apply(m: &Affine, p: DVec2) -> DVec2 {
    DVec2::new(
        m[0] * p.x + m[2] * p.y + m[4],
        m[1] * p.x + m[3] * p.y + m[5],
    )
}

fn compose(m: &Affine, n: &Affine) -> Affine {
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4],
        m[1] * n[4] + m[3] * n[5] + m[5],
    ]
}

fn numbers(s: &str) -> Vec<f64> {
    let mut out = vec![];
    let mut cur = String::new();
    let flush = |cur: &mut String, out: &mut Vec<f64>| {
        if let Ok(v) = cur.parse::<f64>() {
            out.push(v);
        }
        cur.clear();
    };
    let mut prev = ' ';
    for c in s.chars() {
        let starts_new = (c == '-' || c == '+') && !(prev == 'e' || prev == 'E');
        if c.is_ascii_digit() || c == '.' || c == 'e' || c == 'E' || c == '-' || c == '+' {
            if starts_new || (c == '.' && cur.contains('.') && !cur.contains(['e', 'E'])) {
                flush(&mut cur, &mut out);
            }
            cur.push(c);
        } else {
            flush(&mut cur, &mut out);
        }
        prev = c;
    }
    flush(&mut cur, &mut out);
    out
}

/// An SVG `transform` attribute as one map.
fn transform(s: &str) -> Affine {
    let mut m = IDENTITY;
    for part in s.split(')') {
        let Some((name, args)) = part.split_once('(') else {
            continue;
        };
        let v = numbers(args);
        let at = |i: usize, d: f64| v.get(i).copied().unwrap_or(d);
        let n: Affine = match name.trim().trim_start_matches(',').trim() {
            "matrix" if v.len() == 6 => [v[0], v[1], v[2], v[3], v[4], v[5]],
            "translate" => [1.0, 0.0, 0.0, 1.0, at(0, 0.0), at(1, 0.0)],
            "scale" => [at(0, 1.0), 0.0, 0.0, at(1, at(0, 1.0)), 0.0, 0.0],
            "rotate" => {
                let (s, c) = at(0, 0.0).to_radians().sin_cos();
                let (cx, cy) = (at(1, 0.0), at(2, 0.0));
                let r = [c, s, -s, c, 0.0, 0.0];
                let t = [1.0, 0.0, 0.0, 1.0, cx, cy];
                let back = [1.0, 0.0, 0.0, 1.0, -cx, -cy];
                compose(&compose(&t, &r), &back)
            }
            _ => IDENTITY,
        };
        m = compose(&m, &n);
    }
    m
}

/// The value of attribute `name` in a tag's text.
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = tag;
    while let Some(i) = rest.find(name) {
        let before_ok = i == 0 || rest[..i].ends_with(|c: char| c.is_whitespace());
        let after = rest[i + name.len()..].trim_start();
        if before_ok {
            if let Some(after) = after.strip_prefix('=') {
                let after = after.trim_start();
                let q = after.chars().next()?;
                if q == '"' || q == '\'' {
                    let body = &after[1..];
                    return body.find(q).map(|j| &body[..j]);
                }
            }
        }
        rest = &rest[i + name.len()..];
    }
    None
}

/// A length attribute in user units (`mm` and `px` as they say).
fn len_attr(tag: &str, name: &str) -> Option<f64> {
    let v = attr(tag, name)?.trim();
    let n = numbers(v).first().copied()?;
    Some(if v.ends_with("mm") {
        n
    } else if v.ends_with("cm") {
        n * 10.0
    } else if v.ends_with("in") {
        n * 25.4
    } else if v.ends_with("pt") {
        n * 25.4 / 72.0
    } else {
        n
    })
}

/// A style or attribute value (`style="font-size:4px"` or `font-size="4"`).
fn style_value(tag: &str, name: &str) -> Option<f64> {
    if let Some(v) = attr(tag, name) {
        return numbers(v).first().copied();
    }
    let style = attr(tag, "style")?;
    style.split(';').find_map(|kv| {
        let (k, v) = kv.split_once(':')?;
        (k.trim() == name).then(|| numbers(v).first().copied())?
    })
}

fn xml_unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The polylines of an SVG path: straight segments, curves by their end
/// points.
fn path_polylines(d: &str) -> Vec<(Vec<DVec2>, bool)> {
    let mut out: Vec<(Vec<DVec2>, bool)> = vec![];
    let mut cur: Vec<DVec2> = vec![];
    let mut pos = DVec2::ZERO;
    let mut start = DVec2::ZERO;
    // split into commands and their numbers
    let mut cmds: Vec<(char, Vec<f64>)> = vec![];
    let mut buf = String::new();
    let mut cmd: Option<char> = None;
    for c in d.chars() {
        if c.is_ascii_alphabetic() && c != 'e' && c != 'E' {
            if let Some(k) = cmd {
                cmds.push((k, numbers(&buf)));
            }
            cmd = Some(c);
            buf.clear();
        } else {
            buf.push(c);
        }
    }
    if let Some(k) = cmd {
        cmds.push((k, numbers(&buf)));
    }
    let finish = |cur: &mut Vec<DVec2>, out: &mut Vec<(Vec<DVec2>, bool)>, closed: bool| {
        if cur.len() > 1 {
            out.push((std::mem::take(cur), closed));
        } else {
            cur.clear();
        }
    };
    for (k, v) in cmds {
        let rel = k.is_ascii_lowercase();
        let base = |pos: DVec2| if rel { pos } else { DVec2::ZERO };
        match k.to_ascii_uppercase() {
            'M' => {
                finish(&mut cur, &mut out, false);
                for (i, xy) in v.chunks(2).filter(|c| c.len() == 2).enumerate() {
                    pos = base(pos) + DVec2::new(xy[0], xy[1]);
                    if i == 0 {
                        start = pos;
                    }
                    cur.push(pos);
                }
            }
            'L' | 'T' => {
                for xy in v.chunks(2).filter(|c| c.len() == 2) {
                    pos = base(pos) + DVec2::new(xy[0], xy[1]);
                    cur.push(pos);
                }
            }
            'H' => {
                for x in &v {
                    pos = DVec2::new(if rel { pos.x + x } else { *x }, pos.y);
                    cur.push(pos);
                }
            }
            'V' => {
                for y in &v {
                    pos = DVec2::new(pos.x, if rel { pos.y + y } else { *y });
                    cur.push(pos);
                }
            }
            // curves and arcs: by where each piece ends
            'C' | 'S' | 'Q' | 'A' => {
                let n = match k.to_ascii_uppercase() {
                    'C' => 6,
                    'S' | 'Q' => 4,
                    _ => 7,
                };
                for piece in v.chunks(n).filter(|c| c.len() == n) {
                    pos = base(pos) + DVec2::new(piece[n - 2], piece[n - 1]);
                    cur.push(pos);
                }
            }
            'Z' => {
                cur.push(start);
                pos = start;
                finish(&mut cur, &mut out, true);
            }
            _ => {}
        }
    }
    finish(&mut cur, &mut out, false);
    out
}

/// The lines, shapes and text of an SVG drawing, in mm with y up.
pub fn svg_items(text: &str) -> Result<Vec<TitleItem>, String> {
    let open = text.find("<svg").ok_or("that is not an SVG file")?;
    let head_end = text[open..].find('>').ok_or("that is not an SVG file")? + open;
    let head = &text[open..head_end];
    // user units to mm: by the page size over the viewBox, else 96 dpi px
    let vb = attr(head, "viewBox").map(numbers).filter(|v| v.len() == 4);
    let w_attr = attr(head, "width").unwrap_or("");
    let width_mm = len_attr(head, "width").map(|w| {
        if w_attr.ends_with("mm")
            || w_attr.ends_with("cm")
            || w_attr.ends_with("in")
            || w_attr.ends_with("pt")
        {
            w
        } else {
            w * 25.4 / 96.0
        }
    });
    let (k, height) = match (&vb, width_mm) {
        (Some(v), Some(w)) if v[2] > 0.0 => (w / v[2], v[3]),
        (Some(v), None) => (25.4 / 96.0, v[3]),
        (None, Some(_)) => (
            if w_attr.trim().ends_with("mm") {
                1.0
            } else {
                25.4 / 96.0
            },
            len_attr(head, "height").unwrap_or(0.0),
        ),
        _ => (25.4 / 96.0, len_attr(head, "height").unwrap_or(0.0)),
    };
    let origin = vb.as_ref().map_or(DVec2::ZERO, |v| DVec2::new(v[0], v[1]));
    let page = |p: DVec2| {
        let q = p - origin;
        DVec2::new(q.x, height - q.y) * k
    };
    let mut out = vec![];
    let mut stack: Vec<Affine> = vec![IDENTITY];
    let mut rest = &text[head_end + 1..];
    while let Some(lt) = rest.find('<') {
        let after = &rest[lt + 1..];
        let Some(gt) = after.find('>') else { break };
        let tag = &after[..gt];
        rest = &after[gt + 1..];
        if tag.starts_with('!') || tag.starts_with('?') {
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            if name.trim() == "g" && stack.len() > 1 {
                stack.pop();
            }
            continue;
        }
        let name = tag.split_whitespace().next().unwrap_or("");
        let selfclose = tag.ends_with('/');
        let m = compose(
            stack.last().unwrap_or(&IDENTITY),
            &attr(tag, "transform").map_or(IDENTITY, transform),
        );
        let to = |p: DVec2| page(apply(&m, p));
        let width = style_value(tag, "stroke-width").map_or(0.25, |w| (w * k).clamp(0.1, 2.0));
        let fill = attr(tag, "fill")
            .or_else(|| {
                attr(tag, "style").and_then(|s| {
                    s.split(';')
                        .find_map(|kv| kv.split_once(':').filter(|(k, _)| k.trim() == "fill"))
                        .map(|(_, v)| v.trim())
                })
            })
            .filter(|f| !matches!(*f, "none" | "transparent" | "white" | "#fff" | "#ffffff"));
        let mut shape = |pts: Vec<DVec2>, closed: bool| {
            let mut pts: Vec<DVec2> = pts.into_iter().map(to).collect();
            if pts.len() < 2 {
                return;
            }
            if closed && fill.is_some() {
                out.push(TitleItem::Fill {
                    points: pts.clone(),
                    gray: 0.0,
                });
            }
            if closed && pts.first() != pts.last() {
                pts.push(pts[0]);
            }
            out.push(TitleItem::Line { points: pts, width });
        };
        let num = |n: &str| len_attr(tag, n).unwrap_or(0.0);
        match name {
            "g" => {
                if !selfclose {
                    stack.push(m);
                }
            }
            "line" => shape(
                vec![
                    DVec2::new(num("x1"), num("y1")),
                    DVec2::new(num("x2"), num("y2")),
                ],
                false,
            ),
            "rect" => {
                let (x, y, w, h) = (num("x"), num("y"), num("width"), num("height"));
                shape(
                    vec![
                        DVec2::new(x, y),
                        DVec2::new(x + w, y),
                        DVec2::new(x + w, y + h),
                        DVec2::new(x, y + h),
                    ],
                    true,
                );
            }
            "polyline" | "polygon" => {
                let v = numbers(attr(tag, "points").unwrap_or(""));
                let pts = v
                    .chunks(2)
                    .filter(|c| c.len() == 2)
                    .map(|c| DVec2::new(c[0], c[1]))
                    .collect();
                shape(pts, name == "polygon");
            }
            "circle" | "ellipse" => {
                let c = DVec2::new(num("cx"), num("cy"));
                let (rx, ry) = if name == "circle" {
                    (num("r"), num("r"))
                } else {
                    (num("rx"), num("ry"))
                };
                let pts = (0..36)
                    .map(|i| {
                        let t = i as f64 / 36.0 * std::f64::consts::TAU;
                        c + DVec2::new(t.cos() * rx, t.sin() * ry)
                    })
                    .collect();
                shape(pts, true);
            }
            "path" => {
                for (pts, closed) in path_polylines(attr(tag, "d").unwrap_or("")) {
                    shape(pts, closed);
                }
            }
            "text" if !selfclose => {
                // its words, tspans and all, up to </text>
                let end = rest.find("</text>").unwrap_or(rest.len());
                let inner = &rest[..end];
                let mut words = String::new();
                let mut depth = 0;
                for c in inner.chars() {
                    match c {
                        '<' => depth += 1,
                        '>' => depth -= 1,
                        c if depth == 0 => words.push(c),
                        _ => {}
                    }
                }
                let words = xml_unescape(words.trim());
                // a tspan can carry the position instead
                let first_tspan = inner.find("<tspan").map(|i| &inner[i..]);
                let pick = |n: &str| {
                    len_attr(tag, n)
                        .or_else(|| first_tspan.and_then(|t| len_attr(&t[..t.find('>')?], n)))
                        .unwrap_or(0.0)
                };
                let size = style_value(tag, "font-size").unwrap_or(10.0) * k;
                let align = match style_attr(tag, "text-anchor").as_deref() {
                    Some("middle") => TextAlign::Middle,
                    Some("end") => TextAlign::End,
                    _ => TextAlign::Start,
                };
                let base = to(DVec2::new(pick("x"), pick("y")));
                // SVG places text by its baseline; a title item by its middle
                let x_axis = to(DVec2::new(pick("x") + 1.0, pick("y"))) - base;
                let angle = x_axis.y.atan2(x_axis.x);
                if !words.is_empty() {
                    out.push(TitleItem::Text {
                        at: base + DVec2::from_angle(angle).perp() * size * 0.35,
                        size: size.max(0.5),
                        text: words,
                        align,
                        angle: if angle.abs() < 1e-9 { 0.0 } else { angle },
                    });
                }
                rest = &rest[end..];
            }
            _ => {}
        }
    }
    Ok(out)
}

/// A presentation attribute or the same in `style`.
fn style_attr(tag: &str, name: &str) -> Option<String> {
    if let Some(v) = attr(tag, name) {
        return Some(v.trim().to_string());
    }
    attr(tag, "style")?.split(';').find_map(|kv| {
        let (k, v) = kv.split_once(':')?;
        (k.trim() == name).then(|| v.trim().to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DXF: &str = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n0\nLWPOLYLINE\n90\n4\n70\n1\n10\n0\n20\n0\n10\n170\n20\n0\n10\n170\n20\n45\n10\n0\n20\n45\n0\nLINE\n10\n0\n20\n30\n11\n170\n21\n30\n0\nTEXT\n10\n5\n20\n35\n40\n4\n1\n{sheet}\n0\nMTEXT\n10\n5\n20\n20\n40\n2.5\n1\nDrawn by\\P{drawn_by}\n0\nCIRCLE\n10\n160\n20\n10\n40\n3\n0\nENDSEC\n0\nEOF\n";

    #[test]
    fn a_dxf_title_block_reads_its_lines_and_fields() {
        let items = dxf_items(DXF).unwrap();
        let lines = items
            .iter()
            .filter(|i| matches!(i, TitleItem::Line { .. }))
            .count();
        assert_eq!(lines, 3, "polyline, line and circle");
        let texts: Vec<&str> = items
            .iter()
            .filter_map(|i| match i {
                TitleItem::Text { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["{sheet}", "Drawn by", "{drawn_by}"]);
        // a closed polyline comes back closed
        let TitleItem::Line { points, .. } = &items[0] else {
            panic!()
        };
        assert_eq!(points.len(), 5);
        assert_eq!(points[0], points[4]);
        // a block this size is a title block: in the lower right, inside
        // the margin, with a field of its own
        let t = from_items(items).unwrap();
        assert_eq!(t.corner, Corner::BottomRight);
        assert!(t.border);
        let (lo, hi) = t.bounds().unwrap();
        assert!(
            (hi.x + MARGIN).abs() < 1e-9 && (lo.y - MARGIN).abs() < 1e-9,
            "{lo} {hi}"
        );
        assert_eq!(t.fields, vec![("drawn_by".to_string(), String::new())]);
    }

    #[test]
    fn dxf_in_metres_is_scaled_to_millimetres() {
        let m = DXF.replace("$INSUNITS\n70\n4", "$INSUNITS\n70\n6");
        let items = dxf_items(&m).unwrap();
        let TitleItem::Line { points, .. } = &items[1] else {
            panic!()
        };
        assert_eq!(points[1], DVec2::new(170_000.0, 30_000.0));
    }

    #[test]
    fn an_svg_sheet_frame_reads_in_millimetres_with_y_up() {
        let svg = r##"<?xml version="1.0"?>
<svg xmlns="http://www.w3.org/2000/svg" width="420mm" height="297mm" viewBox="0 0 420 297">
  <rect x="20" y="10" width="390" height="277" fill="none" stroke="#000" stroke-width="0.5"/>
  <g transform="translate(240,242)">
    <path d="M0 0 H170 V45 H0 Z" fill="none" stroke="black"/>
    <text x="5" y="20" style="font-size:5px">{sheet} &amp; {number}</text>
  </g>
</svg>"##;
        let items = svg_items(svg).unwrap();
        // the border's lower edge is 10 mm up from the bottom
        let TitleItem::Line { points, .. } = &items[0] else {
            panic!()
        };
        assert!(points
            .iter()
            .any(|p| (p.y - 10.0).abs() < 1e-9 && (p.x - 20.0).abs() < 1e-9));
        let text = items
            .iter()
            .find_map(|i| match i {
                TitleItem::Text { text, at, size, .. } => Some((text.clone(), *at, *size)),
                _ => None,
            })
            .unwrap();
        assert_eq!(text.0, "{sheet} & {number}");
        assert!((text.2 - 5.0).abs() < 1e-9);
        assert!(
            (text.1.x - 245.0).abs() < 1e-9 && text.1.y > 297.0 - 262.0,
            "{}",
            text.1
        );
        // a sheet-sized drawing is a whole frame, kept where it is
        let t = from_items(items).unwrap();
        assert_eq!(t.corner, Corner::BottomLeft);
        assert!(!t.border);
        assert!(t.paper.is_some_and(|p| p.x >= 410.0));
    }

    #[test]
    fn numbers_split_on_signs_and_second_points() {
        assert_eq!(numbers("M10-5.5.5e1,2"), vec![10.0, -5.5, 0.5e1, 2.0]);
        assert_eq!(transform("translate(10 20) scale(2)")[4], 10.0);
        assert_eq!(
            apply(&transform("translate(10 20) scale(2)"), DVec2::ONE),
            DVec2::new(12.0, 22.0)
        );
    }
}
