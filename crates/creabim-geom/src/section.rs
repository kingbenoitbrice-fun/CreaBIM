//! Sections and elevations generated from the model's 3D geometry.
//!
//! A section slices every element's mesh with a vertical plane and fills the
//! cut outlines; everything beyond the plane is drawn as projected edges with
//! hidden lines removed. An elevation is the same drawing with nothing cut.
//!
//! Hidden-line removal uses a software depth buffer: all faces beyond the
//! cut plane are rasterised, then each feature edge is sampled along its
//! length and kept only where it is not behind a face.

use crate::plan::{Pen, PlanItem, PlanView};
use crate::SceneItem;
use creabim_core::{ElementId, ElementKind, Model, ViewStyle};
use glam::{DVec2, DVec3, Vec3};
use std::collections::{HashMap, HashSet};

/// Drawing coordinates: `x` runs along the section line, `y` is height.
pub type Drawing = PlanView;

/// The view frame of a section line.
#[derive(Clone, Copy, Debug)]
pub struct SectionFrame {
    pub origin: DVec2,
    /// Unit vector along the section line.
    pub along: DVec2,
    /// Unit vector the viewer looks along (left of `along`).
    pub look: DVec2,
    pub length: f64,
    pub depth: f64,
}

impl SectionFrame {
    pub fn new(start: DVec2, end: DVec2, depth: f64) -> Self {
        let along = (end - start).normalize_or_zero();
        Self {
            origin: start,
            along,
            look: along.perp(),
            length: start.distance(end),
            depth,
        }
    }

    /// World point → (along, height, distance in front of the cut plane).
    pub fn project(&self, p: DVec3) -> DVec3 {
        let d = p.truncate() - self.origin;
        DVec3::new(d.dot(self.along), p.z, d.dot(self.look))
    }
}

/// A section (or, with `cut` off, an elevation). With `shaded`, surfaces
/// beyond the cut are shaded in their material colours under the linework.
pub fn section_view(
    model: &Model,
    start: DVec2,
    end: DVec2,
    depth: f64,
    cut: bool,
    shaded: bool,
) -> Drawing {
    section_view_at(model, start, end, depth, cut, shaded, 100.0)
}

/// [`section_view`] drawn for `scale` (100 for 1:100), which sizes the
/// level heads.
pub fn section_view_at(
    model: &Model,
    start: DVec2,
    end: DVec2,
    depth: f64,
    cut: bool,
    shaded: bool,
    scale: f64,
) -> Drawing {
    section_view_with(
        model,
        start,
        end,
        depth,
        cut,
        style_of(shaded),
        scale,
        false,
    )
}

fn style_of(shaded: bool) -> ViewStyle {
    if shaded {
        ViewStyle::Shaded
    } else {
        ViewStyle::HiddenLine
    }
}

/// [`section_view_at`] drawn in `style`, with `reveal` drawing what is
/// hidden inside other elements (a beam in a wall) dashed.
#[allow(clippy::too_many_arguments)]
pub fn section_view_with(
    model: &Model,
    start: DVec2,
    end: DVec2,
    depth: f64,
    cut: bool,
    style: ViewStyle,
    scale: f64,
    reveal: bool,
) -> Drawing {
    let frame = SectionFrame::new(start, end, depth);
    let scene = crate::scene::drawn_scene(model);
    let mut items = Vec::new();

    let project = |p: DVec3| frame.project(p);
    let revealed = reveal.then(|| revealable(model));
    let (lines, image) = hidden_lines(
        &scene,
        &project,
        frame.depth,
        Some((0.0, frame.length)),
        style,
        revealed.as_ref(),
    );
    items.extend(image);
    items.extend(lines);
    items.extend(void_lines(
        model,
        &project,
        frame.depth,
        Some((0.0, frame.length)),
    ));

    // Cut fill, drawn over the projection; layered elements layer by layer.
    let mut layers = vec![];
    if cut {
        for s in &scene {
            for loop_ in cut_loops(s, &frame) {
                let pts: Vec<DVec2> = loop_.iter().map(|p| DVec2::new(p.x, p.y)).collect();
                if pts.iter().all(|p| p.x < -1e-6 || p.x > frame.length + 1e-6) {
                    continue;
                }
                match s.layer {
                    Some(layer) => layers.push(crate::plan::CutPiece {
                        id: s.id,
                        points: pts,
                        layer: Some(layer),
                    }),
                    None => items.push(PlanItem::Polygon {
                        id: s.id,
                        pen: Pen::Cut,
                        points: pts,
                    }),
                }
            }
        }
    }

    crate::plan::clean_up_pieces(model, &mut items, layers);
    if reveal {
        crate::plan::reveal_covered(model, &mut items);
    }

    // Sketches between the cut plane and the far edge of the view
    for e in model.iter() {
        let ElementKind::Sketch(sk) = &e.kind else {
            continue;
        };
        let z = sk
            .level
            .and_then(|l| model.level(l))
            .map_or(0.0, |l| l.elevation);
        let lift = |p: DVec3| frame.project(p + DVec3::Z * z);
        let inside = sk.segments().iter().all(|(a, b)| {
            [lift(*a), lift(*b)]
                .iter()
                .all(|p| (-1e-6..=frame.depth + 1e-6).contains(&p.z))
        });
        if inside {
            items.extend(crate::plan::sketch_items(e.id, sk, &lift));
        }
    }

    // Guides drawn on the section plane
    for e in model.iter() {
        let ElementKind::Guide(g) = &e.kind else {
            continue;
        };
        let (a, b) = (frame.project(g.a), frame.project(g.b));
        if a.z.abs() > 0.01 || b.z.abs() > 0.01 {
            continue;
        }
        if let Some((a, b)) = crate::plan::guide_span(a.truncate(), b.truncate()) {
            items.push(PlanItem::Line {
                id: e.id,
                pen: Pen::Guide,
                a,
                b,
            });
        }
    }

    // Level datums across what the drawing shows, a metre past each side.
    let span = PlanView {
        items: items.clone(),
    }
    .bounds()
    .map_or((0.0, frame.length), |(lo, hi)| (lo.x, hi.x));
    items.extend(level_datums(model, span, scale));
    PlanView { items }
}

/// Level lines across a section or elevation spanning `span` along it, a
/// metre past each side, each with its head (name and elevation in the
/// project's units) at the right-hand end. `scale` sizes the heads.
pub fn level_datums(model: &Model, span: (f64, f64), scale: f64) -> Vec<PlanItem> {
    let units = model.units();
    let look = crate::annotate::look_of(model, None, None);
    let mut items = Vec::new();
    for (e, l) in model.levels() {
        let (x0, x1) = (span.0 - 1.0, span.1 + 1.0);
        items.push(PlanItem::Line {
            id: e.id,
            pen: Pen::Grid,
            a: DVec2::new(x0, l.elevation),
            b: DVec2::new(x1, l.elevation),
        });
        items.push(PlanItem::Annotation {
            id: e.id,
            item: Box::new(crate::annotation::Annotation {
                shape: crate::annotation::Shape::Level {
                    at: DVec2::new(x1, l.elevation),
                    elevation: units.elevation(l.elevation),
                    line_end: None,
                },
                text: e.name.clone(),
                look,
                mm: scale / 1000.0,
            }),
        });
    }
    items
}

