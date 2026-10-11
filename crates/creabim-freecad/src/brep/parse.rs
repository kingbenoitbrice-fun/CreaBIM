//! Parser for OpenCascade's text BREP format ("CASCADE Topology V1..V3"),
//! the format FreeCAD uses for `*.brp` shape files inside `.FCStd`.
//!
//! The layout mirrors `BRepTools_ShapeSet`: shared geometry tables first
//! (locations, p-curves, curves, polygons, surfaces, triangulations), then the
//! topological shapes, each listing its sub-shapes as references.

use super::geom::{Curve, Surface};
use super::tokens::Tokens;
use crate::math::{Trsf, V3};
use crate::FcError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orient {
    Forward,
    Reversed,
    Internal,
    External,
}

impl Orient {
    /// Orientation of a sub-shape seen through a parent with orientation
    /// `self` (`TopAbs::Compose`).
    pub fn compose(self, child: Orient) -> Orient {
        match (self, child) {
            (Orient::Forward, c) => c,
            (Orient::Reversed, Orient::Forward) => Orient::Reversed,
            (Orient::Reversed, Orient::Reversed) => Orient::Forward,
            (Orient::Reversed, c) => c,
            (Orient::Internal, _) => Orient::Internal,
            (Orient::External, _) => Orient::External,
        }
    }
}

/// A located, oriented use of a shape.
#[derive(Clone, Copy, Debug)]
pub struct SubRef {
    pub orient: Orient,
    /// Index into [`BrepModel::shapes`].
    pub shape: usize,
    /// Index into [`BrepModel::locations`] (0 = identity).
    pub loc: usize,
}

/// A curve on surface (p-curve) representation of an edge.
#[derive(Clone, Debug)]
pub struct PCurveRep {
    pub pcurve: usize,
    /// Second p-curve of a seam edge on a closed surface (used by the
    /// reversed occurrence of the edge).
    pub pcurve2: Option<usize>,
    pub surface: usize,
    pub loc: usize,
    pub first: f64,
    pub last: f64,
}

#[derive(Clone, Debug)]
pub struct Curve3dRep {
    pub curve: usize,
    pub loc: usize,
    pub first: f64,
    pub last: f64,
}

#[derive(Clone, Debug)]
pub enum ShapeKind {
    Vertex {
        pnt: V3,
    },
    Edge {
        curve: Option<Curve3dRep>,
        pcurves: Vec<PCurveRep>,
        degenerated: bool,
    },
    Wire,
    Face {
        surface: usize,
        loc: usize,
        triangulation: Option<usize>,
    },
    Shell,
    Solid,
    CompSolid,
    Compound,
}

#[derive(Clone, Debug)]
pub struct TShape {
    pub kind: ShapeKind,
    pub subs: Vec<SubRef>,
}

#[derive(Clone, Debug, Default)]
pub struct Triangulation {
    pub nodes: Vec<V3>,
    pub triangles: Vec<[usize; 3]>,
}

/// Parsed BREP text with all tables (1-based in the file, 0 = "none").
#[derive(Debug, Default)]
pub struct BrepModel {
    pub version: u32,
    /// `locations[0]` is the identity.
    pub locations: Vec<Trsf>,
    /// `curves2d[0]` / `curves[0]` / `surfaces[0]` are unused placeholders.
    pub curves2d: Vec<Option<Curve>>,
    pub curves: Vec<Option<Curve>>,
    pub surfaces: Vec<Option<Surface>>,
    pub triangulations: Vec<Triangulation>,
    pub shapes: Vec<TShape>,
    pub root: Option<SubRef>,
}

impl BrepModel {
    pub fn location(&self, i: usize) -> Trsf {
        self.locations.get(i).copied().unwrap_or_default()
    }
    pub fn curve(&self, i: usize) -> Option<&Curve> {
        self.curves.get(i).and_then(|c| c.as_ref())
    }
    pub fn curve2d(&self, i: usize) -> Option<&Curve> {
        self.curves2d.get(i).and_then(|c| c.as_ref())
    }
    pub fn surface(&self, i: usize) -> Option<&Surface> {
        self.surfaces.get(i).and_then(|c| c.as_ref())
    }
}

