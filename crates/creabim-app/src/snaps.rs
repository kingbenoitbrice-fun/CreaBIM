//! Object snaps shared by every pane: ends, midpoints, intersections, points
//! on edges, perpendicular feet, centres and quadrants, grid lines, guide
//! lines, the axes through the point a line started from, and alignment
//! with the other points of the shape being drawn. Geometry is
//! held as segments in some 3D space (the model for the 3D view, drawing
//! coordinates with z = 0 for plans and sections) and compared on screen
//! through a projection, so the same pixel tolerance applies however far the
//! view is zoomed. Each snap remembers the element it came from, so a
//! dimension can stay attached to it.
//!
//! [`SnapSettings`] turns each kind on or off; every query takes one.

use creabim_core::ElementId;
use creabim_geom::{PlanItem, SceneItem};
use eframe::egui::{self, Color32, Painter, Pos2, Stroke};
use glam::{DVec2, DVec3};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapKind {
    Endpoint,
    Midpoint,
    /// The centre of an arc or circle, or of a flat face in 3D.
    Center,
    /// A quarter point of an arc or circle.
    Quadrant,
    Intersection,
    Perpendicular,
    OnEdge,
    Guide,
    /// On a grid line, or a level line in a section.
    OnGridLine,
    /// Along the X or Y axis through the start point.
    AxisX,
    AxisY,
    /// In line, across or along, with another point of the shape being
    /// drawn (its first corner, an earlier one, or a point the pointer
    /// rested on): see [`Alignment`].
    Aligned,
    /// Where two alignments cross, or an alignment crosses an edge.
    AlignedCross,
    /// No object snap: the point under the pointer, rounded to the
    /// increment when that is on.
    Grid,
}

impl SnapKind {
    pub fn label(self) -> &'static str {
        match self {
            SnapKind::Endpoint => "Endpoint",
            SnapKind::Midpoint => "Midpoint",
            SnapKind::Center => "Centre",
            SnapKind::Quadrant => "Quadrant",
            SnapKind::Intersection => "Intersection",
            SnapKind::Perpendicular => "Perpendicular",
            SnapKind::OnEdge => "On edge",
            SnapKind::Guide => "On guide",
            SnapKind::OnGridLine => "On grid/level",
            SnapKind::AxisX => "On X axis",
            SnapKind::AxisY => "On Y axis",
            SnapKind::Aligned => "Aligned",
            SnapKind::AlignedCross => "Aligned both ways",
            SnapKind::Grid => "",
        }
    }

    /// Ends, middles, centres and crossings: what a dimension or a handle
    /// constrained to a line can sensibly land on.
    pub fn is_point(self) -> bool {
        matches!(
            self,
            SnapKind::Endpoint
                | SnapKind::Midpoint
                | SnapKind::Center
                | SnapKind::Quadrant
                | SnapKind::Intersection
        )
    }
}

/// Which snaps are on, with a master switch over them all (F3). Holding Alt
/// suspends them while picking; see [`SnapSettings::live`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SnapSettings {
    pub on: bool,
    pub endpoint: bool,
    pub midpoint: bool,
    pub intersection: bool,
    pub perpendicular: bool,
    /// The nearest point on an edge.
    pub nearest: bool,
    /// Centres of arcs, circles and (in 3D) flat faces.
    pub center: bool,
    pub quadrant: bool,
    /// Grid lines (and level lines in sections), and where they cross.
    pub grid_lines: bool,
    pub guides: bool,
    /// Square to the X and Y axes through where a line started.
    pub axis: bool,
    /// In line with the other points of the shape being drawn, and where
    /// two such alignments cross.
    pub tracking: bool,
    /// Round free points to a 5 cm step.
    pub increment: bool,
}

impl Default for SnapSettings {
    fn default() -> Self {
        Self::ALL
    }
}

impl SnapSettings {
    /// Every snap on.
    pub const ALL: Self = Self {
        on: true,
        endpoint: true,
        midpoint: true,
        intersection: true,
        perpendicular: true,
        nearest: true,
        center: true,
        quadrant: true,
        grid_lines: true,
        guides: true,
        axis: true,
        tracking: true,
        increment: true,
    };

    /// These settings as they apply this frame: holding Alt turns them all
    /// off for the moment.
    pub fn live(self, alt: bool) -> Self {
        Self {
            on: self.on && !alt,
            ..self
        }
    }

    /// Only the point snaps of these (ends, middles, crossings, centres),
    /// for where a drag grabs an element.
    pub fn points_only(self) -> Self {
        Self {
            perpendicular: false,
            nearest: false,
            grid_lines: false,
            guides: false,
            axis: false,
            tracking: false,
            increment: false,
            ..self
        }
    }

    /// Is this kind of snap wanted?
    pub fn allows(&self, kind: SnapKind) -> bool {
        self.on
            && match kind {
                SnapKind::Endpoint => self.endpoint,
                SnapKind::Midpoint => self.midpoint,
                SnapKind::Center => self.center,
                SnapKind::Quadrant => self.quadrant,
                SnapKind::Intersection => self.intersection,
                SnapKind::Perpendicular => self.perpendicular,
                SnapKind::OnEdge => self.nearest,
                SnapKind::Guide => self.guides,
                SnapKind::OnGridLine => self.grid_lines,
                SnapKind::AxisX | SnapKind::AxisY => self.axis,
                SnapKind::Aligned | SnapKind::AlignedCross => self.tracking,
                SnapKind::Grid => self.increment,
            }
    }

    /// Is geometry from this source wanted at all?
    fn takes(&self, src: Source) -> bool {
        match src {
            Source::Edge => true,
            Source::Guide => self.guides,
            Source::GridLine => self.grid_lines,
        }
    }

    /// The increment free points round to, 0 for none.
    fn step(&self, grid: f64) -> f64 {
        if self.allows(SnapKind::Grid) {
            grid
        } else {
            0.0
        }
    }

    /// Each switch with its label, in the order the snap bar lists them.
    pub fn switches(&mut self) -> [(&'static str, &mut bool); 12] {
        [
            ("Endpoint", &mut self.endpoint),
            ("Midpoint", &mut self.midpoint),
            ("Intersection", &mut self.intersection),
            ("Perpendicular", &mut self.perpendicular),
            ("Nearest on edge", &mut self.nearest),
            ("Centre", &mut self.center),
            ("Quadrant", &mut self.quadrant),
            ("Grid and level lines", &mut self.grid_lines),
            ("Guides", &mut self.guides),
            ("X/Y axis tracking", &mut self.axis),
            ("Alignment with the shape's points", &mut self.tracking),
            ("5 cm increment", &mut self.increment),
        ]
    }
}

/// The snap switch and a drop-down of the kinds of snap, for a toolbar.
pub fn snap_bar(ui: &mut egui::Ui, s: &mut SnapSettings) {
    ui.toggle_value(&mut s.on, "Snap").on_hover_text(
        "Object snaps on or off (F3). Hold Alt while picking to suspend them for a moment.",
    );
    let mut all = *s;
    let n = all.switches().iter().filter(|(_, v)| **v).count();
    let total = all.switches().len();
    let text = match n {
        0 => "None".to_string(),
        n if n == total => "All".to_string(),
        n => format!("{n} of {total}"),
    };
    ui.add_enabled_ui(s.on, |ui| {
        egui::ComboBox::from_id_salt("snap_kinds")
            .selected_text(text)
            .width(80.0)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show_ui(ui, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("All").clicked() {
                        s.switches().into_iter().for_each(|(_, v)| *v = true);
                    }
                    if ui.button("None").clicked() {
                        s.switches().into_iter().for_each(|(_, v)| *v = false);
                    }
                });
                ui.separator();
                for (label, v) in s.switches() {
                    ui.checkbox(v, label);
                }
            })
            .response
            .on_hover_text("Which snaps are on");
    });
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Snap {
    pub p: DVec3,
    pub kind: SnapKind,
    /// The element the snap lies on, if any.
    pub host: Option<ElementId>,
}