/// Lay out the level heads of a section or elevation drawn at `scale`:
/// heads the view turns off are left out, dragged ones go where they were
/// dragged, and the rest are spread apart where levels are too close for
/// their text, each joined to its datum line by a dog-leg.
pub fn lay_out_level_heads(
    items: &mut Vec<PlanItem>,
    heads: &std::collections::BTreeMap<ElementId, creabim_core::LevelHead>,
    scale: f64,
) {
    use crate::annotation::Shape;
    let mm = scale / 1000.0;
    items.retain(|it| match it {
        PlanItem::Annotation { id, item } if matches!(item.shape, Shape::Level { .. }) => {
            !heads.get(id).is_some_and(|h| h.hidden)
        }
        _ => true,
    });
    // (index, datum end, text size) of the heads left to place
    let mut auto: Vec<(usize, DVec2, f64)> = Vec::new();
    for (i, it) in items.iter_mut().enumerate() {
        let PlanItem::Annotation { id, item } = it else {
            continue;
        };
        let size = item.look.size;
        let Shape::Level { at, line_end, .. } = &mut item.shape else {
            continue;
        };
        match heads.get(id).and_then(|h| h.offset) {
            Some(off) if off.length() > 1e-9 => {
                *line_end = Some(*at);
                *at += off;
            }
            _ => auto.push((i, *at, size)),
        }
    }
    auto.sort_by(|a, b| a.1.y.total_cmp(&b.1.y));
    // the height a head's two lines of text take, with a little air
    let gap = |size: f64| (size * 2.6 + 2.4) * mm;
    let ys = spread(
        &auto.iter().map(|a| a.1.y).collect::<Vec<_>>(),
        auto.iter().map(|a| gap(a.2)).fold(0.0, f64::max),
    );
    // heads too near another step out a little, so each has its dog-leg
    let crowded: Vec<bool> = (0..auto.len())
        .map(|k| {
            (0..auto.len())
                .any(|j| j != k && (auto[j].1.y - auto[k].1.y).abs() < gap(auto[k].2) - 1e-9)
        })
        .collect();
    for (k, (i, end, _)) in auto.iter().enumerate() {
        if !crowded[k] {
            continue;
        }
        if let PlanItem::Annotation { item, .. } = &mut items[*i] {
            if let Shape::Level { at, line_end, .. } = &mut item.shape {
                *line_end = Some(*end);
                *at = DVec2::new(end.x + 4.0 * mm, ys[k]);
            }
        }
    }
}

/// Heights for labels wanted at the sorted heights `want`, at least `gap`
/// apart: each crowd of labels is spread evenly about where it wants to be.
pub fn spread(want: &[f64], gap: f64) -> Vec<f64> {
    // clusters of (first index, count), merged until none overlap
    let mut clusters: Vec<(usize, usize, f64)> =
        want.iter().enumerate().map(|(i, y)| (i, 1, *y)).collect();
    let bottom = |c: &(usize, usize, f64)| c.2 - gap * (c.1 as f64 - 1.0) / 2.0;
    let top = |c: &(usize, usize, f64)| c.2 + gap * (c.1 as f64 - 1.0) / 2.0;
    loop {
        let clash = clusters
            .windows(2)
            .position(|w| bottom(&w[1]) - top(&w[0]) < gap - 1e-9);
        let Some(k) = clash else { break };
        let (a, b) = (clusters[k], clusters[k + 1]);
        let n = a.1 + b.1;
        let mean = want[a.0..a.0 + n].iter().sum::<f64>() / n as f64;
        clusters[k] = (a.0, n, mean);
        clusters.remove(k + 1);
    }
    let mut out = Vec::with_capacity(want.len());
    for c in &clusters {
        for j in 0..c.1 {
            out.push(bottom(c) + gap * j as f64);
        }
    }
    out
}

/// Section of a view element, if it is a section or elevation.
/// The frame of a parallel 3D view: unit vectors on screen and into it.
#[derive(Clone, Copy, Debug)]
pub struct AxoFrame {
    pub center: DVec3,
    pub right: DVec3,
    pub up: DVec3,
    pub forward: DVec3,
    /// Depth offset that puts the whole model in front of the viewer.
    pub radius: f64,
}

impl AxoFrame {
    /// Viewer at compass `azimuth` (0 = south, counter-clockwise) and
    /// `altitude` above the horizon, looking at the model's centre.
    pub fn new(model: &Model, azimuth: f64, altitude: f64) -> Self {
        let scene = crate::scene::drawn_scene(model);
        let (lo, hi) = scene
            .iter()
            .filter_map(|s| s.mesh.bounds())
            .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
            .map(|(a, b)| (a.as_dvec3(), b.as_dvec3()))
            .unwrap_or((DVec3::ZERO, DVec3::ONE));
        let (az, alt) = (azimuth.to_radians(), altitude.to_radians());
        let flat = DVec3::new(-az.sin(), az.cos(), 0.0);
        let forward = (flat * alt.cos() - DVec3::Z * alt.sin()).normalize();
        let right = DVec3::new(az.cos(), az.sin(), 0.0);
        Self {
            center: (lo + hi) / 2.0,
            right,
            up: right.cross(forward),
            forward,
            radius: (hi - lo).length() / 2.0 + 1.0,
        }
    }

    /// World point → (screen x, screen y, depth from the viewer).
    pub fn project(&self, p: DVec3) -> DVec3 {
        let d = p - self.center;
        DVec3::new(
            d.dot(self.right),
            d.dot(self.up),
            d.dot(self.forward) + self.radius,
        )
    }

    /// The viewing ray through a drawing point.
    pub fn ray(&self, p: DVec2) -> (DVec3, DVec3) {
        let o = self.center + self.right * p.x + self.up * p.y - self.forward * self.radius;
        (o, self.forward)
    }
}

/// A parallel 3D view of the whole model, hidden lines removed, optionally shaded.
pub fn axonometric_view(model: &Model, azimuth: f64, altitude: f64, shaded: bool) -> Drawing {
    axonometric_view_with(model, azimuth, altitude, style_of(shaded), false, &[])
}

