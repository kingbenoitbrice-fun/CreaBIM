//! The starter catalogue: everyday furniture and fixtures, built from a
//! few simple solids (bevelled boxes, cylinders and lathe-like stacks of
//! outlines for bowls and tubs) with their materials, so a plan can be
//! furnished before anything has been modelled in Blender or FreeCAD.
//!
//! Sizes are the usual European ones. Every object is in its own axes (see
//! [`crate::objects`]): its back on `y = 0`, its front towards `-y`.

use crate::objects::{
    CatalogueObject, IfcKind, Mount, ObjectCategory, ObjectPart, ObjectType, SymbolItem,
};
use crate::{EditMesh, Hatch, Material};
use glam::{DVec2, DVec3};
use std::f64::consts::{FRAC_PI_2, TAU};

mod services;

// ---- outlines and solids ----

/// A rounded rectangle centred on `c`, anticlockwise, `k` segments per
/// corner (always `4 (k + 1)` points, so outlines of one stack match).
fn rrect(c: DVec2, w: f64, d: f64, r: f64, k: usize) -> Vec<DVec2> {
    let r = r.clamp(1e-4, (w.min(d) / 2.0 - 1e-4).max(1e-4));
    let (hx, hy) = (w / 2.0 - r, d / 2.0 - r);
    let centres = [
        DVec2::new(hx, -hy),
        DVec2::new(hx, hy),
        DVec2::new(-hx, hy),
        DVec2::new(-hx, -hy),
    ];
    let mut out = vec![];
    for (i, cc) in centres.iter().enumerate() {
        let a0 = -FRAC_PI_2 + i as f64 * FRAC_PI_2;
        for j in 0..=k {
            let a = a0 + FRAC_PI_2 * j as f64 / k.max(1) as f64;
            out.push(c + *cc + DVec2::from_angle(a) * r);
        }
    }
    out
}

/// A superellipse (`p` = 2 is an ellipse, larger is squarer), `n` points,
/// anticlockwise from the front.
fn squircle(c: DVec2, rx: f64, ry: f64, p: f64, n: usize) -> Vec<DVec2> {
    (0..n)
        .map(|i| {
            let a = -FRAC_PI_2 + TAU * i as f64 / n as f64;
            let (s, co) = a.sin_cos();
            let f = |v: f64| v.signum() * v.abs().powf(2.0 / p);
            c + DVec2::new(rx * f(co), ry * f(s))
        })
        .collect()
}

/// A closed solid through outlines at heights, all with the same number of
/// points: up the outside, and for a bowl over the rim and down the inside.
/// The first outline is capped facing down, the last facing up.
fn stack(rings: &[(Vec<DVec2>, f64)]) -> EditMesh {
    let n = rings[0].0.len();
    debug_assert!(rings.iter().all(|r| r.0.len() == n));
    let mut m = EditMesh::default();
    for (pts, z) in rings {
        m.vertices.extend(pts.iter().map(|p| p.extend(*z)));
    }
    let at = |r: usize, j: usize| (r * n + j % n) as u32;
    m.faces.push((0..n).rev().map(|j| at(0, j)).collect());
    for r in 0..rings.len() - 1 {
        for j in 0..n {
            m.faces
                .push(vec![at(r, j), at(r, j + 1), at(r + 1, j + 1), at(r + 1, j)]);
        }
    }
    let last = rings.len() - 1;
    m.faces.push((0..n).map(|j| at(last, j)).collect());
    m
}

fn prism(outline: Vec<DVec2>, z0: f64, z1: f64) -> EditMesh {
    stack(&[(outline.clone(), z0), (outline, z1)])
}

/// A box between two corners.
fn cuboid(lo: DVec3, hi: DVec3) -> EditMesh {
    let c = (lo + hi).truncate() / 2.0;
    let s = hi - lo;
    prism(rrect(c, s.x, s.y, 0.0, 0), lo.z, hi.z)
}

/// A box with rounded corners in plan and its top and bottom edges
/// bevelled by `b`: cushions, mattresses, table tops.
fn soft_box(lo: DVec3, hi: DVec3, r: f64, b: f64) -> EditMesh {
    let c = (lo + hi).truncate() / 2.0;
    let s = hi - lo;
    let b = b.min(s.z / 2.0 - 1e-4).max(0.0);
    let full = rrect(c, s.x, s.y, r, 4);
    let inset = rrect(c, s.x - 2.0 * b, s.y - 2.0 * b, (r - b).max(0.002), 4);
    stack(&[
        (inset.clone(), lo.z),
        (full.clone(), lo.z + b),
        (full, hi.z - b),
        (inset, hi.z),
    ])
}

fn cylinder(c: DVec2, r: f64, z0: f64, z1: f64) -> EditMesh {
    prism(squircle(c, r, r, 2.0, 20), z0, z1)
}

fn outline_of(m: &EditMesh) -> Vec<DVec2> {
    let pts: Vec<DVec2> = m.vertices.iter().map(|v| v.truncate()).collect();
    crate::objects::hull(&pts)
}

// ---- materials ----

type Look = (String, Material);

fn look(name: &str, category: &str, hatch: Hatch, rgb: [u8; 3], rough: f32) -> Look {
    (
        name.to_string(),
        Material {
            hatch,
            color: rgb.map(|v| v as f32 / 255.0),
            category: category.into(),
            roughness: rough,
            ..Default::default()
        },
    )
}