/// What an alignment line runs through.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Through {
    /// The point the line being drawn starts from: its X and Y axes.
    From,
    /// The first point of the shape being drawn.
    Start,
    /// Another point of the shape, numbered from 1.
    Point(usize),
    /// A point the pointer rested on while drawing.
    Rested,
    /// An edge the alignment crosses.
    Edge,
}

/// One line a snapped point is held to: horizontal (along X) or vertical
/// (along Y) through `at`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Line {
    pub at: DVec3,
    /// Along X (horizontal in a plan) or along Y.
    pub along_x: bool,
    pub through: Through,
}

impl Line {
    fn axis(&self) -> DVec3 {
        if self.along_x {
            DVec3::X
        } else {
            DVec3::Y
        }
    }

    /// The point of the line nearest `p`, at the line's height.
    fn foot(&self, p: DVec3) -> DVec3 {
        self.at + self.axis() * (p - self.at).dot(self.axis())
    }

    fn text(&self) -> String {
        let dir = if self.along_x { "X" } else { "Y" };
        match self.through {
            Through::From => format!("on {dir} axis"),
            Through::Start => "in line with the start".into(),
            Through::Point(n) => format!("in line with point {n}"),
            Through::Rested => "in line with a point".into(),
            Through::Edge => "on edge".into(),
        }
    }
}

/// The alignments a tracked snap follows: one line, or two that cross.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Alignment {
    pub lines: [Option<Line>; 2],
}

impl Alignment {
    /// What the snap reads as: "On Y axis · in line with the start".
    pub fn label(&self) -> String {
        let parts: Vec<String> = self.lines.iter().flatten().map(Line::text).collect();
        let mut text = parts.join(" · ");
        if let Some(c) = text.get(0..1) {
            text = c.to_uppercase() + &text[1..];
        }
        text
    }
}

/// Points of the shape being drawn that the next point can line up with,
/// with what each is, for [`Snapper::snap_tracked`].
pub fn track_points(shape: &[DVec3], rested: &[DVec3]) -> Vec<(DVec3, Through)> {
    let mut out: Vec<(DVec3, Through)> = vec![];
    for (i, p) in shape.iter().enumerate() {
        let through = if i == 0 {
            Through::Start
        } else {
            Through::Point(i + 1)
        };
        out.push((*p, through));
    }
    for p in rested {
        if !out.iter().any(|(q, _)| q.distance(*p) < 1e-6) {
            out.push((*p, Through::Rested));
        }
    }
    out
}

/// Where snap geometry came from, so whole families can be turned off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Edge,
    /// An endless construction guide.
    Guide,
    /// A grid line, or a level line in a section.
    GridLine,
}