/// [`axonometric_view`] drawn in `style`, with `reveal` drawing what is
/// hidden inside other elements dashed, cropped by `crop` (the view's crop
/// boxes). The view stays centred on the whole model, so cropping never
/// shifts it on a sheet.
pub fn axonometric_view_with(
    model: &Model,
    azimuth: f64,
    altitude: f64,
    style: ViewStyle,
    reveal: bool,
    crop: &[creabim_core::CropBox],
) -> Drawing {
    let frame = AxoFrame::new(model, azimuth, altitude);
    let mut scene = crate::scene::drawn_scene(model);
    crate::cropbox::crop_scene(model, &mut scene, crop);
    let project = |p: DVec3| frame.project(p);
    let revealed = reveal.then(|| revealable(model));
    let (lines, image) = hidden_lines(
        &scene,
        &project,
        frame.radius * 2.0,
        None,
        style,
        revealed.as_ref(),
    );
    let voids = if crop.is_empty() {
        void_lines(model, &project, frame.radius * 2.0, None)
    } else {
        // only the shafts' edges inside the crop boxes
        let keep = |p: DVec3| {
            crop.iter()
                .any(|b| p.cmpge(b.min - 1e-6).all() && p.cmple(b.max + 1e-6).all())
        };
        void_lines_where(model, &project, frame.radius * 2.0, &keep)
    };
    PlanView {
        items: image.into_iter().chain(lines).chain(voids).collect(),
    }
}

/// The edges of `model`'s shafts seen through `project`, dashed: they
/// are empty space, so a section, elevation or 3D view shows where they
/// run without hiding anything, and they can be clicked there.
fn void_lines(
    model: &Model,
    project: &dyn Fn(DVec3) -> DVec3,
    depth: f64,
    crop: Option<(f64, f64)>,
) -> Vec<PlanItem> {
    let mut out = Vec::new();
    for (id, a, b) in shaft_edges(model) {
        let Some((a, b)) = clip_segment(project(a), project(b), 1e-4, depth) else {
            continue;
        };
        let (a, b) = match crop {
            Some((x0, x1)) => match crop_along(a, b, x0, x1) {
                Some(seg) => seg,
                None => continue,
            },
            None => (a, b),
        };
        out.push(PlanItem::Line {
            id,
            pen: Pen::Hidden,
            a: a.truncate(),
            b: b.truncate(),
        });
    }
    out
}

/// [`void_lines`] of a cropped 3D view: the edges with both ends in `keep`.
fn void_lines_where(
    model: &Model,
    project: &dyn Fn(DVec3) -> DVec3,
    depth: f64,
    keep: &dyn Fn(DVec3) -> bool,
) -> Vec<PlanItem> {
    shaft_edges(model)
        .into_iter()
        .filter(|(_, a, b)| keep(*a) && keep(*b))
        .filter_map(|(id, a, b)| {
            let (a, b) = clip_segment(project(a), project(b), 1e-4, depth)?;
            Some(PlanItem::Line {
                id,
                pen: Pen::Hidden,
                a: a.truncate(),
                b: b.truncate(),
            })
        })
        .collect()
}

/// Every edge of every shaft: its outline at its base and its top, and
/// its corners between them.
pub fn shaft_edges(model: &Model) -> Vec<(ElementId, DVec3, DVec3)> {
    let mut out = Vec::new();
    for e in model.iter() {
        let ElementKind::Shaft(sh) = &e.kind else {
            continue;
        };
        let (z0, z1) = sh.span(model);
        let n = sh.boundary.len();
        for i in 0..n {
            let (p, q) = (sh.boundary[i], sh.boundary[(i + 1) % n]);
            out.push((e.id, p.extend(z0), q.extend(z0)));
            out.push((e.id, p.extend(z1), q.extend(z1)));
            out.push((e.id, p.extend(z0), p.extend(z1)));
        }
    }
    out
}

type Faces = Vec<(ElementId, [f32; 3], Vec<[DVec3; 3]>)>;

/// The elements a view showing hidden elements draws dashed when they are
/// inside another: everything but doors and windows, which sit in their
/// own holes.
fn revealable(model: &Model) -> HashSet<ElementId> {
    model
        .iter()
        .filter(|e| !matches!(e.kind, ElementKind::Opening(_)))
        .map(|e| e.id)
        .collect()
}

/// Feature edges of `scene` seen through `project`, hidden parts removed
/// (all of them kept in [`ViewStyle::Wireframe`]). `project` maps world
/// points to (x, y, depth); only `0..=depth` is drawn, and with `crop`,
/// only `x` within that range. Shaded, it also returns an image of the
/// surfaces, lit from the viewer's upper left. Edges of the
/// `reveal` elements that are hidden because they are inside another
/// element (behind its near face and in front of its far one) come back
/// dashed, as [`Pen::Hidden`].
fn hidden_lines(
    scene: &[SceneItem],
    project: &dyn Fn(DVec3) -> DVec3,
    depth: f64,
    crop: Option<(f64, f64)>,
    style: ViewStyle,
    reveal: Option<&HashSet<ElementId>>,
) -> (Vec<PlanItem>, Option<PlanItem>) {
    let shade = style == ViewStyle::Shaded;
    let wire = style == ViewStyle::Wireframe;
    let mut faces: Faces = Vec::new();
    for s in scene {
        let mut tris = Vec::new();
        for t in s.mesh.indices.chunks(3) {
            let p = |i: u32| project(Vec3::from(s.mesh.positions[i as usize]).as_dvec3());
            tris.extend(clip_triangle([p(t[0]), p(t[1]), p(t[2])], depth));
        }
        faces.push((s.id, s.color, tris));
    }
    let (lo, hi) = extents(&faces, crop);
    let mut zbuf = DepthBuffer::render(&faces, lo, hi, shade);
    if reveal.is_some() {
        zbuf.render_backs(&faces);
    }
    let image = zbuf.image();
    let mut items = Vec::new();
    for (index, s) in scene.iter().enumerate() {
        let revealed = reveal.is_some_and(|r| r.contains(&s.id));
        for (a, b) in feature_edges(s) {
            let Some((a, b)) = clip_segment(project(a), project(b), 1e-4, depth) else {
                continue;
            };
            let (a, b) = match crop {
                Some((x0, x1)) => match crop_along(a, b, x0, x1) {
                    Some(seg) => seg,
                    None => continue,
                },
                None => (a, b),
            };
            if wire {
                items.push(PlanItem::Line {
                    id: s.id,
                    pen: Pen::Projection,
                    a: a.truncate(),
                    b: b.truncate(),
                });
                continue;
            }
            for (p, q) in zbuf.visible_runs(a, b) {
                items.push(PlanItem::Line {
                    id: s.id,
                    pen: Pen::Projection,
                    a: p,
                    b: q,
                });
            }
            if revealed {
                for (p, q) in zbuf.runs(a, b, |p| !zbuf.visible(p) && zbuf.inside_other(p, index)) {
                    items.push(PlanItem::Line {
                        id: s.id,
                        pen: Pen::Hidden,
                        a: p,
                        b: q,
                    });
                }
            }
        }
    }
    (items, image)
}