/// Parses a whole BREP text.
pub fn parse(text: &str) -> Result<BrepModel, FcError> {
    let mut t = Tokens::new(text);
    let mut m = BrepModel {
        version: read_header(&mut t)?,
        ..Default::default()
    };
    m.locations = read_locations(&mut t)?;

    t.expect("Curve2ds")?;
    let n = t.count()?;
    m.curves2d.push(None);
    for _ in 0..n {
        m.curves2d.push(Some(Curve::read2d(&mut t)?));
    }

    t.expect("Curves")?;
    let n = t.count()?;
    m.curves.push(None);
    for _ in 0..n {
        m.curves.push(Some(Curve::read3d(&mut t)?));
    }

    if t.peek() == Some("Polygon3D") {
        t.next()?;
        skip_polygon3d(&mut t)?;
    }
    if t.peek() == Some("PolygonOnTriangulations") {
        t.next()?;
        skip_polygons_on_triangulations(&mut t)?;
    }

    t.expect("Surfaces")?;
    let n = t.count()?;
    m.surfaces.push(None);
    for _ in 0..n {
        m.surfaces.push(Some(Surface::read(&mut t)?));
    }

    if t.peek() == Some("Triangulations") {
        t.next()?;
        m.triangulations = read_triangulations(&mut t, m.version)?;
    }

    t.expect("TShapes")?;
    let n = t.count()?;
    let mut shapes = Vec::with_capacity(n);
    for _ in 0..n {
        shapes.push(read_tshape(&mut t, n, m.version)?);
    }
    m.shapes = shapes;
    m.root = match t.peek() {
        Some(_) => read_ref(&mut t, n)?,
        None => None,
    };
    let loc_count = m.locations.len();
    let bad_ref = |r: &SubRef| r.loc >= loc_count;
    if m.shapes.iter().flat_map(|s| s.subs.iter()).any(bad_ref)
        || m.root.as_ref().is_some_and(bad_ref)
    {
        return Err(FcError::Brep("location index out of range".into()));
    }
    Ok(m)
}

fn read_header(t: &mut Tokens) -> Result<u32, FcError> {
    // "CASCADE Topology V1, (c) Matra-Datavision", optionally preceded by
    // "DBRep_DrawableShape" when written by DRAW or `Shape.exportBrep`.
    t.seek("CASCADE")?;
    t.expect("Topology")?;
    let v = t.next()?;
    let version = match v.trim_end_matches(',') {
        "V1" => 1,
        "V2" => 2,
        "V3" => 3,
        other => return Err(FcError::Brep(format!("unsupported BREP version {other}"))),
    };
    t.seek("Locations")?;
    Ok(version)
}

fn read_locations(t: &mut Tokens) -> Result<Vec<Trsf>, FcError> {
    let n = t.count()?;
    let mut locs = Vec::with_capacity(n + 1);
    locs.push(Trsf::IDENTITY);
    for _ in 0..n {
        let kind = t.int()?;
        let loc = match kind {
            1 => {
                let mut m = [[0.0; 4]; 3];
                for row in m.iter_mut() {
                    for c in row.iter_mut() {
                        *c = t.f64()?;
                    }
                }
                Trsf { m }
            }
            2 => {
                // Product of earlier locations raised to powers, terminated
                // by 0; each factor is applied on the left (TopLoc_Location).
                let mut l = Trsf::IDENTITY;
                loop {
                    let idx = t.int()?;
                    if idx == 0 {
                        break;
                    }
                    let pow = t.int()?;
                    let base = usize::try_from(idx)
                        .ok()
                        .and_then(|i| locs.get(i))
                        .ok_or_else(|| FcError::Brep("bad location reference".into()))?;
                    let f = base
                        .powered(pow)
                        .ok_or_else(|| FcError::Brep("singular location".into()))?;
                    l = f.then_after(&l);
                }
                l
            }
            k => return Err(FcError::Brep(format!("unknown location type {k}"))),
        };
        locs.push(loc);
    }
    Ok(locs)
}

fn skip_polygon3d(t: &mut Tokens) -> Result<(), FcError> {
    let n = t.count()?;
    for _ in 0..n {
        let nodes = t.count()?;
        let has_params = t.int()? != 0;
        t.f64()?; // deflection
        for _ in 0..nodes * 3 + if has_params { nodes } else { 0 } {
            t.next()?;
        }
    }
    Ok(())
}

fn skip_polygons_on_triangulations(t: &mut Tokens) -> Result<(), FcError> {
    let n = t.count()?;
    for _ in 0..n {
        let nodes = t.count()?;
        for _ in 0..nodes {
            t.next()?;
        }
        t.expect("p")?;
        t.f64()?; // deflection
        if t.int()? != 0 {
            for _ in 0..nodes {
                t.next()?;
            }
        }
    }
    Ok(())
}

fn read_triangulations(t: &mut Tokens, version: u32) -> Result<Vec<Triangulation>, FcError> {
    let n = t.count()?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let nb_nodes = t.count()?;
        let nb_tris = t.count()?;
        let has_uv = t.int()? != 0;
        let has_normals = version >= 3 && t.int()? != 0;
        t.f64()?; // deflection
        let mut nodes = Vec::with_capacity(nb_nodes);
        for _ in 0..nb_nodes {
            nodes.push(t.v3()?);
        }
        if has_uv {
            for _ in 0..nb_nodes * 2 {
                t.next()?;
            }
        }
        let mut triangles = Vec::with_capacity(nb_tris);
        for _ in 0..nb_tris {
            let mut tri = [0usize; 3];
            for v in tri.iter_mut() {
                let i = t.count()?;
                if i == 0 || i > nb_nodes {
                    return Err(FcError::Brep("triangle node out of range".into()));
                }
                *v = i - 1;
            }
            triangles.push(tri);
        }
        if has_normals {
            for _ in 0..nb_nodes * 3 {
                t.next()?;
            }
        }
        out.push(Triangulation { nodes, triangles });
    }
    Ok(out)
}