impl Source {
    fn edge_kind(self) -> SnapKind {
        match self {
            Source::Edge => SnapKind::OnEdge,
            Source::Guide => SnapKind::Guide,
            Source::GridLine => SnapKind::OnGridLine,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Segment {
    a: DVec3,
    b: DVec3,
    src: Source,
    host: Option<ElementId>,
}

#[derive(Clone, Copy, Debug)]
struct Point {
    p: DVec3,
    kind: SnapKind,
    /// The sources it comes from (two for a crossing); all must be wanted.
    src: [Source; 2],
    host: Option<ElementId>,
}

/// Snap geometry for one pane.
#[derive(Clone, Debug, Default)]
pub struct Snapper {
    segments: Vec<Segment>,
    /// Corner, middle and centre candidates.
    points: Vec<Point>,
    /// Distance between grid points; 0 turns the grid off.
    pub grid: f64,
}

/// Pixels within which a snap takes over the pointer.
pub const TOLERANCE: f32 = 10.0;

impl Snapper {
    pub fn new() -> Self {
        Self {
            grid: 0.05,
            ..Self::default()
        }
    }

    fn point(&mut self, p: DVec3, kind: SnapKind, src: Source, host: Option<ElementId>) {
        self.points.push(Point {
            p,
            kind,
            src: [src; 2],
            host,
        });
    }

    /// An edge, with its ends and middle; a guide is only a line.
    pub fn add_segment(&mut self, a: DVec3, b: DVec3, src: Source, host: Option<ElementId>) {
        if a.distance(b) < 1e-6 {
            return;
        }
        self.segments.push(Segment { a, b, src, host });
        if src != Source::Guide {
            self.point(a, SnapKind::Endpoint, src, host);
            self.point(b, SnapKind::Endpoint, src, host);
            self.point((a + b) / 2.0, SnapKind::Midpoint, src, host);
        }
    }

    /// A curve given as a polyline: its pieces snap as edges, but only its
    /// ends and halfway point as points, plus `center` and `quadrants` for
    /// an arc.
    pub fn add_curve(
        &mut self,
        pts: &[DVec3],
        center: Option<DVec3>,
        quadrants: &[DVec3],
        host: Option<ElementId>,
    ) {
        let Some((first, last)) = pts.first().zip(pts.last()) else {
            return;
        };
        for w in pts.windows(2) {
            if w[0].distance(w[1]) > 1e-9 {
                self.segments.push(Segment {
                    a: w[0],
                    b: w[1],
                    src: Source::Edge,
                    host,
                });
            }
        }
        self.point(*first, SnapKind::Endpoint, Source::Edge, host);
        if first.distance(*last) > 1e-9 {
            self.point(*last, SnapKind::Endpoint, Source::Edge, host);
        }
        self.point(pts[pts.len() / 2], SnapKind::Midpoint, Source::Edge, host);
        if let Some(c) = center {
            self.point(c, SnapKind::Center, Source::Edge, host);
        }
        for q in quadrants {
            self.point(*q, SnapKind::Quadrant, Source::Edge, host);
        }
    }

    /// Every sketch's lines and arcs, in model coordinates, for the 3D view.
    pub fn add_sketches(&mut self, model: &creabim_core::Model, exclude: &[ElementId]) {
        for e in model.iter() {
            let creabim_core::ElementKind::Sketch(sk) = &e.kind else {
                continue;
            };
            if exclude.contains(&e.id) {
                continue;
            }
            let z = sk
                .level
                .and_then(|l| model.level(l))
                .map_or(0.0, |l| l.elevation);
            let at = |p: DVec2| sk.plane.to_model(p) + DVec3::Z * z;
            for c in sk.curves() {
                let pts: Vec<DVec3> = c.points().into_iter().map(at).collect();
                match c {
                    creabim_core::Curve::Line(a, b) => {
                        self.add_segment(at(a), at(b), Source::Edge, Some(e.id))
                    }
                    creabim_core::Curve::Arc {
                        center,
                        radius,
                        start,
                        sweep,
                    } => {
                        let (lo, hi) = (start.min(start + sweep), start.max(start + sweep));
                        let quads: Vec<DVec3> = quadrant_angles(lo, hi)
                            .map(|t| at(center + DVec2::from_angle(t) * radius))
                            .collect();
                        self.add_curve(&pts, Some(at(center)), &quads, Some(e.id))
                    }
                }
            }
        }
    }

    /// Where segments cross. Only meaningful for flat drawings; the 3D view
    /// snaps to corners and edges instead.
    pub fn add_intersections(&mut self) {
        let flat = |p: DVec3| p.truncate();
        let n = self.segments.len().min(4000);
        let mut out = Vec::new();
        for i in 0..n {
            let s = self.segments[i];
            for j in i + 1..n {
                let t = self.segments[j];
                let (ga, gb) = (s.src == Source::Guide, t.src == Source::Guide);
                if let Some(p) = crossing(flat(s.a), flat(s.b), flat(t.a), flat(t.b), ga, gb) {
                    // the element rather than the grid or guide it crosses
                    let host = if s.src == Source::Edge || t.src != Source::Edge {
                        s.host.or(t.host)
                    } else {
                        t.host.or(s.host)
                    };
                    out.push(Point {
                        p: p.extend(0.0),
                        kind: SnapKind::Intersection,
                        src: [s.src, t.src],
                        host,
                    });
                }
            }
        }
        self.points.extend(out);
    }

    /// Snaps from a plan or section drawing: every edge, arc and grid or
    /// guide line in it, whatever element drew it (walls, slabs, masses,
    /// columns, beams, sketches, grids, levels), and where they cross.
    /// `exclude`'s linework is left out (an element being dragged must not
    /// snap to itself).
    pub fn from_drawing(items: &[PlanItem], exclude: &[ElementId]) -> Self {
        use creabim_geom::plan::Pen;
        let mut s = Self::new();
        let p3 = |p: DVec2| p.extend(0.0);
        for it in items {
            let id = it.id();
            if exclude.contains(&id) {
                continue;
            }
            match it {
                PlanItem::Line { a, b, pen, .. } => {
                    let src = match pen {
                        Pen::Guide => Source::Guide,
                        Pen::Grid => Source::GridLine,
                        _ => Source::Edge,
                    };
                    s.add_segment(p3(*a), p3(*b), src, Some(id))
                }
                PlanItem::Polygon { points, .. } => {
                    for i in 0..points.len() {
                        let j = (i + 1) % points.len();
                        s.add_segment(p3(points[i]), p3(points[j]), Source::Edge, Some(id));
                    }
                }
                PlanItem::Arc {
                    center,
                    radius,
                    start,
                    end,
                    ..
                } => {
                    let n = (((end - start).abs() / creabim_core::sketch::ARC_STEP).ceil()
                        as usize)
                        .max(2);
                    let on = |t: f64| p3(*center + DVec2::from_angle(t) * *radius);
                    let pts: Vec<DVec3> = (0..=n)
                        .map(|i| on(start + (end - start) * i as f64 / n as f64))
                        .collect();
                    let quads: Vec<DVec3> = quadrant_angles(*start, *end).map(on).collect();
                    s.add_curve(&pts, Some(p3(*center)), &quads, Some(id));
                }
                _ => {}
            }
        }
        s.add_intersections();
        s
    }

    /// Snaps from the 3D scene: the corners, creases and face centres of
    /// every mesh (walls, slabs, masses, columns, beams), not the
    /// triangulation's inner diagonals.
    pub fn from_scene(scene: &[SceneItem], exclude: &[ElementId]) -> Self {
        let mut s = Self::new();
        for item in scene {
            if exclude.contains(&item.id) {
                continue;
            }
            for (a, b) in feature_edges(&item.mesh) {
                s.add_segment(a, b, Source::Edge, Some(item.id));
            }
            for c in face_centres(&item.mesh) {
                s.point(c, SnapKind::Center, Source::Edge, Some(item.id));
            }
        }
        s
    }

    /// Every guide in the model, for the 3D view, `reach` metres each way.
    pub fn add_guides(&mut self, model: &creabim_core::Model, reach: f64) {
        for e in model.iter() {
            if let creabim_core::ElementKind::Guide(g) = &e.kind {
                if let Some(d) = (g.b - g.a).try_normalize() {
                    let m = (g.a + g.b) / 2.0;
                    self.add_segment(m - d * reach, m + d * reach, Source::Guide, Some(e.id));
                }
            }
        }
    }

    /// Every grid line, for the 3D view, on each level (or the ground when
    /// there are none), with where grid lines cross.
    /// Lines in `skip` (hidden in the view) are left out.
    pub fn add_grids(
        &mut self,
        model: &creabim_core::Model,
        skip: &std::collections::HashSet<ElementId>,
    ) {
        let grids: Vec<(ElementId, DVec2, DVec2)> = model
            .iter()
            .filter(|e| !skip.contains(&e.id))
            .filter_map(|e| match &e.kind {
                creabim_core::ElementKind::Grid(g) => Some((e.id, g.start, g.end)),
                _ => None,
            })
            .collect();
        let mut heights: Vec<f64> = model.levels().map(|(_, l)| l.elevation).collect();
        if heights.is_empty() {
            heights.push(0.0);
        }
        for z in heights {
            for (i, &(id, a, b)) in grids.iter().enumerate() {
                self.add_segment(a.extend(z), b.extend(z), Source::GridLine, Some(id));
                for &(_, c, d) in &grids[i + 1..] {
                    if let Some(p) = crossing(a, b, c, d, false, false) {
                        self.points.push(Point {
                            p: p.extend(z),
                            kind: SnapKind::Intersection,
                            src: [Source::GridLine; 2],
                            host: Some(id),
                        });
                    }
                }
            }
        }
    }

    /// The edge or guide that `p` lies on, if exactly one does: what a
    /// parallel guide is drawn from.
    pub fn edge_at(&self, p: DVec3) -> Option<(DVec3, DVec3)> {
        let mut on = self
            .segments
            .iter()
            .filter(|s| closest_on_segment(s.a, s.b, p).distance(p) < 1e-6)
            .map(|s| (s.a, s.b));
        let first = on.next()?;
        on.next().is_none().then_some(first)
    }

    /// The best snap for the pointer at `cursor`, of the kinds `cfg` allows.
    /// `from` is where the line being drawn started, for perpendicular and
    /// axis snaps; `free` is the unsnapped point under the pointer, used for
    /// the increment and returned as is when snapping is off.
    pub fn snap(
        &self,
        cfg: &SnapSettings,
        cursor: Pos2,
        project: &dyn Fn(DVec3) -> Option<Pos2>,
        from: Option<DVec3>,
        free: Option<DVec3>,
    ) -> Option<Snap> {
        self.snap_tracked(cfg, cursor, project, from, &[], free)
            .map(|(s, _)| s)
    }

    /// Like [`Snapper::snap`], also lining the point up with `track`: the
    /// other points of the shape being drawn (Revit's tracking, SketchUp's
    /// inference). Horizontal and vertical lines through each are snapped
    /// to, as are the X and Y axes through `from`; where two of those lines
    /// cross, or one crosses an edge, is a snap point of its own. Point
    /// snaps still come first. The alignment followed, if any, is returned
    /// to draw.
    pub fn snap_tracked(
        &self,
        cfg: &SnapSettings,
        cursor: Pos2,
        project: &dyn Fn(DVec3) -> Option<Pos2>,
        from: Option<DVec3>,
        track: &[(DVec3, Through)],
        free: Option<DVec3>,
    ) -> Option<(Snap, Option<Alignment>)> {
        let near = |p: DVec3| {
            project(p)
                .map(|s| s.distance(cursor))
                .filter(|d| *d < TOLERANCE)
        };
        // points beat edges: the nearest point within tolerance, by priority
        let rank = |k: SnapKind| match k {
            SnapKind::Endpoint => 0.0,
            SnapKind::Intersection => 0.5,
            SnapKind::Midpoint | SnapKind::Center | SnapKind::Quadrant => 1.0,
            _ => 2.0,
        };
        let best_point = self
            .points
            .iter()
            .filter(|q| cfg.allows(q.kind) && q.src.iter().all(|s| cfg.takes(*s)))
            .filter_map(|q| near(q.p).map(|d| (d + rank(q.kind) * 2.0, q)))
            .min_by(|a, b| a.0.total_cmp(&b.0));
        if let Some((_, q)) = best_point {
            return Some((
                Snap {
                    p: q.p,
                    kind: q.kind,
                    host: q.host,
                },
                None,
            ));
        }
        let segments = || self.segments.iter().filter(|s| cfg.takes(s.src));
        // the alignment lines the pointer is near: the axes through the
        // start of the line, then through each point of the shape
        let mut lines: Vec<(f32, Line)> = vec![];
        if let Some(free) = free {
            let mut all: Vec<Line> = vec![];
            if let (Some(f), true) = (from, cfg.allows(SnapKind::AxisX)) {
                for along_x in [true, false] {
                    all.push(Line {
                        at: f,
                        along_x,
                        through: Through::From,
                    });
                }
            }
            if cfg.allows(SnapKind::Aligned) {
                for (p, through) in track {
                    if from.is_some_and(|f| f.distance(*p) < 1e-6) {
                        continue;
                    }
                    for along_x in [true, false] {
                        all.push(Line {
                            at: *p,
                            along_x,
                            through: *through,
                        });
                    }
                }
            }
            for l in all {
                let foot = l.foot(free);
                if let (Some(sp), Some(sf)) = (project(foot), project(free)) {
                    let d = sp.distance(sf);
                    if d < TOLERANCE && foot.distance(l.at) > 1e-3 {
                        lines.push((d, l));
                    }
                }
            }
        }
        // two alignments that cross near the pointer: one along X, one
        // along Y, through different points at one height
        if cfg.allows(SnapKind::AlignedCross) {
            let mut best: Option<(f32, DVec3, Line, Line)> = None;
            for (_, a) in lines.iter().filter(|(_, l)| l.along_x) {
                for (_, b) in lines.iter().filter(|(_, l)| !l.along_x) {
                    if a.at.distance(b.at) < 1e-6 || (a.at.z - b.at.z).abs() > 1e-6 {
                        continue;
                    }
                    if a.through == Through::From && b.through == Through::From {
                        continue;
                    }
                    let p = DVec3::new(b.at.x, a.at.y, a.at.z);
                    if let Some(d) = near(p) {
                        if best.is_none_or(|(bd, ..)| d < bd) {
                            best = Some((d, p, *a, *b));
                        }
                    }
                }
            }
            if let Some((_, p, a, b)) = best {
                return Some((
                    Snap {
                        p,
                        kind: SnapKind::AlignedCross,
                        host: None,
                    },
                    Some(Alignment {
                        lines: [Some(b), Some(a)],
                    }),
                ));
            }
        }
        if let (Some(f), true) = (from, cfg.allows(SnapKind::Perpendicular)) {
            for s in segments() {
                let foot = closest_on_segment(s.a, s.b, f);
                if foot.distance(f) > 1e-6 && (foot - s.a).length() > 1e-6 && near(foot).is_some() {
                    let along = (s.b - s.a).normalize();
                    if (foot - f).normalize().dot(along).abs() < 1e-6 {
                        return Some((
                            Snap {
                                p: foot,
                                kind: SnapKind::Perpendicular,
                                host: s.host,
                            },
                            None,
                        ));
                    }
                }
            }
        }
        // the nearest point on an edge, in screen space
        let mut best: Option<(f32, Snap)> = None;
        for s in segments() {
            let kind = s.src.edge_kind();
            if !cfg.allows(kind) {
                continue;
            }
            let (Some(sa), Some(sb)) = (project(s.a), project(s.b)) else {
                continue;
            };
            let t = screen_t(sa, sb, cursor);
            let on = sa + (sb - sa) * t;
            let d = on.distance(cursor);
            if d < TOLERANCE && best.is_none_or(|(bd, _)| d < bd) {
                let p = s.a + (s.b - s.a) * t as f64;
                best = Some((
                    d,
                    Snap {
                        p,
                        kind,
                        host: s.host,
                    },
                ));
            }
        }
        if let Some((_, s)) = best {
            // an edge an alignment crosses there: the point where it does
            if cfg.allows(SnapKind::AlignedCross) && s.kind == SnapKind::OnEdge {
                let hit = lines
                    .iter()
                    .filter_map(|(_, l)| {
                        let seg = self.segments.iter().find(|g| {
                            g.src == Source::Edge
                                && closest_on_segment(g.a, g.b, s.p).distance(s.p) < 1e-6
                        })?;
                        let (a, b) = (l.at.truncate(), (l.at + l.axis()).truncate());
                        let p = crossing(a, b, seg.a.truncate(), seg.b.truncate(), true, false)?;
                        let p = p.extend(l.at.z);
                        (closest_on_segment(seg.a, seg.b, p).distance(p) < 1e-6).then_some(())?;
                        Some((near(p)?, p, *l, seg.host))
                    })
                    .min_by(|a, b| a.0.total_cmp(&b.0));
                if let Some((_, p, l, host)) = hit {
                    let edge = Line {
                        at: p,
                        along_x: !l.along_x,
                        through: Through::Edge,
                    };
                    return Some((
                        Snap {
                            p,
                            kind: SnapKind::AlignedCross,
                            host,
                        },
                        Some(Alignment {
                            lines: [Some(l), Some(edge)],
                        }),
                    ));
                }
            }
            return Some((s, None));
        }
        let step = cfg.step(self.grid);
        // the nearest single alignment: the axes through the start come
        // first when as near
        let nearest = lines.iter().min_by(|a, b| {
            let bias = |l: &Line| if l.through == Through::From { 0.5 } else { 0.0 };
            (a.0 - bias(&a.1)).total_cmp(&(b.0 - bias(&b.1)))
        });
        if let (Some((_, l)), Some(free)) = (nearest, free) {
            let axis = l.axis();
            let p = match (l.through, from) {
                // a round length from the start
                (Through::From, _) => l.at + axis * grid_round((free - l.at).dot(axis), step),
                // a round distance from where the line starts
                (_, Some(f)) => {
                    let foot = l.foot(free);
                    let along = grid_round((foot - f).dot(axis), step);
                    l.foot(f) + axis * along
                }
                (_, None) => {
                    let foot = l.foot(free);
                    l.at + axis * grid_round((foot - l.at).dot(axis), step)
                }
            };
            let kind = match (l.through, l.along_x) {
                (Through::From, true) => SnapKind::AxisX,
                (Through::From, false) => SnapKind::AxisY,
                _ => SnapKind::Aligned,
            };
            let align = (kind == SnapKind::Aligned).then_some(Alignment {
                lines: [Some(*l), None],
            });
            return Some((
                Snap {
                    p,
                    kind,
                    host: None,
                },
                align,
            ));
        }
        free.map(|p| {
            (
                Snap {
                    p: DVec3::new(grid_round(p.x, step), grid_round(p.y, step), p.z),
                    kind: SnapKind::Grid,
                    host: None,
                },
                None,
            )
        })
    }
}

/// The quarter angles (multiples of 90°) from `lo` to `hi`, radians.
fn quadrant_angles(lo: f64, hi: f64) -> impl Iterator<Item = f64> {
    let q = std::f64::consts::FRAC_PI_2;
    let first = (lo / q - 1e-9).ceil() as i64;
    let last = (hi / q + 1e-9).floor() as i64;
    // a whole circle's end is its start again
    let last = if hi - lo >= std::f64::consts::TAU - 1e-9 {
        last.min(first + 3)
    } else {
        last
    };
    (first..=last).map(move |k| k as f64 * q)
}

fn grid_round(v: f64, step: f64) -> f64 {
    if step > 0.0 {
        (v / step).round() * step
    } else {
        v
    }
}

fn screen_t(a: Pos2, b: Pos2, p: Pos2) -> f32 {
    let ab = b - a;
    let len2 = ab.length_sq();
    if len2 < 1e-6 {
        0.0
    } else {
        ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
    }
}

pub fn closest_on_segment(a: DVec3, b: DVec3, p: DVec3) -> DVec3 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-12)).clamp(0.0, 1.0);
    a + ab * t
}