/// The live drawing of a view element, annotations included.
pub fn view_drawing(model: &Model, view: ElementId) -> Option<Drawing> {
    view_drawing_with(model, view, false)
}

/// [`view_drawing`], with `xray` drawing what is hidden inside other
/// elements whether or not the view shows it: for a view seen in X-ray
/// while working, which its sheets don't follow.
pub fn view_drawing_with(model: &Model, view: ElementId, xray: bool) -> Option<Drawing> {
    let e = model.get(view)?;
    let ElementKind::View(v) = &e.kind else {
        return None;
    };
    let reveal = v.show_hidden || xray;
    let style = v.style;
    // what the view leaves out: gone from the model it is drawn from, and
    // from the drawing (for what has to stay in that model, like levels)
    let mut hidden = v.hidden.hidden_set(model);
    // a section never hides its own drawing, even with section marks off
    hidden.remove(&view);
    let filtered;
    let model = if hidden.is_empty() {
        model
    } else {
        filtered = creabim_core::visibility::without(model, &hidden);
        &filtered
    };
    let mut d = match &v.kind {
        creabim_core::ViewKind::Plan { level, cut_height } => {
            let mut d = crate::plan::plan_view_with(model, *level, *cut_height, &v.graphics);
            if reveal {
                crate::plan::reveal_covered(model, &mut d.items);
            }
            if v.ceiling_plan {
                crate::rooms::as_ceiling_plan(model, &mut d.items);
            }
            // rooms under everything, separators and ceilings with them
            let mut rooms = crate::rooms::plan_items(model, v, *level, *cut_height);
            // with the pens the view's graphics give their categories
            crate::plan::category_pens(model, &mut rooms, &v.graphics);
            rooms.append(&mut d.items);
            d.items = rooms;
            // another level traced underneath, drawn first so it stays below
            if let Some(under) = v.underlay.level(model, *level) {
                let mut items = crate::plan::underlay_items(model, under, *cut_height);
                items.append(&mut d.items);
                d.items = items;
            }
            d
        }
        creabim_core::ViewKind::Section {
            start,
            end,
            depth,
            cut,
        } => {
            let mut d =
                section_view_with(model, *start, *end, *depth, *cut, style, v.scale, reveal);
            // rooms washed behind the cut, from floor to ceiling
            if v.room_colours && *cut {
                let mut rooms = crate::rooms::section_items(model, *start, *end);
                rooms.append(&mut d.items);
                d.items = rooms;
            }
            d
        }
        creabim_core::ViewKind::Axonometric { azimuth, altitude } => {
            let crop = crate::cropbox::active(model, Some(view));
            axonometric_view_with(model, *azimuth, *altitude, style, reveal, &crop)
        }
        creabim_core::ViewKind::Schedule { category, by_type } => {
            let table = crate::schedule::schedule_by(model, *category, *by_type);
            return Some(crate::schedule::schedule_drawing(view, &e.name, &table));
        }
    };
    if reveal && v.hidden_in_colour {
        tint_hidden(model, view, &v.kind, &mut d.items);
    }
    // tag the image with its view, so viewers can keep one texture per view
    for it in &mut d.items {
        if let PlanItem::Image { id, raster, .. } = it {
            *id = view;
            if let Some(r) = std::sync::Arc::get_mut(raster) {
                r.key = view_key(view);
            }
        }
    }
    if let creabim_core::ViewKind::Section { .. } = v.kind {
        lay_out_level_heads(&mut d.items, &v.level_heads, v.scale);
    }
    d.items.extend(crate::annotate::annotations(model, view, v));
    if !hidden.is_empty() {
        d.items.retain(|it| !hidden.contains(&it.id()));
    }
    // a plan's names, notes and tags moved off lines and each other
    if matches!(v.kind, creabim_core::ViewKind::Plan { .. }) {
        crate::declutter::place_labels(&mut d.items, v.scale / 1000.0);
    }
    // dimension lines whose style says so break where tags and notes
    // finally sit
    crate::annotate::break_dimensions(&mut d.items, v.scale / 1000.0);
    Some(d)
}

/// Wash the elements a drawing shows hidden inside others (those with
/// dashed lines in `items`) in their material colours, over what hides
/// them and under their dashed lines: each piece as the outline of what
/// of it the view sees.
fn tint_hidden(
    model: &Model,
    view: ElementId,
    kind: &creabim_core::ViewKind,
    items: &mut Vec<PlanItem>,
) {
    let revealable = revealable(model);
    let hidden: HashSet<ElementId> = items
        .iter()
        .filter_map(|it| match it {
            PlanItem::Line {
                id,
                pen: Pen::Hidden,
                ..
            } if revealable.contains(id) => Some(*id),
            _ => None,
        })
        // what holds others is only dashed where it runs into its
        // neighbours (a wall's end in the next wall), not hidden inside
        .filter(|id| {
            !matches!(
                model.get(*id).map(|e| &e.kind),
                Some(ElementKind::Wall(_) | ElementKind::Slab(_) | ElementKind::Roof(_))
            )
        })
        .collect();
    if hidden.is_empty() {
        return;
    }
    let mut scene = crate::scene::drawn_scene(model);
    scene.retain(|s| hidden.contains(&s.id));
    let tints = match kind {
        creabim_core::ViewKind::Plan { level, cut_height } => {
            let z = model.level(*level).map_or(0.0, |l| l.elevation) + cut_height;
            // from the cut down to a metre under the level
            let project = |p: DVec3| DVec3::new(p.x, p.y, z - p.z);
            outlines(&scene, &project, cut_height + 1.0, None)
        }
        creabim_core::ViewKind::Section {
            start, end, depth, ..
        } => {
            let frame = SectionFrame::new(*start, *end, *depth);
            let project = |p: DVec3| frame.project(p);
            outlines(&scene, &project, frame.depth, Some((0.0, frame.length)))
        }
        creabim_core::ViewKind::Axonometric { azimuth, altitude } => {
            // tinted as cropped, like the rest of the drawing
            let crop = crate::cropbox::active(model, Some(view));
            crate::cropbox::crop_scene(model, &mut scene, &crop);
            let frame = AxoFrame::new(model, *azimuth, *altitude);
            let project = |p: DVec3| frame.project(p);
            outlines(&scene, &project, frame.radius * 2.0, None)
        }
        creabim_core::ViewKind::Schedule { .. } => vec![],
    };
    // under the dashed lines, over everything else
    let (dashed, rest): (Vec<PlanItem>, Vec<PlanItem>) =
        std::mem::take(items).into_iter().partition(
            |it| matches!(it, PlanItem::Line { pen: Pen::Hidden, id, .. } if hidden.contains(id)),
        );
    *items = rest;
    items.extend(tints);
    items.extend(dashed);
}