fn metal(name: &str, rgb: [u8; 3], rough: f32) -> Look {
    let mut l = look(name, "metal", Hatch::Solid, rgb, rough);
    l.1.metallic = 1.0;
    l
}

fn ceramic() -> Look {
    look(
        "Sanitary ceramic, white",
        "ceramic",
        Hatch::Empty,
        [244, 244, 240],
        0.12,
    )
}
fn acrylic() -> Look {
    look(
        "Acrylic, white",
        "plastic",
        Hatch::Empty,
        [247, 247, 245],
        0.2,
    )
}
fn chrome() -> Look {
    metal("Chrome", [218, 221, 226], 0.12)
}
fn stainless() -> Look {
    metal("Steel, brushed stainless", [190, 193, 196], 0.35)
}
fn black_metal() -> Look {
    let mut l = metal("Metal, black powder coat", [38, 38, 40], 0.55);
    l.1.metallic = 0.4;
    l
}
fn glass() -> Look {
    let mut l = look("Glass, clear", "glass", Hatch::Empty, [210, 230, 235], 0.05);
    l.1.opacity = 0.3;
    l
}
fn black_glass() -> Look {
    look("Glass, black", "glass", Hatch::Empty, [16, 16, 18], 0.05)
}
fn oak() -> Look {
    look("Timber, oak", "wood", Hatch::Timber, [176, 138, 92], 0.6)
}
fn walnut() -> Look {
    look("Walnut veneer", "wood", Hatch::Timber, [104, 70, 46], 0.5)
}
fn laminate() -> Look {
    look(
        "Laminate, white",
        "plastic",
        Hatch::Empty,
        [236, 236, 232],
        0.4,
    )
}
fn kitchen_front() -> Look {
    look(
        "Kitchen front, sage",
        "finish",
        Hatch::Empty,
        [140, 158, 134],
        0.45,
    )
}
fn quartz() -> Look {
    look(
        "Worktop, quartz",
        "stone",
        Hatch::Diagonal,
        [222, 220, 212],
        0.25,
    )
}
fn plinth() -> Look {
    look(
        "Plinth, dark grey",
        "plastic",
        Hatch::Empty,
        [52, 54, 58],
        0.6,
    )
}
fn fabric(name: &str, rgb: [u8; 3]) -> Look {
    look(name, "textile", Hatch::Empty, rgb, 0.95)
}
fn linen() -> Look {
    fabric("Linen, white", [236, 233, 225])
}
fn appliance_white() -> Look {
    look(
        "Appliance, white enamel",
        "metal",
        Hatch::Empty,
        [240, 240, 238],
        0.3,
    )
}

// ---- building an object ----

struct Build {
    parts: Vec<ObjectPart>,
    materials: Vec<Option<Look>>,
    symbol: Vec<SymbolItem>,
    /// The height it is placed at (see [`ObjectType::offset`]).
    offset: f64,
}

impl Build {
    fn new() -> Self {
        Self {
            parts: vec![],
            materials: vec![],
            symbol: vec![],
            offset: 0.0,
        }
    }

    /// Placed at `h`: above the floor to its underside, or for objects
    /// hung from the ceiling, below it to their top.
    fn at(&mut self, h: f64) -> &mut Self {
        self.offset = h;
        self
    }

    /// A part; `plan` draws its outline in the plan symbol.
    fn add(&mut self, name: &str, look: Look, mesh: EditMesh, plan: bool) -> &mut Self {
        if plan {
            self.symbol.push(SymbolItem::Loop(outline_of(&mesh)));
        }
        self.parts.push(ObjectPart {
            name: name.into(),
            mesh,
            material: None,
        });
        self.materials.push(Some(look));
        self
    }

    fn outline(&mut self, pts: Vec<DVec2>) -> &mut Self {
        self.symbol.push(SymbolItem::Loop(pts));
        self
    }

    fn line(&mut self, a: DVec2, b: DVec2) -> &mut Self {
        self.symbol.push(SymbolItem::Line(a, b));
        self
    }

    #[allow(clippy::too_many_arguments)]
    fn finish(
        &mut self,
        key: &str,
        name: &str,
        description: &str,
        category: ObjectCategory,
        ifc: (&str, &str),
        mount: Mount,
    ) -> CatalogueObject {
        let key = format!("starter/{key}");
        CatalogueObject {
            key: key.clone(),
            name: name.into(),
            description: description.into(),
            object: ObjectType {
                category,
                ifc: IfcKind::new(ifc.0, ifc.1),
                mount,
                offset: std::mem::take(&mut self.offset),
                parts: std::mem::take(&mut self.parts),
                symbol: std::mem::take(&mut self.symbol),
                catalogue: Some(key),
            },
            materials: std::mem::take(&mut self.materials),
        }
    }
}