/// Where segments ab and cd cross, if they do. A guide is endless, so it
/// crosses wherever its line does.
fn crossing(
    a: DVec2,
    b: DVec2,
    c: DVec2,
    d: DVec2,
    ab_endless: bool,
    cd_endless: bool,
) -> Option<DVec2> {
    let r = b - a;
    let s = d - c;
    let den = r.perp_dot(s);
    if den.abs() < 1e-12 {
        return None;
    }
    let t = (c - a).perp_dot(s) / den;
    let u = (c - a).perp_dot(r) / den;
    let inside = |v: f64| (-1e-9..=1.0 + 1e-9).contains(&v);
    // shared corners are already endpoints
    if (!ab_endless && !inside(t)) || (!cd_endless && !inside(u)) {
        return None;
    }
    let p = a + r * t;
    let corner = [a, b, c, d].iter().any(|q| q.distance(p) < 1e-6);
    (!corner).then_some(p)
}

/// A mesh's corners welded (flat-shaded meshes repeat them per face) and
/// its triangles over them, without slivers.
fn weld(mesh: &creabim_geom::mesh::TriMesh) -> (Vec<DVec3>, Vec<[u32; 3]>) {
    let key = |p: [f32; 3]| {
        (
            (p[0] * 1000.0).round() as i64,
            (p[1] * 1000.0).round() as i64,
            (p[2] * 1000.0).round() as i64,
        )
    };
    let mut ids: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut pts: Vec<DVec3> = Vec::new();
    let mut welded = |p: [f32; 3]| -> u32 {
        *ids.entry(key(p)).or_insert_with(|| {
            pts.push(DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64));
            pts.len() as u32 - 1
        })
    };
    let mut tris = Vec::new();
    for t in mesh.indices.chunks(3) {
        let v = [
            welded(mesh.positions[t[0] as usize]),
            welded(mesh.positions[t[1] as usize]),
            welded(mesh.positions[t[2] as usize]),
        ];
        if v[0] != v[1] && v[1] != v[2] && v[0] != v[2] {
            tris.push(v);
        }
    }
    (pts, tris)
}