/// The outline (convex hull) of each scene item seen through `project`,
/// within `0..=depth` and, with `crop`, `x` in that range, as tints in the
/// item's colour.
fn outlines(
    scene: &[SceneItem],
    project: &dyn Fn(DVec3) -> DVec3,
    depth: f64,
    crop: Option<(f64, f64)>,
) -> Vec<PlanItem> {
    let mut out = Vec::new();
    for s in scene {
        let mut pts = Vec::new();
        for t in s.mesh.indices.chunks(3) {
            let p = |i: u32| project(Vec3::from(s.mesh.positions[i as usize]).as_dvec3());
            for tri in clip_triangle([p(t[0]), p(t[1]), p(t[2])], depth) {
                pts.extend(tri.iter().map(|q| DVec2::new(q.x, q.y)));
            }
        }
        let mut hull = convex_hull(pts);
        if let Some((x0, x1)) = crop {
            hull = crate::crop::clip_polygon(&hull, DVec2::new(x0, -1e9), DVec2::new(x1, 1e9));
        }
        if hull.len() >= 3 {
            out.push(PlanItem::Tint {
                id: s.id,
                points: hull,
                rgb: s.color,
            });
        }
    }
    out
}

/// The convex hull of `pts`, counter-clockwise (Andrew's monotone chain).
fn convex_hull(mut pts: Vec<DVec2>) -> Vec<DVec2> {
    pts.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    pts.dedup_by(|a, b| a.distance_squared(*b) < 1e-14);
    if pts.len() < 3 {
        return pts;
    }
    let cross = |o: DVec2, a: DVec2, b: DVec2| (a - o).perp_dot(b - o);
    let chain = |pts: &mut dyn Iterator<Item = DVec2>| {
        let mut h: Vec<DVec2> = Vec::new();
        for p in pts {
            while h.len() >= 2 && cross(h[h.len() - 2], h[h.len() - 1], p) <= 1e-12 {
                h.pop();
            }
            h.push(p);
        }
        h.pop();
        h
    };
    let mut hull = chain(&mut pts.iter().copied());
    hull.extend(chain(&mut pts.iter().rev().copied()));
    hull
}

fn view_key(view: ElementId) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    view.hash(&mut h);
    h.finish()
}

fn extents(faces: &Faces, crop: Option<(f64, f64)>) -> (DVec2, DVec2) {
    let mut lo = DVec2::splat(f64::MAX);
    let mut hi = DVec2::splat(f64::MIN);
    for (_, _, tris) in faces {
        for t in tris {
            for p in t {
                lo = lo.min(p.truncate());
                hi = hi.max(p.truncate());
            }
        }
    }
    if let Some((x0, x1)) = crop {
        (lo.x, hi.x) = (x0, x1);
    }
    if lo.x > hi.x || lo.y > hi.y {
        (lo, hi) = (DVec2::ZERO, DVec2::ONE);
    }
    (lo - DVec2::splat(0.1), hi + DVec2::splat(0.1))
}

/// Keep the part of a triangle with `0 <= d <= depth` (d = z component).
fn clip_triangle(tri: [DVec3; 3], depth: f64) -> Vec<[DVec3; 3]> {
    let mut poly: Vec<DVec3> = tri.to_vec();
    for (sign, limit) in [(1.0, 0.0), (-1.0, -depth)] {
        // keep points with sign * d >= limit
        let inside = |p: &DVec3| sign * p.z >= limit;
        let mut out = Vec::with_capacity(poly.len() + 2);
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            if inside(&a) {
                out.push(a);
            }
            if inside(&a) != inside(&b) {
                let t = (limit - sign * a.z) / (sign * (b.z - a.z));
                out.push(a + (b - a) * t);
            }
        }
        poly = out;
        if poly.len() < 3 {
            return vec![];
        }
    }
    (1..poly.len() - 1)
        .map(|i| [poly[0], poly[i], poly[i + 1]])
        .collect()
}

fn clip_segment(a: DVec3, b: DVec3, near: f64, far: f64) -> Option<(DVec3, DVec3)> {
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    let dz = b.z - a.z;
    for (num, den) in [(a.z - near, -dz), (far - a.z, dz)] {
        // constraint: num + t * (-den) >= 0  rewritten per Liang–Barsky
        if den.abs() < 1e-12 {
            if num < 0.0 {
                return None;
            }
        } else {
            let t = num / den;
            if den > 0.0 {
                t1 = t1.min(t);
            } else {
                t0 = t0.max(t);
            }
        }
    }
    (t0 <= t1).then(|| (a + (b - a) * t0, a + (b - a) * t1))
}

fn crop_along(a: DVec3, b: DVec3, lo: f64, hi: f64) -> Option<(DVec3, DVec3)> {
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    let dx = b.x - a.x;
    if dx.abs() < 1e-12 {
        return (a.x >= lo && a.x <= hi).then_some((a, b));
    }
    let (ta, tb) = ((lo - a.x) / dx, (hi - a.x) / dx);
    t0 = t0.max(ta.min(tb));
    t1 = t1.min(ta.max(tb));
    (t0 <= t1).then(|| (a + (b - a) * t0, a + (b - a) * t1))
}