/// Reads `+12 3` style references; `*` ends a list and yields `None`.
fn read_ref(t: &mut Tokens, nb_shapes: usize) -> Result<Option<SubRef>, FcError> {
    let tok = t.next()?;
    if tok == "*" {
        return Ok(None);
    }
    let mut chars = tok.chars();
    let orient = match chars.next() {
        Some('+') => Orient::Forward,
        Some('-') => Orient::Reversed,
        Some('i') => Orient::Internal,
        Some('e') => Orient::External,
        _ => return Err(FcError::Brep(format!("bad shape reference '{tok}'"))),
    };
    let idx: usize = chars
        .as_str()
        .parse()
        .map_err(|_| FcError::Brep(format!("bad shape reference '{tok}'")))?;
    let loc = t.count()?;
    // Shapes are numbered from the end of the table in references.
    if idx == 0 || idx > nb_shapes {
        return Err(FcError::Brep(format!("shape reference {idx} out of range")));
    }
    Ok(Some(SubRef {
        orient,
        shape: nb_shapes - idx,
        loc,
    }))
}

fn read_tshape(t: &mut Tokens, nb_shapes: usize, version: u32) -> Result<TShape, FcError> {
    let ty = t.next()?;
    let kind = match ty {
        "Ve" => {
            t.f64()?; // tolerance
            let pnt = t.v3()?;
            // Point representations (on curves/surfaces), ended by "0 0".
            loop {
                match t.int()? {
                    0 => {
                        if t.peek() == Some("0") {
                            t.next()?;
                        }
                        break;
                    }
                    1 => {
                        t.f64()?;
                        t.int()?;
                        t.int()?;
                    }
                    2 => {
                        t.f64()?;
                        t.int()?;
                        t.int()?;
                        t.int()?;
                    }
                    3 => {
                        t.f64()?;
                        t.f64()?;
                        t.int()?;
                        t.int()?;
                    }
                    k => return Err(FcError::Brep(format!("bad vertex representation {k}"))),
                }
            }
            ShapeKind::Vertex { pnt }
        }
        "Ed" => read_edge(t, version)?,
        "Wi" => ShapeKind::Wire,
        "Fa" => {
            let _natural = t.int()?;
            let _tol = t.f64()?;
            let surface = t.count()?;
            let loc = t.count()?;
            let mut triangulation = None;
            if t.peek() == Some("2") {
                t.next()?;
                let i = t.count()?;
                if i > 0 {
                    triangulation = Some(i - 1);
                }
            }
            ShapeKind::Face {
                surface,
                loc,
                triangulation,
            }
        }
        "Sh" => ShapeKind::Shell,
        "So" => ShapeKind::Solid,
        "CS" => ShapeKind::CompSolid,
        "Co" => ShapeKind::Compound,
        other => return Err(FcError::Brep(format!("unknown shape type '{other}'"))),
    };
    let _flags = t.next()?;
    let mut subs = Vec::new();
    while let Some(r) = read_ref(t, nb_shapes)? {
        subs.push(r);
    }
    Ok(TShape { kind, subs })
}

fn read_edge(t: &mut Tokens, version: u32) -> Result<ShapeKind, FcError> {
    t.f64()?; // tolerance
    t.int()?; // same parameter
    t.int()?; // same range
    let degenerated = t.int()? != 0;
    let mut curve = None;
    let mut pcurves = Vec::new();
    loop {
        match t.int()? {
            0 => break,
            1 => {
                let c = t.count()?;
                let loc = t.count()?;
                let first = t.f64()?;
                let last = t.f64()?;
                curve = Some(Curve3dRep {
                    curve: c,
                    loc,
                    first,
                    last,
                });
            }
            k @ (2 | 3) => {
                let pcurve = t.count()?;
                let mut pcurve2 = None;
                if k == 3 {
                    let (pc2, reg) = t.int_with_suffix()?;
                    pcurve2 = usize::try_from(pc2).ok();
                    if reg.is_none() {
                        t.next()?; // regularity, e.g. "CN"
                    }
                }
                let surface = t.count()?;
                let loc = t.count()?;
                let first = t.f64()?;
                let last = t.f64()?;
                if version >= 2 {
                    // UV of the end points, written since format V2.
                    for _ in 0..4 {
                        t.f64()?;
                    }
                }
                pcurves.push(PCurveRep {
                    pcurve,
                    pcurve2,
                    surface,
                    loc,
                    first,
                    last,
                });
            }
            4 => {
                t.next()?; // regularity between the two faces, e.g. "C0"
                for _ in 0..4 {
                    t.int()?;
                }
            }
            5 => {
                t.int()?;
                t.int()?;
            }
            6 => {
                for _ in 0..3 {
                    t.int()?;
                }
            }
            7 => {
                for _ in 0..4 {
                    t.int()?;
                }
            }
            k => return Err(FcError::Brep(format!("bad edge representation {k}"))),
        }
    }
    Ok(ShapeKind::Edge {
        curve,
        pcurves,
        degenerated,
    })
}