/// Each triangle's edges keyed by their welded corners, low index first.
fn edge_map(tris: &[[u32; 3]]) -> HashMap<(u32, u32), Vec<usize>> {
    let mut edges: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (i, t) in tris.iter().enumerate() {
        for k in 0..3 {
            let (a, b) = (t[k], t[(k + 1) % 3]);
            edges.entry((a.min(b), a.max(b))).or_default().push(i);
        }
    }
    edges
}

fn tri_normal(pts: &[DVec3], t: &[u32; 3]) -> DVec3 {
    let (a, b, c) = (pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]);
    (b - a).cross(c - a).normalize_or_zero()
}

/// The creases and open edges of a triangle mesh: where neighbouring faces
/// meet at more than about 20°, or where a face has no neighbour.
pub fn feature_edges(mesh: &creabim_geom::mesh::TriMesh) -> Vec<(DVec3, DVec3)> {
    let (pts, tris) = weld(mesh);
    let normals: Vec<DVec3> = tris.iter().map(|t| tri_normal(&pts, t)).collect();
    let mut out: Vec<(DVec3, DVec3)> = edge_map(&tris)
        .into_iter()
        .filter(|(_, ts)| {
            let n0 = normals[ts[0]];
            ts.len() == 1 || ts.iter().any(|t| normals[*t].dot(n0) < 0.94)
        })
        .map(|((a, b), _)| (pts[a as usize], pts[b as usize]))
        .collect();
    out.sort_by(|a, b| {
        (a.0.x, a.0.y, a.0.z)
            .partial_cmp(&(b.0.x, b.0.y, b.0.z))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// The centre of each flat face of a triangle mesh: triangles that share an
/// edge and lie in one plane make one face, and its centre is their
/// area-weighted middle.
pub fn face_centres(mesh: &creabim_geom::mesh::TriMesh) -> Vec<DVec3> {
    let (pts, tris) = weld(mesh);
    let normals: Vec<DVec3> = tris.iter().map(|t| tri_normal(&pts, t)).collect();
    let edges = edge_map(&tris);
    let mut seen = vec![false; tris.len()];
    let mut out = Vec::new();
    for start in 0..tris.len() {
        if seen[start] || normals[start] == DVec3::ZERO {
            continue;
        }
        let (n0, p0) = (normals[start], pts[tris[start][0] as usize]);
        let mut stack = vec![start];
        seen[start] = true;
        let (mut area, mut sum) = (0.0, DVec3::ZERO);
        while let Some(i) = stack.pop() {
            let t = tris[i];
            let (a, b, c) = (pts[t[0] as usize], pts[t[1] as usize], pts[t[2] as usize]);
            let w = (b - a).cross(c - a).length() / 2.0;
            area += w;
            sum += (a + b + c) / 3.0 * w;
            for k in 0..3 {
                let (u, v) = (t[k], t[(k + 1) % 3]);
                for &j in &edges[&(u.min(v), u.max(v))] {
                    let q = pts[tris[j][0] as usize];
                    if !seen[j] && normals[j].dot(n0) > 0.9999 && (q - p0).dot(n0).abs() < 1e-4 {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        if area > 1e-9 {
            out.push(sum / area);
        }
    }
    out
}

/// Draw the snap marker at `at`: a square for ends, a triangle for
/// midpoints, a cross for intersections, a right angle for perpendicular
/// feet, a circle for centres, a diamond for quadrants, a small circle on
/// edges. Axis snaps draw their axis from `from`.
pub fn draw(painter: &Painter, snap: Snap, at: Pos2, from: Option<Pos2>) {
    let Some(c) = marker(painter, snap, at, from) else {
        return;
    };
    let label = snap.kind.label();
    if !label.is_empty() {
        painter.text(
            at + egui::vec2(10.0, 10.0),
            egui::Align2::LEFT_TOP,
            label,
            egui::FontId::proportional(12.0),
            c,
        );
    }
}

/// The marker of [`draw`] without its label; its colour, if it drew one.
fn marker(painter: &Painter, snap: Snap, at: Pos2, from: Option<Pos2>) -> Option<Color32> {
    let c = match snap.kind {
        SnapKind::Endpoint | SnapKind::Intersection => Color32::from_rgb(20, 150, 60),
        SnapKind::Midpoint => Color32::from_rgb(20, 120, 200),
        SnapKind::Center | SnapKind::Quadrant => Color32::from_rgb(200, 120, 20),
        SnapKind::Perpendicular => Color32::from_rgb(160, 60, 180),
        SnapKind::OnEdge | SnapKind::Guide | SnapKind::OnGridLine => Color32::from_rgb(200, 40, 40),
        SnapKind::AxisX => Color32::from_rgb(220, 50, 50),
        SnapKind::AxisY => Color32::from_rgb(40, 160, 60),
        SnapKind::Aligned | SnapKind::AlignedCross => TRACK,
        SnapKind::Grid => return None,
    };
    let s = Stroke::new(2.0, c);
    let r = 6.0;
    match snap.kind {
        SnapKind::Endpoint => {
            painter.rect_stroke(
                egui::Rect::from_center_size(at, egui::Vec2::splat(r * 2.0)),
                0.0,
                s,
                egui::StrokeKind::Middle,
            );
        }
        SnapKind::Midpoint => {
            painter.add(egui::Shape::closed_line(
                vec![
                    at + egui::vec2(0.0, -r),
                    at + egui::vec2(r, r * 0.8),
                    at + egui::vec2(-r, r * 0.8),
                ],
                s,
            ));
        }
        SnapKind::Center => {
            painter.circle_stroke(at, r, s);
            painter.circle_filled(at, 1.5, c);
        }
        SnapKind::Quadrant => {
            painter.add(egui::Shape::closed_line(
                vec![
                    at + egui::vec2(0.0, -r),
                    at + egui::vec2(r, 0.0),
                    at + egui::vec2(0.0, r),
                    at + egui::vec2(-r, 0.0),
                ],
                s,
            ));
        }
        SnapKind::Intersection => {
            painter.line_segment([at + egui::vec2(-r, -r), at + egui::vec2(r, r)], s);
            painter.line_segment([at + egui::vec2(-r, r), at + egui::vec2(r, -r)], s);
        }
        SnapKind::Perpendicular => {
            painter.line_segment([at + egui::vec2(-r, r), at + egui::vec2(r, r)], s);
            painter.line_segment([at + egui::vec2(-r, r), at + egui::vec2(-r, -r)], s);
            painter.line_segment([at + egui::vec2(-r, 0.0), at + egui::vec2(0.0, 0.0)], s);
            painter.line_segment([at + egui::vec2(0.0, 0.0), at + egui::vec2(0.0, r)], s);
        }
        SnapKind::OnEdge | SnapKind::Guide | SnapKind::OnGridLine => {
            painter.circle_stroke(at, r * 0.7, s);
        }
        SnapKind::Aligned => {
            painter.circle_filled(at, 3.0, c);
        }
        SnapKind::AlignedCross => {
            painter.line_segment([at + egui::vec2(-r, -r), at + egui::vec2(r, r)], s);
            painter.line_segment([at + egui::vec2(-r, r), at + egui::vec2(r, -r)], s);
            painter.circle_stroke(at, r * 0.75, Stroke::new(1.5, c));
        }
        SnapKind::AxisX | SnapKind::AxisY => {
            if let Some(f) = from {
                painter.add(egui::Shape::dashed_line(
                    &[f, at],
                    Stroke::new(1.5, c),
                    6.0,
                    4.0,
                ));
            }
            painter.circle_filled(at, 3.0, c);
        }
        SnapKind::Grid => {}
    }
    Some(c)
}

/// The colour of alignment tracking: its dashed lines, marker and label.
const TRACK: Color32 = Color32::from_rgb(214, 96, 20);

/// Draw a snap with the alignment it follows, if any: a dashed line from
/// each point it lines up with to the snapped point, a small cross on that
/// point, and the alignment's label instead of the snap's.
pub fn draw_tracked(
    painter: &Painter,
    project: &dyn Fn(DVec3) -> Option<Pos2>,
    snap: Snap,
    align: Option<Alignment>,
    at: Pos2,
    from: Option<Pos2>,
) {
    // (an axis from the start is one of the alignment's lines, drawn from it)
    let Some(align) = align else {
        return draw(painter, snap, at, from);
    };
    let line = Stroke::new(1.2, TRACK);
    for l in align.lines.iter().flatten() {
        if l.through == Through::Edge {
            continue;
        }
        let Some(a) = project(l.at) else { continue };
        // a little past the snapped point, so the line reads as running on
        let d = (at - a).normalized();
        painter.add(egui::Shape::dashed_line(
            &[a, at + d * 14.0],
            line,
            5.0,
            4.0,
        ));
        let k = 3.5;
        painter.line_segment([a + egui::vec2(-k, -k), a + egui::vec2(k, k)], line);
        painter.line_segment([a + egui::vec2(-k, k), a + egui::vec2(k, -k)], line);
    }
    marker(painter, snap, at, None);
    // the label says what it lines up with
    let label = align.label();
    let galley = painter.layout_no_wrap(label, egui::FontId::proportional(12.0), TRACK);
    // below right of the point, or left of it near the pane's right edge
    let clip = painter.clip_rect();
    let mut pos = at + egui::vec2(12.0, 10.0);
    if pos.x + galley.size().x > clip.right() - 4.0 {
        pos.x = at.x - 12.0 - galley.size().x;
    }
    if pos.y + galley.size().y > clip.bottom() - 4.0 {
        pos.y = at.y - 10.0 - galley.size().y;
    }
    let bg = egui::Rect::from_min_size(pos, galley.size()).expand(2.0);
    painter.rect_filled(bg, 3.0, Color32::from_rgba_unmultiplied(255, 255, 255, 200));
    painter.galley(pos, galley, TRACK);
}

#[cfg(test)]
mod tests {
    use super::*;
    use creabim_core::{EditMesh, Element, ElementKind, FreeMesh, Transaction};

    fn flat(p: DVec3) -> Option<Pos2> {
        Some(Pos2::new((p.x * 100.0) as f32, (-p.y * 100.0) as f32))
    }

    fn at(x: f64, y: f64) -> Pos2 {
        flat(DVec3::new(x, y, 0.0)).unwrap()
    }

    const ALL: SnapSettings = SnapSettings::ALL;

    #[test]
    fn corners_beat_edges_and_midpoints_are_found() {
        let mut s = Snapper::new();
        s.add_segment(DVec3::ZERO, DVec3::new(4.0, 0.0, 0.0), Source::Edge, None);
        s.add_segment(
            DVec3::new(2.0, -1.0, 0.0),
            DVec3::new(2.0, 1.0, 0.0),
            Source::Edge,
            None,
        );
        s.add_intersections();
        let hit = s.snap(&ALL, at(0.03, 0.02), &flat, None, None).unwrap();
        assert_eq!(hit.kind, SnapKind::Endpoint);
        let hit = s.snap(&ALL, at(2.02, 0.03), &flat, None, None).unwrap();
        assert_eq!(hit.kind, SnapKind::Intersection);
        let hit = s.snap(&ALL, at(3.0, 0.04), &flat, None, None).unwrap();
        assert_eq!(hit.kind, SnapKind::OnEdge);
        assert!((hit.p.y).abs() < 1e-9);
    }

    #[test]
    fn arcs_snap_to_their_ends_middle_centre_and_quadrants() {
        let items = [PlanItem::Arc {
            id: ElementId::new(),
            pen: creabim_geom::plan::Pen::Symbol,
            center: DVec2::ZERO,
            radius: 2.0,
            start: 0.0,
            end: std::f64::consts::PI,
        }];
        let s = Snapper::from_drawing(&items, &[]);
        let hit = s.snap(&ALL, at(0.02, 0.01), &flat, None, None).unwrap();
        assert_eq!(hit.kind, SnapKind::Center);
        assert_eq!(hit.host, Some(items[0].id()));
        let hit = s.snap(&ALL, at(0.01, 2.02), &flat, None, None).unwrap();
        assert_eq!(hit.kind, SnapKind::Midpoint);
        // with midpoints off the top is still a quadrant
        let quads = SnapSettings {
            midpoint: false,
            ..ALL
        };
        let hit = s.snap(&quads, at(0.01, 2.02), &flat, None, None).unwrap();
        assert_eq!(hit.kind, SnapKind::Quadrant);
        // between pieces it is on the edge, not at a piece's end
        let a = 0.3f64;
        let hit = s
            .snap(&ALL, at(2.0 * a.cos(), 2.0 * a.sin()), &flat, None, None)
            .unwrap();
        assert_eq!(hit.kind, SnapKind::OnEdge);
        assert!((hit.p.length() - 2.0).abs() < 0.01);
    }

    #[test]
    fn a_line_snaps_square_to_the_axis_it_started_from() {
        let s = Snapper::new();
        let from = DVec3::new(1.0, 1.0, 0.0);
        let free = DVec3::new(3.013, 1.03, 0.0);
        let hit = s
            .snap(&ALL, flat(free).unwrap(), &flat, Some(from), Some(free))
            .unwrap();
        assert_eq!(hit.kind, SnapKind::AxisX);
        assert!((hit.p.y - 1.0).abs() < 1e-9 && (hit.p.x - 3.0).abs() < 1e-9);
        // without axis tracking it is the free point on the increment
        let no_axis = SnapSettings { axis: false, ..ALL };
        let hit = s
            .snap(&no_axis, flat(free).unwrap(), &flat, Some(from), Some(free))
            .unwrap();
        assert_eq!(hit.kind, SnapKind::Grid);
        assert!((hit.p.x - 3.0).abs() < 1e-9 && (hit.p.y - 1.05).abs() < 1e-9);
    }

    #[test]
    fn the_closing_corner_lines_up_with_the_start_and_the_last_point() {
        // an outline begun at (0, 0), now at (4, 3): the pointer near (4, 0)
        let s = Snapper::new();
        let shape = [DVec3::ZERO, DVec3::new(2.0, 3.0, 0.0)];
        let from = DVec3::new(4.0, 3.0, 0.0);
        let mut pts = shape.to_vec();
        pts.push(from);
        let track = track_points(&pts, &[]);
        let free = DVec3::new(4.03, 0.04, 0.0);
        let (hit, align) = s
            .snap_tracked(
                &ALL,
                flat(free).unwrap(),
                &flat,
                Some(from),
                &track,
                Some(free),
            )
            .unwrap();
        assert_eq!(hit.kind, SnapKind::AlignedCross);
        assert!(hit.p.distance(DVec3::new(4.0, 0.0, 0.0)) < 1e-9);
        let label = align.unwrap().label();
        assert!(
            label.contains("Y axis") && label.contains("start"),
            "{label}"
        );
        // further up the Y axis only the axis holds it
        let free = DVec3::new(4.02, 1.51, 0.0);
        let (hit, _) = s
            .snap_tracked(
                &ALL,
                flat(free).unwrap(),
                &flat,
                Some(from),
                &track,
                Some(free),
            )
            .unwrap();
        assert_eq!(hit.kind, SnapKind::AxisY);
        // in line with an earlier point, away from the axes
        let free = DVec3::new(2.04, -1.33, 0.0);
        let (hit, align) = s
            .snap_tracked(
                &ALL,
                flat(free).unwrap(),
                &flat,
                Some(from),
                &track,
                Some(free),
            )
            .unwrap();
        assert_eq!(hit.kind, SnapKind::Aligned);
        assert!((hit.p.x - 2.0).abs() < 1e-9);
        assert!(align.unwrap().label().contains("point 2"));
        // the tracking switch turns it off, leaving the free point
        let off = SnapSettings {
            tracking: false,
            ..ALL
        };
        let (hit, align) = s
            .snap_tracked(
                &off,
                flat(free).unwrap(),
                &flat,
                Some(from),
                &track,
                Some(free),
            )
            .unwrap();
        assert_eq!(hit.kind, SnapKind::Grid);
        assert!(align.is_none());
    }

    #[test]
    fn an_alignment_crossing_an_edge_snaps_where_it_crosses() {
        let mut s = Snapper::new();
        // a wall face along y = 5 from x 0 to 10
        s.add_segment(
            DVec3::new(0.0, 5.0, 0.0),
            DVec3::new(10.0, 5.0, 0.0),
            Source::Edge,
            None,
        );
        let track = track_points(&[DVec3::new(3.0, 0.0, 0.0)], &[]);
        let from = DVec3::new(8.0, 1.0, 0.0);
        let free = DVec3::new(3.04, 5.02, 0.0);
        let (hit, _) = s
            .snap_tracked(
                &ALL,
                flat(free).unwrap(),
                &flat,
                Some(from),
                &track,
                Some(free),
            )
            .unwrap();
        assert_eq!(hit.kind, SnapKind::AlignedCross);
        assert!(hit.p.distance(DVec3::new(3.0, 5.0, 0.0)) < 1e-9);
        // point snaps still come first
        let free = DVec3::new(0.03, 5.02, 0.0);
        let hit = s
            .snap_tracked(
                &ALL,
                flat(free).unwrap(),
                &flat,
                Some(from),
                &track,
                Some(free),
            )
            .unwrap()
            .0;
        assert_eq!(hit.kind, SnapKind::Endpoint);
    }

    /// A model with one 2 × 4 m mass whose corners are at x 19..21, y 3..7.
    fn mass_model() -> (creabim_core::Model, ElementId, ElementId) {
        let mut model = creabim_core::sample::sample_house();
        let level = model
            .levels()
            .find(|(_, l)| l.elevation == 0.0)
            .unwrap()
            .0
            .id;
        let mass = Element::new(
            "Mass",
            ElementKind::FreeMesh(FreeMesh::new(
                Some(level),
                DVec3::new(20.0, 5.0, 0.0),
                EditMesh::cuboid(DVec3::new(2.0, 4.0, 3.0)),
            )),
        );
        let id = mass.id;
        Transaction::new("Add mass")
            .insert(mass)
            .apply(&mut model)
            .unwrap();
        let plan = model
            .iter()
            .find(|e| match &e.kind {
                ElementKind::View(v) => {
                    matches!(v.kind, creabim_core::ViewKind::Plan { level: l, .. } if l == level)
                }
                _ => false,
            })
            .unwrap()
            .id;
        (model, id, plan)
    }

    #[test]
    fn a_mass_corner_snaps_in_a_plan() {
        let (model, mass, plan) = mass_model();
        let drawing = creabim_geom::section::view_drawing(&model, plan).unwrap();
        let s = Snapper::from_drawing(&drawing.items, &[]);
        let hit = s.snap(&ALL, at(21.03, 7.02), &flat, None, None).unwrap();
        assert_eq!(hit.kind, SnapKind::Endpoint);
        assert!(hit.p.truncate().distance(DVec2::new(21.0, 7.0)) < 1e-9);
        assert_eq!(hit.host, Some(mass));
        // the middle of its long side
        let hit = s.snap(&ALL, at(21.02, 5.01), &flat, None, None).unwrap();
        assert_eq!(hit.kind, SnapKind::Midpoint);
        // a mass being dragged doesn't snap to itself
        let s = Snapper::from_drawing(&drawing.items, &[mass]);
        let hit = s.snap(&ALL, at(21.03, 7.02), &flat, None, None);
        assert!(hit.is_none_or(|h| h.host != Some(mass)));
    }

    #[test]
    fn turning_endpoints_off_keeps_midpoints() {
        let mut s = Snapper::new();
        s.add_segment(DVec3::ZERO, DVec3::new(2.0, 0.0, 0.0), Source::Edge, None);
        let no_ends = SnapSettings {
            endpoint: false,
            ..ALL
        };
        // near the end: on the edge, not the end
        let hit = s.snap(&no_ends, at(0.02, 0.03), &flat, None, None).unwrap();
        assert_ne!(hit.kind, SnapKind::Endpoint);
        // the middle still snaps
        let hit = s.snap(&no_ends, at(1.02, 0.03), &flat, None, None).unwrap();
        assert_eq!(hit.kind, SnapKind::Midpoint);
        assert!(hit.p.distance(DVec3::new(1.0, 0.0, 0.0)) < 1e-9);
        // and with edges off too, nothing but the free point is left
        let bare = SnapSettings {
            nearest: false,
            ..no_ends
        };
        let hit = s.snap(&bare, at(0.02, 0.03), &flat, None, None);
        assert!(hit.is_none());
    }

    #[test]
    fn master_off_gives_no_snap() {
        let mut s = Snapper::new();
        s.add_segment(DVec3::ZERO, DVec3::new(2.0, 0.0, 0.0), Source::Edge, None);
        let off = SnapSettings { on: false, ..ALL };
        let free = DVec3::new(0.013, 0.021, 0.0);
        assert!(s
            .snap(&off, flat(free).unwrap(), &flat, None, None)
            .is_none());
        let hit = s
            .snap(&off, flat(free).unwrap(), &flat, None, Some(free))
            .unwrap();
        assert_eq!(hit.kind, SnapKind::Grid);
        assert_eq!(hit.p, free, "not even rounded");
        // holding Alt is the same as the switch being off
        assert_eq!(ALL.live(true), off);
        assert_eq!(ALL.live(false), ALL);
    }

    #[test]
    fn grid_lines_snap_and_cross_and_can_be_turned_off() {
        let id = ElementId::new();
        let line = |a: DVec2, b: DVec2, pen| PlanItem::Line { id, pen, a, b };
        use creabim_geom::plan::Pen;
        let items = [
            line(DVec2::new(0.0, -5.0), DVec2::new(0.0, 5.0), Pen::Grid),
            line(DVec2::new(-5.0, 2.0), DVec2::new(7.0, 2.0), Pen::Projection),
        ];
        let s = Snapper::from_drawing(&items, &[]);
        let hit = s.snap(&ALL, at(0.03, 2.02), &flat, None, None).unwrap();
        assert_eq!(hit.kind, SnapKind::Intersection);
        let hit = s.snap(&ALL, at(0.04, -3.0), &flat, None, None).unwrap();
        assert_eq!(hit.kind, SnapKind::OnGridLine);
        let no_grids = SnapSettings {
            grid_lines: false,
            ..ALL
        };
        let hit = s
            .snap(&no_grids, at(0.03, 2.02), &flat, None, None)
            .unwrap();
        assert_eq!(hit.kind, SnapKind::OnEdge);
        assert!(s
            .snap(&no_grids, at(0.04, -3.0), &flat, None, None)
            .is_none());
    }

    #[test]
    fn a_cube_has_twelve_feature_edges_and_six_face_centres() {
        let m = EditMesh::cuboid(DVec3::ONE);
        let mut tri = creabim_geom::mesh::TriMesh::default();
        for f in 0..m.faces.len() {
            for t in m.face_corners(f) {
                for i in t {
                    let p = m.vertices[m.faces[f][i] as usize].as_vec3();
                    tri.positions.push(p.to_array());
                    tri.normals.push([0.0; 3]);
                    tri.indices.push(tri.positions.len() as u32 - 1);
                }
            }
        }
        assert_eq!(feature_edges(&tri).len(), 12);
        let centres = face_centres(&tri);
        assert_eq!(centres.len(), 6);
        assert!(centres
            .iter()
            .any(|c| c.distance(DVec3::new(0.0, 0.0, 1.0)) < 1e-6));
    }
}