/// Edges where the surface creases, plus open boundaries. Coplanar
/// triangulation edges inside a flat face are skipped.
pub fn feature_edges(item: &SceneItem) -> Vec<(DVec3, DVec3)> {
    let key = |p: [f32; 3]| {
        let q = |v: f32| (v as f64 * 1e4).round() as i64;
        (q(p[0]), q(p[1]), q(p[2]))
    };
    type Key = (i64, i64, i64);
    let mut edges: HashMap<(Key, Key), (DVec3, DVec3, Vec<Vec3>)> = HashMap::new();
    let m = &item.mesh;
    for t in m.indices.chunks(3) {
        let n = Vec3::from(m.normals[t[0] as usize]);
        for k in 0..3 {
            let (pa, pb) = (
                m.positions[t[k] as usize],
                m.positions[t[(k + 1) % 3] as usize],
            );
            let (ka, kb) = (key(pa), key(pb));
            if ka == kb {
                continue;
            }
            let id = if ka < kb { (ka, kb) } else { (kb, ka) };
            edges
                .entry(id)
                .or_insert_with(|| (Vec3::from(pa).as_dvec3(), Vec3::from(pb).as_dvec3(), vec![]))
                .2
                .push(n);
        }
    }
    let mut out: Vec<(DVec3, DVec3)> = edges
        .into_values()
        .filter(|(_, _, ns)| {
            ns.len() == 1 || ns.iter().any(|a| ns.iter().any(|b| a.dot(*b) < 0.995))
        })
        .map(|(a, b, _)| (a, b))
        .collect();
    out.sort_by(|x, y| {
        (x.0.x, x.0.y, x.0.z)
            .partial_cmp(&(y.0.x, y.0.y, y.0.z))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// Closed outlines where the plane d = 0 slices an element's mesh, in drawing coordinates.
fn cut_loops(item: &SceneItem, frame: &SectionFrame) -> Vec<Vec<DVec2>> {
    let m = &item.mesh;
    let mut segs: Vec<(DVec2, DVec2)> = Vec::new();
    for t in m.indices.chunks(3) {
        let p: Vec<DVec3> = t
            .iter()
            .map(|&i| frame.project(Vec3::from(m.positions[i as usize]).as_dvec3()))
            .collect();
        let mut hits = Vec::new();
        for k in 0..3 {
            let (a, b) = (p[k], p[(k + 1) % 3]);
            if (a.z > 0.0) != (b.z > 0.0) {
                let s = a.z / (a.z - b.z);
                let q = a + (b - a) * s;
                hits.push(DVec2::new(q.x, q.y));
            }
        }
        if hits.len() == 2 && hits[0].distance(hits[1]) > 1e-9 {
            segs.push((hits[0], hits[1]));
        }
    }
    chain(segs)
}

/// Join segments that share endpoints into loops.
fn chain(mut segs: Vec<(DVec2, DVec2)>) -> Vec<Vec<DVec2>> {
    let close = |a: DVec2, b: DVec2| a.distance(b) < 1e-6;
    let mut loops = Vec::new();
    while let Some((a, b)) = segs.pop() {
        let mut pts = vec![a, b];
        loop {
            let end = *pts.last().expect("non-empty");
            let Some(i) = segs
                .iter()
                .position(|(p, q)| close(*p, end) || close(*q, end))
            else {
                break;
            };
            let (p, q) = segs.swap_remove(i);
            let next = if close(p, end) { q } else { p };
            if close(next, pts[0]) {
                break;
            }
            pts.push(next);
        }
        loops.push(simplify(pts));
    }
    loops.retain(|l| l.len() >= 3);
    loops
}

/// Drop points that sit on a straight line between their neighbours.
fn simplify(pts: Vec<DVec2>) -> Vec<DVec2> {
    let n = pts.len();
    if n < 4 {
        return pts;
    }
    (0..n)
        .filter(|&i| {
            let (a, b, c) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n]);
            (b - a).perp_dot(c - b).abs() > 1e-9 * (b - a).length().max(1.0)
        })
        .map(|i| pts[i])
        .collect()
}

struct DepthBuffer {
    lo: DVec2,
    px: f64,
    w: usize,
    h: usize,
    depth: Vec<f32>,
    /// Shaded colour per cell, rows from the bottom, when shading.
    rgba: Option<Vec<u8>>,
    /// Which element (by its index in the faces) is nearest in each cell.
    nearest: Vec<u32>,
    /// How deep that element goes in each cell (its far side), once
    /// [`DepthBuffer::render_backs`] has run.
    back: Vec<f32>,
}

/// No element in the cell.
const EMPTY: u32 = u32::MAX;

/// Direction light comes from, in view coordinates (x right, y up, depth away).
const LIGHT: DVec3 = DVec3::new(-0.45, 0.65, -0.62);

impl DepthBuffer {
    fn render(faces: &Faces, lo: DVec2, hi: DVec2, shade: bool) -> Self {
        let size = hi - lo;
        let px = (size.x.max(size.y) / 2048.0).max(0.005);
        let w = (size.x / px).ceil() as usize + 1;
        let h = (size.y / px).ceil() as usize + 1;
        let mut buf = Self {
            lo,
            px,
            w,
            h,
            depth: vec![f32::INFINITY; w * h],
            rgba: shade.then(|| vec![0; w * h * 4]),
            nearest: vec![EMPTY; w * h],
            back: vec![f32::NEG_INFINITY; w * h],
        };
        let light = LIGHT.normalize();
        for (index, (_, color, tris)) in faces.iter().enumerate() {
            for t in tris {
                let n = (t[1] - t[0]).cross(t[2] - t[0]).normalize_or_zero();
                let lit = 0.55 + 0.45 * n.dot(light).abs();
                let c = color.map(|c| ((c as f64 * lit).clamp(0.0, 1.0) * 255.0) as u8);
                buf.raster(t, c, index as u32);
            }
        }
        buf
    }

    /// Find how deep the nearest element goes in each cell: the furthest
    /// of its own faces there.
    fn render_backs(&mut self, faces: &Faces) {
        for (index, (_, _, tris)) in faces.iter().enumerate() {
            for t in tris {
                self.cover(t, |buf, i, d| {
                    if buf.nearest[i] == index as u32 && d > buf.back[i] {
                        buf.back[i] = d;
                    }
                });
            }
        }
    }

    /// Is the point inside another element than `own`: behind the near
    /// face of the element nearest there and in front of its far one?
    fn inside_other(&self, p: DVec3, own: usize) -> bool {
        let q = self.to_px(p);
        let (x, y) = (q.x.floor(), q.y.floor());
        if x < 0.0 || y < 0.0 || x as usize >= self.w || y as usize >= self.h {
            return false;
        }
        let i = y as usize * self.w + x as usize;
        let by = self.nearest[i];
        let z = p.z as f32;
        by != EMPTY && by != own as u32 && z > self.depth[i] + 0.02 && z < self.back[i] + 0.02
    }

    /// The shaded colours as an image item, if shading.
    fn image(&self) -> Option<PlanItem> {
        let src = self.rgba.as_ref()?;
        // the buffer's rows run from the bottom; images run from the top
        let mut rgba = Vec::with_capacity(src.len());
        for row in src.chunks(self.w * 4).rev() {
            rgba.extend_from_slice(row);
        }
        Some(PlanItem::Image {
            id: ElementId::NIL,
            raster: std::sync::Arc::new(crate::raster::Raster {
                width: self.w,
                height: self.h,
                rgba,
                key: 0,
            }),
            lo: self.lo,
            hi: self.lo + DVec2::new(self.w as f64, self.h as f64) * self.px,
            clip: None,
        })
    }

    fn to_px(&self, p: DVec3) -> DVec3 {
        DVec3::new(
            (p.x - self.lo.x) / self.px,
            (p.y - self.lo.y) / self.px,
            p.z,
        )
    }

    fn raster(&mut self, t: &[DVec3; 3], color: [u8; 3], index: u32) {
        self.cover(t, |buf, i, d| {
            if d < buf.depth[i] {
                buf.depth[i] = d;
                buf.nearest[i] = index;
                if let Some(rgba) = &mut buf.rgba {
                    rgba[i * 4..i * 4 + 4].copy_from_slice(&[color[0], color[1], color[2], 255]);
                }
            }
        });
    }

    /// Call `f` with each cell the triangle covers and its depth there.
    fn cover(&mut self, t: &[DVec3; 3], mut f: impl FnMut(&mut Self, usize, f32)) {
        let [a, b, c] = t.map(|p| self.to_px(p));
        let area = (b - a).truncate().perp_dot((c - a).truncate());
        if area.abs() < 1e-12 {
            return; // seen edge-on
        }
        let x0 = a.x.min(b.x).min(c.x).floor().max(0.0) as usize;
        let x1 = (a.x.max(b.x).max(c.x).ceil() as usize).min(self.w - 1);
        let y0 = a.y.min(b.y).min(c.y).floor().max(0.0) as usize;
        let y1 = (a.y.max(b.y).max(c.y).ceil() as usize).min(self.h - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let p = glam::DVec2::new(x as f64 + 0.5, y as f64 + 0.5);
                let w0 = (c - b).truncate().perp_dot(p - b.truncate()) / area;
                let w1 = (a - c).truncate().perp_dot(p - c.truncate()) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let d = (w0 * a.z + w1 * b.z + w2 * c.z) as f32;
                f(self, y * self.w + x, d);
            }
        }
    }

    /// Is the point no further than the nearest face around it (with tolerance)?
    fn visible(&self, p: DVec3) -> bool {
        let q = self.to_px(p);
        let (cx, cy) = (q.x.floor() as isize, q.y.floor() as isize);
        let mut far = f32::NEG_INFINITY;
        for dy in -1..=1 {
            for dx in -1..=1 {
                let (x, y) = (cx + dx, cy + dy);
                if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h {
                    return true;
                }
                far = far.max(self.depth[y as usize * self.w + x as usize]);
            }
        }
        p.z as f32 <= far + 0.02
    }

    fn visible_runs(&self, a: DVec3, b: DVec3) -> Vec<(DVec2, DVec2)> {
        self.runs(a, b, |p| self.visible(p))
    }

    /// The stretches of `a`-`b` where `keep` holds, in drawing coordinates.
    fn runs(&self, a: DVec3, b: DVec3, keep: impl Fn(DVec3) -> bool) -> Vec<(DVec2, DVec2)> {
        let len_px = ((b - a).truncate().length() / self.px).ceil().max(1.0) as usize;
        let steps = len_px * 2;
        let mut runs = Vec::new();
        let mut start: Option<f64> = None;
        let mut last = 0.0;
        for i in 0..=steps {
            let t = i as f64 / steps as f64;
            let vis = keep(a + (b - a) * t);
            match (vis, start) {
                (true, None) => start = Some(t),
                (false, Some(s)) => {
                    runs.push((s, last));
                    start = None;
                }
                _ => {}
            }
            if vis {
                last = t;
            }
        }
        if let Some(s) = start {
            runs.push((s, 1.0));
        }
        runs.into_iter()
            .filter(|(s, e)| e > s)
            .map(|(s, e)| {
                let p = a + (b - a) * s;
                let q = a + (b - a) * e;
                (DVec2::new(p.x, p.y), DVec2::new(q.x, q.y))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::sample::sample_house;

    fn sample() -> Model {
        sample_house()
    }

    fn section_a(m: &Model) -> ElementId {
        m.iter().find(|e| e.name == "Section A").unwrap().id
    }

    fn set_view(m: &Model, id: ElementId, f: impl FnOnce(&mut creabim_core::View)) -> Model {
        let mut doc = creabim_core::Document::from_model(m.clone());
        doc.edit(id, "Change view", |e| {
            if let ElementKind::View(v) = &mut e.kind {
                f(v);
            }
        })
        .unwrap();
        doc.model().clone()
    }

    #[test]
    fn x_ray_draws_a_beam_buried_in_a_wall_without_changing_the_view() {
        let (m, beam) = creabim_core::sample::sample_with_buried_beam();
        let sec = section_a(&m);
        let dashed = |d: &Drawing| {
            d.items
                .iter()
                .any(|i| matches!(i, PlanItem::Line { id, pen: Pen::Hidden, .. } if *id == beam))
        };
        // the wall's cut covers the beam's
        assert!(!dashed(&view_drawing(&m, sec).unwrap()));
        assert!(dashed(&view_drawing_with(&m, sec, true).unwrap()));
        // the view (and so its sheets) is as it was
        assert!(!dashed(&view_drawing(&m, sec).unwrap()));
    }

    #[test]
    fn a_buried_beam_shows_in_its_material_colour_for_detailing() {
        let (m, beam) = creabim_core::sample::sample_with_buried_beam();
        let sec = section_a(&m);
        let tint = |d: &Drawing| {
            d.items.iter().find_map(|i| match i {
                PlanItem::Tint { id, points, rgb } if *id == beam => Some((points.clone(), *rgb)),
                _ => None,
            })
        };
        assert!(tint(&view_drawing(&m, sec).unwrap()).is_none());
        let coloured = set_view(&m, sec, |v| {
            v.show_hidden = true;
            v.hidden_in_colour = true;
        });
        let d = view_drawing(&coloured, sec).unwrap();
        let (pts, rgb) = tint(&d).expect("the beam is washed in its colour");
        // timber, the beam's material
        assert!(
            (rgb[0] - 0.70).abs() < 0.02 && (rgb[2] - 0.40).abs() < 0.02,
            "{rgb:?}"
        );
        // its section: 0.16 wide about the wall's line, 2.3 to 2.6 high
        let (lo, hi) = pts
            .iter()
            .fold((pts[0], pts[0]), |(lo, hi), p| (lo.min(*p), hi.max(*p)));
        assert!(
            (hi.y - 2.6).abs() < 0.02 && (lo.y - 2.3).abs() < 0.02,
            "{lo} {hi}"
        );
        assert!((hi.x - lo.x - 0.16).abs() < 0.02, "{lo} {hi}");
        // drawn over what hides it, under its dashed lines
        let at = |f: &dyn Fn(&PlanItem) -> bool| d.items.iter().position(f).unwrap();
        let tint_at = at(&|i| matches!(i, PlanItem::Tint { id, .. } if *id == beam));
        let dash_at =
            at(&|i| matches!(i, PlanItem::Line { id, pen: Pen::Hidden, .. } if *id == beam));
        let fill_at = at(&|i| matches!(i, PlanItem::Fill { .. }));
        assert!(fill_at < tint_at && tint_at < dash_at);
        // the colour needs hidden elements shown
        let off = set_view(&m, sec, |v| v.hidden_in_colour = true);
        assert!(tint(&view_drawing(&off, sec).unwrap()).is_none());
    }

    #[test]
    fn hulls_are_convex_and_counter_clockwise() {
        let pts = vec![
            DVec2::new(0.0, 0.0),
            DVec2::new(1.0, 0.0),
            DVec2::new(0.5, 0.5),
            DVec2::new(1.0, 1.0),
            DVec2::new(0.0, 1.0),
            DVec2::new(1.0, 0.0),
        ];
        let h = convex_hull(pts);
        assert_eq!(h.len(), 4);
        let area: f64 = (0..4).map(|i| h[i].perp_dot(h[(i + 1) % 4])).sum::<f64>() / 2.0;
        assert!((area - 1.0).abs() < 1e-9);
    }

    #[test]
    fn section_through_the_front_door_cuts_walls_and_slabs() {
        let m = sample();
        // line at x = 2.1 running north, looking west
        let d = section_view(
            &m,
            DVec2::new(2.1, -3.0),
            DVec2::new(2.1, 9.0),
            20.0,
            true,
            false,
        );
        let cut_names: std::collections::BTreeSet<String> = d
            .items
            .iter()
            .filter_map(|i| match i {
                PlanItem::Fill { id, .. } => Some(m.get(*id).unwrap().name.clone()),
                _ => None,
            })
            .collect();
        for n in [
            "GF Wall 1",
            "GF Wall 3",
            "FF Wall 1",
            "FF Wall 3",
            "GF Slab",
            "FF Slab",
            "Entrance Canopy",
        ] {
            assert!(cut_names.contains(n), "{n} not cut; got {cut_names:?}");
        }
        assert!(
            !cut_names.contains("GF Partition"),
            "partition at x = 6 is behind the viewer"
        );
        // the front door opening splits GF Wall 1's cut: the slice below the door is
        // missing, so the wall is cut as a lintel above the door only
        let wall1: Vec<_> = d
            .items
            .iter()
            .filter_map(|i| match i {
                PlanItem::Fill { id, points, .. } if m.get(*id).unwrap().name == "GF Wall 1" => {
                    Some(points.clone())
                }
                _ => None,
            })
            .collect();
        assert_eq!(wall1.len(), 1);
        let min_z = wall1[0].iter().map(|p| p.y).fold(f64::MAX, f64::min);
        assert!(
            (min_z - 2.2).abs() < 1e-6,
            "lintel starts at the door head, got {min_z}"
        );
    }

    #[test]
    fn a_guide_on_the_section_plane_shows_there_but_not_in_plans() {
        let mut m = sample();
        let guide = creabim_core::Element::new(
            "Guide",
            ElementKind::Guide(creabim_core::Guide {
                level: None,
                a: DVec3::new(2.1, 0.0, 0.5),
                b: DVec3::new(2.1, 4.0, 2.5),
            }),
        );
        let id = guide.id;
        creabim_core::Transaction::new("t")
            .insert(guide)
            .apply(&mut m)
            .unwrap();
        let d = section_view(
            &m,
            DVec2::new(2.1, -3.0),
            DVec2::new(2.1, 9.0),
            20.0,
            true,
            false,
        );
        assert!(d.items.iter().any(|i| i.id() == id));
        let level = m.levels().next().unwrap().0.id;
        let plan = crate::plan_view(&m, level, 1.2);
        assert!(!plan.items.iter().any(|i| i.id() == id));
    }

    #[test]
    fn elevation_cuts_nothing_and_hides_the_back_wall() {
        let m = sample();
        // south elevation, standing at y = -4 looking north
        let d = section_view(
            &m,
            DVec2::new(-2.0, -4.0),
            DVec2::new(12.0, -4.0),
            30.0,
            false,
            false,
        );
        assert!(!d.items.iter().any(|i| matches!(i, PlanItem::Fill { .. })));
        let names: std::collections::BTreeSet<String> = d
            .items
            .iter()
            .filter_map(|i| match i {
                PlanItem::Line {
                    id,
                    pen: Pen::Projection,
                    ..
                } => Some(m.get(*id).unwrap().name.clone()),
                _ => None,
            })
            .collect();
        assert!(names.contains("GF Wall 1") && names.contains("Front Door"));
        assert!(names.contains("Entrance Canopy"));
        assert!(
            !names.contains("GF Window 3.1"),
            "a window in the back wall is hidden"
        );
        assert!(
            !names.contains("GF Partition"),
            "the inner partition is hidden"
        );
    }

    #[test]
    fn walls_cut_through_the_slabs_that_sit_in_them() {
        let m = sample();
        let d = section_view(
            &m,
            DVec2::new(2.1, -3.0),
            DVec2::new(2.1, 9.0),
            20.0,
            true,
            false,
        );
        let fills = |kind: &str| -> Vec<Vec<DVec2>> {
            d.items
                .iter()
                .filter_map(|i| match i {
                    PlanItem::Fill { id, points, .. }
                        if m.get(*id).unwrap().name.contains(kind) =>
                    {
                        Some(points.clone())
                    }
                    _ => None,
                })
                .collect()
        };
        let (walls, slabs) = (fills("Wall"), fills("Slab"));
        assert!(!walls.is_empty() && !slabs.is_empty());
        let inside = |p: DVec2, poly: &[DVec2]| {
            let (lo, hi) = poly
                .iter()
                .fold((DVec2::MAX, DVec2::MIN), |(a, b), q| (a.min(*q), b.max(*q)));
            p.cmpgt(lo + 1e-6).all() && p.cmplt(hi - 1e-6).all()
        };
        // no point of a slab's cut lies inside a wall's cut
        for s in &slabs {
            let c = s.iter().copied().sum::<DVec2>() / s.len() as f64;
            for w in &walls {
                assert!(!inside(c, w), "slab piece centred at {c} overlaps a wall");
            }
        }
    }

    #[test]
    fn shaded_views_carry_an_image_under_their_lines() {
        let m = sample();
        let d = axonometric_view(&m, 35.0, 30.0, true);
        let Some(PlanItem::Image { raster, .. }) = d.items.first() else {
            panic!("the image comes first, under the linework");
        };
        assert!(raster.width > 100 && raster.height > 100);
        // some pixels are covered, and the corners are background
        assert!(raster.rgba.chunks(4).any(|p| p[3] == 255));
        assert_eq!(raster.rgba[3], 0);
        assert!(!axonometric_view(&m, 35.0, 30.0, false)
            .items
            .iter()
            .any(|i| matches!(i, PlanItem::Image { .. })));
    }

    #[test]
    fn clipping_keeps_only_the_part_beyond_the_plane() {
        let t = [
            DVec3::new(0.0, 0.0, -1.0),
            DVec3::new(1.0, 0.0, 1.0),
            DVec3::new(0.0, 1.0, 1.0),
        ];
        let parts = clip_triangle(t, 10.0);
        assert!(!parts.is_empty());
        assert!(parts.iter().flatten().all(|p| p.z >= -1e-12));
    }
}