fn v3(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}
fn v2(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

/// A pillar tap with a spout reaching forward to `reach`, standing at `at`.
fn tap(b: &mut Build, at: DVec2, z: f64, reach: f64) {
    b.add("Tap", chrome(), cylinder(at, 0.022, z, z + 0.16), true);
    b.add(
        "Spout",
        chrome(),
        cuboid(
            v3(at.x - 0.012, at.y - reach, z + 0.13),
            v3(at.x + 0.012, at.y, z + 0.15),
        ),
        true,
    );
}

// ---- bathroom ----

fn washbasin() -> CatalogueObject {
    let mut b = Build::new();
    let (w, d, top, k) = (0.6, 0.46, 0.85, 6);
    let c = v2(0.0, -d / 2.0);
    let bowl = v2(0.0, -0.26);
    b.add(
        "Basin",
        ceramic(),
        stack(&[
            (
                rrect(c + v2(0.0, 0.03), w - 0.1, d - 0.12, 0.05, k),
                top - 0.2,
            ),
            (rrect(c, w, d, 0.07, k), top - 0.04),
            (rrect(c, w, d, 0.07, k), top),
            (rrect(bowl, 0.46, 0.3, 0.13, k), top),
            (rrect(bowl, 0.42, 0.26, 0.12, k), top - 0.07),
            (rrect(bowl, 0.3, 0.17, 0.08, k), top - 0.14),
        ]),
        true,
    );
    b.outline(rrect(bowl, 0.46, 0.3, 0.13, k));
    tap(&mut b, v2(0.0, -0.06), top, 0.1);
    b.finish(
        "washbasin",
        "Washbasin 600",
        "Wall-hung ceramic basin with a pillar tap, rim at 850 mm",
        ObjectCategory::Bathroom,
        ("IfcSanitaryTerminal", "WASHHANDBASIN"),
        Mount::Wall,
    )
}

fn wc() -> CatalogueObject {
    let mut b = Build::new();
    let n = 32;
    let cy = -0.43;
    b.add(
        "Cistern",
        ceramic(),
        soft_box(v3(-0.19, -0.18, 0.4), v3(0.19, 0.0, 0.8), 0.03, 0.01),
        true,
    );
    b.add(
        "Flush button",
        chrome(),
        cylinder(v2(0.0, -0.09), 0.035, 0.8, 0.81),
        false,
    );
    b.add(
        "Pan",
        ceramic(),
        stack(&[
            (squircle(v2(0.0, cy + 0.04), 0.12, 0.17, 2.6, n), 0.0),
            (squircle(v2(0.0, cy + 0.03), 0.13, 0.2, 2.6, n), 0.22),
            (squircle(v2(0.0, cy), 0.18, 0.27, 2.4, n), 0.38),
            (squircle(v2(0.0, cy - 0.01), 0.14, 0.21, 2.4, n), 0.39),
            (squircle(v2(0.0, cy - 0.02), 0.07, 0.1, 2.4, n), 0.24),
        ]),
        true,
    );
    b.add(
        "Seat and lid",
        ceramic(),
        stack(&[
            (squircle(v2(0.0, cy), 0.18, 0.27, 2.4, n), 0.39),
            (squircle(v2(0.0, cy), 0.185, 0.28, 2.4, n), 0.405),
            (squircle(v2(0.0, cy), 0.175, 0.27, 2.4, n), 0.425),
        ]),
        true,
    );
    b.finish(
        "wc",
        "WC, close-coupled",
        "Floor-standing pan with its cistern, seat height 420 mm",
        ObjectCategory::Bathroom,
        ("IfcSanitaryTerminal", "TOILETPAN"),
        Mount::Wall,
    )
}

fn bathtub() -> CatalogueObject {
    let mut b = Build::new();
    let (w, d, h, k) = (1.7, 0.75, 0.56, 6);
    let c = v2(0.0, -d / 2.0);
    let inner = c + v2(0.05, 0.0);
    b.add(
        "Bath",
        acrylic(),
        stack(&[
            (rrect(c, w, d, 0.03, k), 0.0),
            (rrect(c, w, d, 0.03, k), h),
            (rrect(inner, 1.52, 0.6, 0.26, k), h),
            (rrect(inner, 1.46, 0.54, 0.24, k), h - 0.2),
            (rrect(inner + v2(0.03, 0.0), 1.28, 0.42, 0.19, k), 0.12),
        ]),
        true,
    );
    b.outline(rrect(inner, 1.52, 0.6, 0.26, k));
    b.add(
        "Waste",
        chrome(),
        cylinder(inner + v2(0.58, 0.0), 0.035, 0.12, 0.125),
        false,
    );
    tap(&mut b, v2(0.79, -0.37), h, 0.12);
    b.finish(
        "bathtub",
        "Bath 1700 × 750",
        "Acrylic bath with its taps at one end",
        ObjectCategory::Bathroom,
        ("IfcSanitaryTerminal", "BATH"),
        Mount::Wall,
    )
}

fn shower() -> CatalogueObject {
    let mut b = Build::new();
    let (s, k) = (0.9, 6);
    let c = v2(0.0, -s / 2.0);
    b.add(
        "Tray",
        acrylic(),
        stack(&[
            (rrect(c, s, s, 0.01, k), 0.0),
            (rrect(c, s, s, 0.01, k), 0.05),
            (rrect(c, s - 0.06, s - 0.06, 0.03, k), 0.05),
            (rrect(c, s - 0.12, s - 0.12, 0.03, k), 0.025),
        ]),
        true,
    );
    b.outline(rrect(c, s - 0.06, s - 0.06, 0.03, k));
    b.line(v2(-0.42, -0.03), v2(0.42, -0.87));
    b.line(v2(0.42, -0.03), v2(-0.42, -0.87));
    b.add("Drain", chrome(), cylinder(c, 0.05, 0.025, 0.03), false);
    // a fixed panel across half the front and one along the right side
    b.add(
        "Front panel",
        glass(),
        cuboid(v3(-0.445, -0.9, 0.05), v3(0.0, -0.892, 2.0)),
        true,
    );
    b.add(
        "Side panel",
        glass(),
        cuboid(v3(0.442, -0.9, 0.05), v3(0.45, -0.005, 2.0)),
        true,
    );
    b.add(
        "Riser",
        chrome(),
        cylinder(v2(-0.3, -0.03), 0.012, 1.0, 2.0),
        false,
    );
    b.add(
        "Arm",
        chrome(),
        cuboid(v3(-0.31, -0.25, 1.98), v3(-0.29, -0.03, 2.0)),
        false,
    );
    b.add(
        "Head",
        chrome(),
        cylinder(v2(-0.3, -0.25), 0.1, 1.965, 1.98),
        false,
    );
    b.add(
        "Mixer",
        chrome(),
        soft_box(v3(-0.36, -0.06, 1.0), v3(-0.24, -0.01, 1.06), 0.02, 0.004),
        false,
    );
    b.finish(
        "shower",
        "Shower 900 × 900",
        "Low tray with a glass screen, a fixed head and a mixer",
        ObjectCategory::Bathroom,
        ("IfcSanitaryTerminal", "SHOWER"),
        Mount::Wall,
    )
}

// ---- kitchen ----

/// A base unit carcass `w` wide with `doors` doors, on its plinth; the
/// worktop is left to the caller.
fn base_unit(b: &mut Build, w: f64, doors: usize, front: Look) {
    b.add(
        "Plinth",
        plinth(),
        cuboid(v3(-w / 2.0, -0.5, 0.0), v3(w / 2.0, 0.0, 0.1)),
        false,
    );
    b.add(
        "Carcass",
        laminate(),
        cuboid(v3(-w / 2.0, -0.56, 0.1), v3(w / 2.0, 0.0, 0.87)),
        false,
    );
    let dw = w / doors as f64;
    for i in 0..doors {
        let x0 = -w / 2.0 + i as f64 * dw;
        b.add(
            "Door",
            front.clone(),
            cuboid(
                v3(x0 + 0.002, -0.58, 0.102),
                v3(x0 + dw - 0.002, -0.56, 0.868),
            ),
            false,
        );
        let hx = if i % 2 == 0 && doors > 1 {
            x0 + dw - 0.05
        } else {
            x0 + 0.05
        };
        b.add(
            "Handle",
            stainless(),
            cuboid(v3(hx - 0.006, -0.6, 0.62), v3(hx + 0.006, -0.58, 0.8)),
            false,
        );
    }
}

fn worktop(b: &mut Build, w: f64) {
    b.add(
        "Worktop",
        quartz(),
        cuboid(v3(-w / 2.0, -0.62, 0.87), v3(w / 2.0, 0.0, 0.91)),
        true,
    );
}

fn base_cabinet() -> CatalogueObject {
    let mut b = Build::new();
    base_unit(&mut b, 0.6, 1, kitchen_front());
    worktop(&mut b, 0.6);
    b.line(v2(-0.3, -0.58), v2(0.3, -0.58));
    b.finish(
        "base-cabinet-600",
        "Base cabinet 600",
        "Kitchen base unit with one door and a 40 mm worktop, 910 mm high",
        ObjectCategory::Kitchen,
        ("IfcFurniture", "USERDEFINED"),
        Mount::Wall,
    )
}

fn sink_unit() -> CatalogueObject {
    let mut b = Build::new();
    let w = 1.0;
    base_unit(&mut b, w, 2, kitchen_front());
    // the worktop round the sink's cut-out
    let (x0, x1, y0, y1) = (-0.42, 0.42, -0.54, -0.08);
    for (lo, hi) in [
        (v3(-w / 2.0, -0.62, 0.87), v3(x0, 0.0, 0.91)),
        (v3(x1, -0.62, 0.87), v3(w / 2.0, 0.0, 0.91)),
        (v3(x0, -0.62, 0.87), v3(x1, y0, 0.91)),
        (v3(x0, y1, 0.87), v3(x1, 0.0, 0.91)),
    ] {
        b.add("Worktop", quartz(), cuboid(lo, hi), false);
    }
    b.outline(rrect(v2(0.0, -0.31), w, 0.62, 0.0, 0));
    let k = 5;
    let bowl = v2(-0.12, -0.31);
    b.add(
        "Sink",
        stainless(),
        stack(&[
            (rrect(bowl, 0.5, 0.4, 0.05, k), 0.72),
            (rrect(bowl, 0.52, 0.42, 0.05, k), 0.905),
            (rrect(v2(0.0, -0.31), x1 - x0, y1 - y0, 0.02, k), 0.905),
            (rrect(v2(0.0, -0.31), x1 - x0, y1 - y0, 0.02, k), 0.915),
            (rrect(bowl, 0.5, 0.4, 0.05, k), 0.915),
            (rrect(bowl, 0.48, 0.38, 0.05, k), 0.73),
        ]),
        true,
    );
    b.outline(rrect(bowl, 0.5, 0.4, 0.05, k));
    // the drainer's grooves
    for i in 0..4 {
        let x = 0.2 + i as f64 * 0.045;
        b.line(v2(x, -0.47), v2(x, -0.15));
    }
    tap(&mut b, v2(-0.12, -0.1), 0.915, 0.2);
    b.finish(
        "sink-unit-1000",
        "Sink unit 1000",
        "Kitchen base unit with two doors, an inset stainless sink with a drainer and a tap",
        ObjectCategory::Kitchen,
        ("IfcSanitaryTerminal", "SINK"),
        Mount::Wall,
    )
}

fn cooker_unit() -> CatalogueObject {
    let mut b = Build::new();
    let w = 0.6;
    b.add(
        "Plinth",
        plinth(),
        cuboid(v3(-w / 2.0, -0.5, 0.0), v3(w / 2.0, 0.0, 0.1)),
        false,
    );
    b.add(
        "Carcass",
        laminate(),
        cuboid(v3(-w / 2.0, -0.56, 0.1), v3(w / 2.0, 0.0, 0.87)),
        false,
    );
    b.add(
        "Drawer",
        kitchen_front(),
        cuboid(v3(-0.298, -0.58, 0.102), v3(0.298, -0.56, 0.25)),
        false,
    );
    b.add(
        "Oven door",
        black_glass(),
        cuboid(v3(-0.298, -0.58, 0.26), v3(0.298, -0.56, 0.84)),
        false,
    );
    b.add(
        "Oven handle",
        stainless(),
        cuboid(v3(-0.22, -0.61, 0.78), v3(0.22, -0.59, 0.8)),
        false,
    );
    worktop(&mut b, w);
    b.add(
        "Hob",
        black_glass(),
        soft_box(v3(-0.29, -0.56, 0.91), v3(0.29, -0.05, 0.916), 0.01, 0.002),
        true,
    );
    for (x, y, r) in [
        (-0.14, -0.18, 0.09),
        (0.14, -0.18, 0.07),
        (-0.14, -0.43, 0.07),
        (0.14, -0.43, 0.09),
    ] {
        b.outline(squircle(v2(x, y), r, r, 2.0, 24));
        b.add(
            "Ring",
            look("Hob ring, grey", "glass", Hatch::Empty, [70, 72, 76], 0.2),
            prism(squircle(v2(x, y), r, r, 2.0, 24), 0.916, 0.917),
            false,
        );
    }
    b.finish(
        "cooker-600",
        "Cooker unit 600",
        "Built-in oven under a four-zone ceramic hob, with a drawer",
        ObjectCategory::Kitchen,
        ("IfcElectricAppliance", "ELECTRICCOOKER"),
        Mount::Wall,
    )
}

fn wall_cabinet() -> CatalogueObject {
    let mut b = Build::new();
    let (w, z0, z1) = (0.6, 1.45, 2.17);
    b.add(
        "Carcass",
        laminate(),
        cuboid(v3(-w / 2.0, -0.33, z0), v3(w / 2.0, 0.0, z1)),
        true,
    );
    b.add(
        "Door",
        kitchen_front(),
        cuboid(v3(-0.298, -0.35, z0 + 0.002), v3(0.298, -0.33, z1 - 0.002)),
        true,
    );
    b.add(
        "Handle",
        stainless(),
        cuboid(v3(0.23, -0.37, z0 + 0.03), v3(0.242, -0.35, z0 + 0.2)),
        false,
    );
    b.finish(
        "wall-cabinet-600",
        "Wall cabinet 600",
        "Kitchen wall unit 720 mm high, hung at 1450 mm",
        ObjectCategory::Kitchen,
        ("IfcFurniture", "USERDEFINED"),
        Mount::Wall,
    )
}

fn fridge() -> CatalogueObject {
    let mut b = Build::new();
    let (w, d, h) = (0.6, 0.65, 1.85);
    b.add(
        "Body",
        appliance_white(),
        soft_box(
            v3(-w / 2.0, -d + 0.04, 0.0),
            v3(w / 2.0, 0.0, h),
            0.02,
            0.01,
        ),
        true,
    );
    b.add(
        "Fridge door",
        appliance_white(),
        soft_box(
            v3(-0.298, -d, 0.71),
            v3(0.298, -d + 0.04, h - 0.005),
            0.01,
            0.006,
        ),
        false,
    );
    b.add(
        "Freezer door",
        appliance_white(),
        soft_box(v3(-0.298, -d, 0.02), v3(0.298, -d + 0.04, 0.7), 0.01, 0.006),
        false,
    );
    for (z0, z1) in [(0.75, 1.1), (0.45, 0.66)] {
        b.add(
            "Handle",
            stainless(),
            cuboid(v3(0.24, -d - 0.03, z0), v3(0.255, -d, z1)),
            false,
        );
    }
    b.line(v2(-w / 2.0, -d), v2(w / 2.0, -d));
    b.line(v2(-w / 2.0, -d + 0.04), v2(w / 2.0, -d));
    b.finish(
        "fridge-freezer",
        "Fridge freezer 600",
        "Freestanding fridge over freezer, 1850 mm high",
        ObjectCategory::Kitchen,
        ("IfcElectricAppliance", "FRIDGE_FREEZER"),
        Mount::Wall,
    )
}

// ---- living ----

/// A sofa or armchair: `seats` cushions between two arms.
fn sofa_like(b: &mut Build, seats: usize, seat_w: f64, cover: Look) {
    let arm = 0.2;
    let d = 0.9;
    let w = seats as f64 * seat_w + 2.0 * arm;
    let x0 = -w / 2.0;
    b.add(
        "Base",
        cover.clone(),
        cuboid(v3(x0 + arm, -d + 0.04, 0.08), v3(-x0 - arm, -0.05, 0.22)),
        false,
    );
    for sx in [x0, -x0 - arm] {
        b.add(
            "Arm",
            cover.clone(),
            soft_box(v3(sx, -d, 0.08), v3(sx + arm, 0.0, 0.62), 0.05, 0.04),
            true,
        );
    }
    b.add(
        "Back",
        cover.clone(),
        soft_box(
            v3(x0 + arm - 0.01, -0.2, 0.08),
            v3(-x0 - arm + 0.01, 0.0, 0.82),
            0.03,
            0.03,
        ),
        true,
    );
    for i in 0..seats {
        let a = x0 + arm + i as f64 * seat_w;
        b.add(
            "Seat cushion",
            cover.clone(),
            soft_box(
                v3(a + 0.005, -d + 0.02, 0.22),
                v3(a + seat_w - 0.005, -0.32, 0.44),
                0.05,
                0.05,
            ),
            true,
        );
        b.add(
            "Back cushion",
            cover.clone(),
            soft_box(
                v3(a + 0.01, -0.36, 0.44),
                v3(a + seat_w - 0.01, -0.18, 0.78),
                0.05,
                0.06,
            ),
            true,
        );
    }
    for (x, y) in [
        (x0 + 0.06, -d + 0.06),
        (-x0 - 0.06, -d + 0.06),
        (x0 + 0.06, -0.06),
        (-x0 - 0.06, -0.06),
    ] {
        b.add(
            "Foot",
            walnut(),
            cylinder(v2(x, y), 0.025, 0.0, 0.08),
            false,
        );
    }
}

fn sofa() -> CatalogueObject {
    let mut b = Build::new();
    sofa_like(&mut b, 3, 0.62, fabric("Fabric, grey", [118, 122, 128]));
    b.finish(
        "sofa-3",
        "Sofa, three seats",
        "Three-seat sofa 2260 × 900, seat height 440 mm",
        ObjectCategory::Living,
        ("IfcFurniture", "SOFA"),
        Mount::Wall,
    )
}

fn armchair() -> CatalogueObject {
    let mut b = Build::new();
    sofa_like(&mut b, 1, 0.6, fabric("Fabric, teal", [52, 106, 112]));
    b.finish(
        "armchair",
        "Armchair",
        "Upholstered armchair 1000 × 900",
        ObjectCategory::Living,
        ("IfcFurniture", "CHAIR"),
        Mount::Floor,
    )
}

/// A table top with four legs set in from its corners.
fn table(b: &mut Build, w: f64, d: f64, h: f64, top: Look, legs: Look, leg: f64) {
    b.add(
        "Top",
        top,
        soft_box(v3(-w / 2.0, -d, h - 0.03), v3(w / 2.0, 0.0, h), 0.02, 0.004),
        true,
    );
    let (ix, iy) = (w / 2.0 - 0.08, 0.08);
    for (x, y) in [(-ix, -iy), (ix, -iy), (-ix, -d + iy), (ix, -d + iy)] {
        b.add(
            "Leg",
            legs.clone(),
            cuboid(
                v3(x - leg / 2.0, y - leg / 2.0, 0.0),
                v3(x + leg / 2.0, y + leg / 2.0, h - 0.03),
            ),
            false,
        );
    }
}

fn coffee_table() -> CatalogueObject {
    let mut b = Build::new();
    table(&mut b, 1.2, 0.6, 0.4, oak(), black_metal(), 0.03);
    b.add(
        "Shelf",
        oak(),
        cuboid(v3(-0.5, -0.52, 0.12), v3(0.5, -0.08, 0.14)),
        false,
    );
    b.finish(
        "coffee-table",
        "Coffee table",
        "Oak coffee table 1200 × 600, 400 mm high, with a shelf",
        ObjectCategory::Living,
        ("IfcFurniture", "TABLE"),
        Mount::Floor,
    )
}

// ---- dining ----

fn dining_table() -> CatalogueObject {
    let mut b = Build::new();
    table(&mut b, 1.8, 0.9, 0.75, oak(), oak(), 0.07);
    b.add(
        "Rail",
        oak(),
        cuboid(v3(-0.8, -0.11, 0.62), v3(0.8, -0.09, 0.72)),
        false,
    );
    b.add(
        "Rail",
        oak(),
        cuboid(v3(-0.8, -0.81, 0.62), v3(0.8, -0.79, 0.72)),
        false,
    );
    b.finish(
        "dining-table",
        "Dining table, six seats",
        "Oak table 1800 × 900, 750 mm high",
        ObjectCategory::Dining,
        ("IfcFurniture", "TABLE"),
        Mount::Floor,
    )
}

fn chair() -> CatalogueObject {
    let mut b = Build::new();
    let (w, d, seat) = (0.45, 0.5, 0.46);
    b.add(
        "Seat",
        oak(),
        soft_box(
            v3(-w / 2.0, -d, seat - 0.03),
            v3(w / 2.0, -0.02, seat),
            0.03,
            0.006,
        ),
        true,
    );
    for (x, y) in [(-0.19, -0.05), (0.19, -0.05), (-0.19, -0.47), (0.19, -0.47)] {
        let top = if y > -0.1 { 0.86 } else { seat - 0.03 };
        b.add(
            "Leg",
            oak(),
            cuboid(v3(x - 0.017, y - 0.017, 0.0), v3(x + 0.017, y + 0.017, top)),
            y > -0.1,
        );
    }
    b.add(
        "Back rail",
        oak(),
        soft_box(v3(-0.21, -0.07, 0.66), v3(0.21, -0.03, 0.86), 0.01, 0.005),
        true,
    );
    b.finish(
        "chair",
        "Dining chair",
        "Oak chair, seat height 460 mm",
        ObjectCategory::Dining,
        ("IfcFurniture", "CHAIR"),
        Mount::Floor,
    )
}

// ---- bedroom ----

fn bed(w: f64, pillows: usize, cover: Look) -> Build {
    let mut b = Build::new();
    let l = 2.0;
    let head = 0.06;
    let fw = w + 0.06;
    b.add(
        "Headboard",
        oak(),
        soft_box(
            v3(-fw / 2.0, -head, 0.0),
            v3(fw / 2.0, 0.0, 1.0),
            0.01,
            0.01,
        ),
        true,
    );
    b.add(
        "Frame",
        oak(),
        cuboid(
            v3(-fw / 2.0, -head - l - 0.03, 0.12),
            v3(fw / 2.0, -head, 0.32),
        ),
        true,
    );
    for (x, y) in [
        (-fw / 2.0 + 0.04, -head - l + 0.01),
        (fw / 2.0 - 0.04, -head - l + 0.01),
    ] {
        b.add(
            "Leg",
            oak(),
            cuboid(v3(x - 0.03, y - 0.03, 0.0), v3(x + 0.03, y + 0.03, 0.12)),
            false,
        );
    }
    b.add(
        "Mattress",
        linen(),
        soft_box(
            v3(-w / 2.0, -head - l, 0.32),
            v3(w / 2.0, -head, 0.54),
            0.05,
            0.04,
        ),
        true,
    );
    let pw = (w - 0.1) / pillows as f64;
    for i in 0..pillows {
        let x0 = -w / 2.0 + 0.05 + i as f64 * pw;
        b.add(
            "Pillow",
            linen(),
            soft_box(
                v3(x0 + 0.02, -head - 0.48, 0.54),
                v3(x0 + pw - 0.02, -head - 0.08, 0.65),
                0.08,
                0.05,
            ),
            true,
        );
    }
    b.add(
        "Duvet",
        cover,
        soft_box(
            v3(-w / 2.0 - 0.03, -head - l - 0.02, 0.48),
            v3(w / 2.0 + 0.03, -head - 0.62, 0.6),
            0.04,
            0.03,
        ),
        true,
    );
    // the duvet turned back
    b.line(v2(-w / 2.0, -head - 0.8), v2(w / 2.0, -head - 0.8));
    b
}

fn double_bed() -> CatalogueObject {
    bed(1.6, 2, fabric("Duvet, blue", [134, 160, 194])).finish(
        "double-bed",
        "Double bed 1600",
        "Oak bed for a 1600 × 2000 mattress, with its headboard",
        ObjectCategory::Bedroom,
        ("IfcFurniture", "BED"),
        Mount::Wall,
    )
}

fn single_bed() -> CatalogueObject {
    bed(0.9, 1, fabric("Duvet, ochre", [204, 164, 92])).finish(
        "single-bed",
        "Single bed 900",
        "Oak bed for a 900 × 2000 mattress, with its headboard",
        ObjectCategory::Bedroom,
        ("IfcFurniture", "BED"),
        Mount::Wall,
    )
}

fn wardrobe() -> CatalogueObject {
    let mut b = Build::new();
    let (w, d, h) = (1.0, 0.6, 2.1);
    b.add(
        "Carcass",
        laminate(),
        cuboid(v3(-w / 2.0, -d + 0.02, 0.0), v3(w / 2.0, 0.0, h)),
        true,
    );
    for (x0, x1) in [(-0.498, -0.002), (0.002, 0.498)] {
        b.add(
            "Door",
            walnut(),
            cuboid(v3(x0, -d, 0.002), v3(x1, -d + 0.02, h - 0.002)),
            false,
        );
    }
    for x in [-0.03, 0.03] {
        b.add(
            "Handle",
            stainless(),
            cuboid(v3(x - 0.006, -d - 0.025, 0.95), v3(x + 0.006, -d, 1.25)),
            false,
        );
    }
    b.line(v2(-w / 2.0, -d), v2(w / 2.0, -d));
    // the hanging rail
    b.line(v2(-w / 2.0 + 0.05, -d / 2.0), v2(w / 2.0 - 0.05, -d / 2.0));
    b.finish(
        "wardrobe",
        "Wardrobe 1000",
        "Two-door wardrobe 1000 × 600, 2100 mm high",
        ObjectCategory::Bedroom,
        ("IfcFurniture", "USERDEFINED"),
        Mount::Wall,
    )
}

fn bedside() -> CatalogueObject {
    let mut b = Build::new();
    let (w, d, h) = (0.45, 0.4, 0.5);
    b.add(
        "Body",
        oak(),
        soft_box(
            v3(-w / 2.0, -d + 0.02, 0.0),
            v3(w / 2.0, 0.0, h),
            0.01,
            0.004,
        ),
        true,
    );
    for (z0, z1) in [(0.05, 0.26), (0.28, 0.48)] {
        b.add(
            "Drawer",
            walnut(),
            cuboid(v3(-0.215, -d, z0), v3(0.215, -d + 0.02, z1)),
            false,
        );
        b.add(
            "Knob",
            stainless(),
            cylinder(
                v2(0.0, -d - 0.01),
                0.012,
                (z0 + z1) / 2.0 - 0.012,
                (z0 + z1) / 2.0 + 0.012,
            ),
            false,
        );
    }
    b.finish(
        "bedside-table",
        "Bedside table",
        "Oak bedside table with two drawers",
        ObjectCategory::Bedroom,
        ("IfcFurniture", "USERDEFINED"),
        Mount::Wall,
    )
}

// ---- office ----

fn desk() -> CatalogueObject {
    let mut b = Build::new();
    let (w, d, h) = (1.4, 0.7, 0.74);
    b.add(
        "Top",
        laminate(),
        soft_box(
            v3(-w / 2.0, -d, h - 0.025),
            v3(w / 2.0, 0.0, h),
            0.01,
            0.003,
        ),
        true,
    );
    for x in [-w / 2.0 + 0.05, w / 2.0 - 0.05] {
        for y in [-0.06, -d + 0.06] {
            b.add(
                "Leg",
                black_metal(),
                cuboid(
                    v3(x - 0.02, y - 0.02, 0.0),
                    v3(x + 0.02, y + 0.02, h - 0.025),
                ),
                false,
            );
        }
        b.add(
            "Foot rail",
            black_metal(),
            cuboid(v3(x - 0.02, -d + 0.04, 0.0), v3(x + 0.02, -0.04, 0.04)),
            false,
        );
    }
    b.add(
        "Modesty panel",
        laminate(),
        cuboid(
            v3(-w / 2.0 + 0.08, -0.05, 0.35),
            v3(w / 2.0 - 0.08, -0.032, h - 0.025),
        ),
        false,
    );
    b.finish(
        "desk",
        "Desk 1400",
        "Desk 1400 × 700 on a black steel frame",
        ObjectCategory::Office,
        ("IfcFurniture", "DESK"),
        Mount::Wall,
    )
}

/// Every starter object, by category.
pub fn starters() -> Vec<CatalogueObject> {
    vec![
        washbasin(),
        wc(),
        bathtub(),
        shower(),
        sink_unit(),
        base_cabinet(),
        cooker_unit(),
        wall_cabinet(),
        fridge(),
        sofa(),
        armchair(),
        coffee_table(),
        dining_table(),
        chair(),
        double_bed(),
        single_bed(),
        wardrobe(),
        bedside(),
        desk(),
    ]
    .into_iter()
    .chain(services::starters())
    .collect()
}

/// The starter object with this key.
pub fn starter(key: &str) -> Option<CatalogueObject> {
    starters().into_iter().find(|c| c.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Twice the volume a closed mesh encloses (positive when its faces
    /// point outwards).
    fn volume(m: &EditMesh) -> f64 {
        let mut v = 0.0;
        for f in 0..m.faces.len() {
            for [a, b, c] in m.face_triangles(f) {
                let (a, b, c) = (
                    m.vertices[a as usize],
                    m.vertices[b as usize],
                    m.vertices[c as usize],
                );
                v += a.dot(b.cross(c)) / 6.0;
            }
        }
        v
    }

    #[test]
    fn every_starter_is_closed_outward_and_in_its_own_axes() {
        let all = starters();
        assert!(all.len() >= 15);
        let mut keys: Vec<&str> = all.iter().map(|c| c.key.as_str()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), all.len(), "keys are unique");
        for c in &all {
            c.object.validate().unwrap();
            assert_eq!(c.materials.len(), c.object.parts.len(), "{}", c.name);
            for p in &c.object.parts {
                let v = volume(&p.mesh);
                assert!(v > 0.0, "{} {}: volume {v}", c.name, p.name);
            }
            let (lo, hi) = c.object.bounds();
            assert!(lo.z >= -1e-9 && hi.y <= 1e-9, "{}: {lo} {hi}", c.name);
            // sockets and switches stand a few millimetres off their wall
            let depth = if c.object.category.is_services() {
                0.005
            } else {
                0.1
            };
            assert!(hi.x > 0.0 && lo.x < 0.0 && lo.y < -depth, "{}", c.name);
            assert!(!c.object.plan_symbol().is_empty());
            let k = c.object.ifc.normalized();
            assert_eq!(k, c.object.ifc, "{}: a predefined type IFC knows", c.name);
            // every material comes with a name and a look
            assert!(c.materials.iter().all(|m| m.is_some()));
        }
        // a bowl's inside is lower than its rim
        let basin = starter("starter/washbasin").unwrap();
        let (lo, hi) = basin.object.bounds();
        assert!(lo.z > 0.6 && (hi.z - 1.01).abs() < 0.01, "{lo} {hi}");
    }
}
