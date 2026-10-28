//! Building services fixtures: sockets, switches, lights, a consumer unit
//! and a smoke detector; a floor drain, a boiler and an outside tap; a
//! radiator, ceiling diffusers, a split air conditioner and an extract
//! fan. Each is a small recognisable shape with the plan symbol drawings
//! use for it (the IEC 60617 / BS EN 60617 family for electrical points),
//! drawn a little larger than the device so it reads at 1:50 and 1:100.
//!
//! Wall devices stand at their usual height above the floor (sockets at
//! 300 mm, switches at 1100 mm, to their underside); lights, diffusers and
//! detectors hang from the ceiling, centred on their own middle.

use super::{
    appliance_white, chrome, cuboid, cylinder, look, metal, rrect, soft_box, squircle, stack,
    stainless, v2, v3, Build, Look,
};
use crate::objects::{CatalogueObject, Mount, ObjectCategory};
use crate::Hatch;
use glam::DVec2;
use std::f64::consts::{FRAC_PI_4, TAU};

// ---- materials ----

fn accessory_white() -> Look {
    look(
        "Accessory plastic, white",
        "plastic",
        Hatch::Empty,
        [247, 247, 244],
        0.35,
    )
}
fn dark_plastic() -> Look {
    look(
        "Plastic, charcoal",
        "plastic",
        Hatch::Empty,
        [46, 48, 52],
        0.5,
    )
}
fn opal() -> Look {
    let mut l = look("Glass, opal", "glass", Hatch::Empty, [252, 250, 242], 0.1);
    l.1.opacity = 0.85;
    l
}
fn brass() -> Look {
    metal("Brass", [196, 160, 92], 0.3)
}

// ---- symbol pieces ----

fn ring(c: DVec2, r: f64) -> Vec<DVec2> {
    squircle(c, r, r, 2.0, 24)
}

/// An arc about `c` from angle `a0` to `a1` (radians), as lines.
fn arc(b: &mut Build, c: DVec2, r: f64, a0: f64, a1: f64) {
    let n = 12;
    for i in 0..n {
        let t = |k: usize| a0 + (a1 - a0) * k as f64 / n as f64;
        b.line(
            c + DVec2::from_angle(t(i)) * r,
            c + DVec2::from_angle(t(i + 1)) * r,
        );
    }
}

/// The diagonal cross of a lighting point, inside a circle of `r`.
fn cross(b: &mut Build, c: DVec2, r: f64) {
    for a in [FRAC_PI_4, 3.0 * FRAC_PI_4] {
        let d = DVec2::from_angle(a) * r;
        b.line(c - d, c + d);
    }
}

/// A socket outlet: a half dome off the wall, its flat side parallel to
/// it, one stroke out of the dome a socket.
fn socket_symbol(b: &mut Build, sockets: usize) {
    let (c, r) = (v2(0.0, -0.05), 0.09);
    b.line(v2(0.0, 0.0), c);
    b.line(c - DVec2::X * r, c + DVec2::X * r);
    arc(b, c, r, std::f64::consts::PI, TAU);
    let spread = 0.35;
    for i in 0..sockets {
        let a = -std::f64::consts::FRAC_PI_2 + spread * (i as f64 - (sockets - 1) as f64 / 2.0);
        let d = DVec2::from_angle(a);
        b.line(c + d * r, c + d * (r + 0.07));
    }
}

/// A switch: a small circle off the wall with a stroke ending in a tick
/// for each gang.
fn switch_symbol(b: &mut Build, gangs: usize) {
    let (c, r) = (v2(0.0, -0.07), 0.035);
    b.line(v2(0.0, 0.0), c + v2(0.0, r));
    b.outline(ring(c, r));
    let d = v2(1.0, -1.0).normalize();
    let (a, e) = (c + d * r, c + d * (r + 0.16));
    b.line(a, e);
    for g in 0..gangs {
        let at = e - d * 0.04 * g as f64;
        let n = d.perp() * 0.045;
        b.line(at, at + n);
    }
}

// ---- electrical ----

/// A wall plate `w` wide and `h` high, standing `t` off the wall.
fn plate(b: &mut Build, w: f64, h: f64, t: f64) {
    b.add(
        "Plate",
        accessory_white(),
        soft_box(v3(-w / 2.0, -t, 0.0), v3(w / 2.0, 0.0, h), 0.006, 0.002),
        false,
    );
}

/// The face of a socket and its switch, centred at `x`.
fn socket_face(b: &mut Build, x: f64, h: f64) {
    b.add(
        "Socket",
        accessory_white(),
        soft_box(
            v3(x - 0.026, -0.013, h / 2.0 - 0.03),
            v3(x + 0.026, -0.009, h / 2.0 + 0.026),
            0.004,
            0.001,
        ),
        false,
    );
    b.add(
        "Switch",
        dark_plastic(),
        cuboid(
            v3(x + 0.008, -0.016, h / 2.0 + 0.012),
            v3(x + 0.022, -0.009, h / 2.0 + 0.026),
        ),
        false,
    );
}

fn socket(sockets: usize) -> CatalogueObject {
    let mut b = Build::new();
    let (w, h) = if sockets == 2 {
        (0.146, 0.086)
    } else {
        (0.086, 0.086)
    };
    plate(&mut b, w, h, 0.009);
    let pitch = if sockets == 2 { 0.068 } else { 0.0 };
    for i in 0..sockets {
        socket_face(&mut b, pitch * (i as f64 - (sockets - 1) as f64 / 2.0), h);
    }
    socket_symbol(&mut b, sockets);
    b.at(0.3);
    if sockets == 2 {
        b.finish(
            "socket-double",
            "Double socket",
            "Switched twin socket outlet, 146 × 86, at 300 mm",
            ObjectCategory::Electrical,
            ("IfcOutlet", "POWEROUTLET"),
            Mount::Wall,
        )
    } else {
        b.finish(
            "socket-single",
            "Single socket",
            "Switched socket outlet, 86 × 86, at 300 mm",
            ObjectCategory::Electrical,
            ("IfcOutlet", "POWEROUTLET"),
            Mount::Wall,
        )
    }
}

fn light_switch(gangs: usize) -> CatalogueObject {
    let mut b = Build::new();
    let s = 0.086;
    plate(&mut b, s, s, 0.009);
    let w = 0.056 / gangs as f64;
    for g in 0..gangs {
        let x = -0.028 + w * (g as f64 + 0.5);
        b.add(
            "Rocker",
            accessory_white(),
            soft_box(
                v3(x - w / 2.0 + 0.002, -0.014, 0.015),
                v3(x + w / 2.0 - 0.002, -0.009, s - 0.015),
                0.003,
                0.001,
            ),
            false,
        );
    }
    switch_symbol(&mut b, gangs);
    b.at(1.1);
    let (key, name, ifc) = if gangs == 1 {
        ("switch-1-gang", "Light switch, 1 gang", "TOGGLESWITCH")
    } else {
        ("switch-2-gang", "Light switch, 2 gang", "TOGGLESWITCH")
    };
    b.finish(
        key,
        name,
        &format!("{gangs}-gang rocker light switch, 86 × 86, at 1100 mm"),
        ObjectCategory::Electrical,
        ("IfcSwitchingDevice", ifc),
        Mount::Wall,
    )
}

fn consumer_unit() -> CatalogueObject {
    let mut b = Build::new();
    let (w, d, h) = (0.4, 0.12, 0.3);
    b.add(
        "Enclosure",
        appliance_white(),
        soft_box(v3(-w / 2.0, -d, 0.0), v3(w / 2.0, 0.0, h), 0.01, 0.004),
        true,
    );
    b.add(
        "Breakers",
        dark_plastic(),
        cuboid(v3(-0.17, -d - 0.006, 0.12), v3(0.17, -d, 0.18)),
        false,
    );
    b.add(
        "Cover",
        look(
            "Plastic, smoked clear",
            "plastic",
            Hatch::Empty,
            [120, 124, 130],
            0.1,
        ),
        cuboid(v3(-0.18, -d - 0.016, 0.06), v3(0.18, -d - 0.006, 0.24)),
        false,
    );
    // a distribution board: its box with one half hatched
    b.line(v2(-w / 2.0, -d), v2(w / 2.0, 0.0));
    for t in [0.25, 0.5, 0.75] {
        let a = v2(-w / 2.0 + w * t, -d);
        b.line(a, a + v2(w * (1.0 - t), d * (1.0 - t)));
    }
    b.at(1.35);
    b.finish(
        "consumer-unit",
        "Consumer unit",
        "Distribution board for the house's circuits, 400 × 300, at 1350 mm",
        ObjectCategory::Electrical,
        ("IfcElectricDistributionBoard", "CONSUMERUNIT"),
        Mount::Wall,
    )
}

fn smoke_detector() -> CatalogueObject {
    let mut b = Build::new();
    let (r, c) = (0.055, v2(0.0, -0.055));
    b.add(
        "Detector",
        accessory_white(),
        stack(&[
            (ring(c, r - 0.01), 0.0),
            (ring(c, r), 0.012),
            (ring(c, r), 0.045),
        ]),
        false,
    );
    b.add(
        "Indicator",
        look(
            "Indicator, red",
            "plastic",
            Hatch::Empty,
            [200, 40, 36],
            0.2,
        ),
        cylinder(c + v2(0.025, 0.0), 0.004, 0.0, 0.002),
        false,
    );
    // a circle with an S in it
    b.outline(ring(c, 0.1));
    let s = [
        v2(0.035, 0.035),
        v2(-0.035, 0.035),
        v2(-0.035, 0.0),
        v2(0.035, 0.0),
        v2(0.035, -0.035),
        v2(-0.035, -0.035),
    ];
    for w in s.windows(2) {
        b.line(c + w[0], c + w[1]);
    }
    b.finish(
        "smoke-detector",
        "Smoke detector",
        "Optical smoke alarm on the ceiling",
        ObjectCategory::Electrical,
        ("IfcSensor", "SMOKESENSOR"),
        Mount::Ceiling,
    )
}

// ---- lighting ----

fn pendant() -> CatalogueObject {
    let mut b = Build::new();
    let (r, c, h) = (0.18, v2(0.0, -0.18), 0.6);
    b.add(
        "Rose",
        accessory_white(),
        cylinder(c, 0.05, h - 0.03, h),
        false,
    );
    b.add(
        "Flex",
        dark_plastic(),
        cylinder(c, 0.004, 0.19, h - 0.03),
        false,
    );
    b.add(
        "Shade",
        opal(),
        stack(&[
            (ring(c, r), 0.0),
            (ring(c, r - 0.01), 0.04),
            (ring(c, 0.1), 0.15),
            (ring(c, 0.04), 0.2),
        ]),
        false,
    );
    b.outline(ring(c, 0.15));
    cross(&mut b, c, 0.15);
    b.finish(
        "ceiling-light",
        "Ceiling light, pendant",
        "Opal glass pendant on a 400 mm flex from a ceiling rose",
        ObjectCategory::Lighting,
        ("IfcLightFixture", "POINTSOURCE"),
        Mount::Ceiling,
    )
}

fn downlight() -> CatalogueObject {
    let mut b = Build::new();
    let c = v2(0.0, -0.045);
    b.add(
        "Trim",
        accessory_white(),
        stack(&[(ring(c, 0.043), 0.0), (ring(c, 0.045), 0.008)]),
        false,
    );
    b.add("Lens", opal(), cylinder(c, 0.032, 0.0, 0.002), false);
    b.outline(ring(c, 0.08));
    b.outline(ring(c, 0.04));
    cross(&mut b, c, 0.08);
    b.finish(
        "downlight",
        "Recessed downlight",
        "LED downlight, 90 mm trim flush with the ceiling",
        ObjectCategory::Lighting,
        ("IfcLightFixture", "POINTSOURCE"),
        Mount::Ceiling,
    )
}

fn wall_light() -> CatalogueObject {
    let mut b = Build::new();
    b.add(
        "Mount",
        accessory_white(),
        soft_box(v3(-0.05, -0.02, 0.0), v3(0.05, 0.0, 0.12), 0.01, 0.003),
        false,
    );
    b.add(
        "Arm",
        chrome(),
        cuboid(v3(-0.008, -0.12, 0.05), v3(0.008, -0.02, 0.066)),
        false,
    );
    let c = v2(0.0, -0.13);
    b.add(
        "Shade",
        opal(),
        stack(&[
            (ring(c, 0.045), 0.04),
            (ring(c, 0.08), 0.15),
            (ring(c, 0.085), 0.2),
        ]),
        false,
    );
    // a lighting point on a bracket: the wall plate and the circle
    b.line(v2(-0.07, 0.0), v2(0.07, 0.0));
    b.line(v2(0.0, 0.0), c + v2(0.0, 0.1));
    b.outline(ring(c, 0.1));
    cross(&mut b, c, 0.1);
    b.at(1.8);
    b.finish(
        "wall-light",
        "Wall light",
        "Up-lighting wall bracket with an opal shade, at 1800 mm",
        ObjectCategory::Lighting,
        ("IfcLightFixture", "DIRECTIONSOURCE"),
        Mount::Wall,
    )
}

// ---- plumbing ----

fn floor_drain() -> CatalogueObject {
    let mut b = Build::new();
    let (s, c) = (0.15, v2(0.0, -0.075));
    b.add(
        "Frame",
        stainless(),
        cuboid(v3(-s / 2.0, -s, 0.0), v3(s / 2.0, 0.0, 0.004)),
        true,
    );
    for i in 0..5 {
        let x = -0.05 + 0.025 * i as f64;
        b.add(
            "Grate bar",
            stainless(),
            cuboid(
                v3(x - 0.004, -s + 0.015, 0.004),
                v3(x + 0.004, -0.015, 0.007),
            ),
            false,
        );
    }
    b.outline(rrect(c, 0.11, 0.11, 0.0, 0));
    b.outline(ring(c, 0.035));
    b.line(c - DVec2::X * 0.035, c + DVec2::X * 0.035);
    b.line(c - DVec2::Y * 0.035, c + DVec2::Y * 0.035);
    b.finish(
        "floor-drain",
        "Floor drain",
        "Stainless gully with a 150 × 150 grate, flush with the floor",
        ObjectCategory::Plumbing,
        ("IfcWasteTerminal", "FLOORWASTE"),
        Mount::Floor,
    )
}

fn boiler() -> CatalogueObject {
    let mut b = Build::new();
    let (w, d, h) = (0.42, 0.3, 0.7);
    b.add(
        "Casing",
        appliance_white(),
        soft_box(v3(-w / 2.0, -d, 0.0), v3(w / 2.0, 0.0, h), 0.02, 0.008),
        true,
    );
    b.add(
        "Display",
        dark_plastic(),
        cuboid(v3(-0.1, -d - 0.004, 0.06), v3(0.1, -d, 0.12)),
        false,
    );
    b.add(
        "Flue",
        appliance_white(),
        cylinder(v2(0.0, -0.15), 0.05, h, h + 0.12),
        false,
    );
    b.outline(ring(v2(0.0, -d / 2.0), 0.09));
    b.line(v2(-w / 2.0, -d), v2(w / 2.0, 0.0));
    b.at(1.0);
    b.finish(
        "boiler",
        "Combi boiler",
        "Wall-hung gas combination boiler for heating and hot water",
        ObjectCategory::Plumbing,
        ("IfcBoiler", "WATER"),
        Mount::Wall,
    )
}

fn outside_tap() -> CatalogueObject {
    let mut b = Build::new();
    b.add(
        "Wall plate",
        brass(),
        soft_box(v3(-0.026, -0.008, 0.0), v3(0.026, 0.0, 0.052), 0.012, 0.002),
        false,
    );
    b.add(
        "Body",
        brass(),
        cuboid(v3(-0.014, -0.08, 0.012), v3(0.014, -0.006, 0.04)),
        false,
    );
    b.add(
        "Spout",
        brass(),
        cylinder(v2(0.0, -0.075), 0.011, 0.0, 0.012),
        false,
    );
    b.add(
        "Handle",
        look("Handle, red", "plastic", Hatch::Empty, [176, 36, 32], 0.4),
        cuboid(v3(-0.035, -0.05, 0.04), v3(0.035, -0.036, 0.052)),
        false,
    );
    // a draw-off tap: its pipe off the wall and a valve's bow tie
    b.line(v2(0.0, 0.0), v2(0.0, -0.06));
    b.outline(vec![
        v2(-0.04, -0.06),
        v2(0.04, -0.14),
        v2(0.04, -0.06),
        v2(-0.04, -0.14),
    ]);
    b.at(0.6);
    b.finish(
        "outside-tap",
        "Outside tap",
        "Brass bib tap with a hose union on an outside wall, at 600 mm",
        ObjectCategory::Plumbing,
        ("IfcValve", "FAUCET"),
        Mount::Wall,
    )
}

// ---- heating and ventilation ----

fn radiator() -> CatalogueObject {
    let mut b = Build::new();
    let (w, h) = (0.8, 0.6);
    for (y0, y1) in [(-0.09, -0.068), (-0.058, -0.036)] {
        b.add(
            "Panel",
            appliance_white(),
            soft_box(v3(-w / 2.0, y0, 0.0), v3(w / 2.0, y1, h), 0.005, 0.004),
            false,
        );
    }
    for x in [-0.3, 0.3] {
        b.add(
            "Bracket",
            appliance_white(),
            cuboid(v3(x - 0.02, -0.036, h - 0.1), v3(x + 0.02, 0.0, h - 0.06)),
            false,
        );
    }
    b.add(
        "Thermostatic valve",
        appliance_white(),
        cylinder(v2(-w / 2.0 - 0.03, -0.064), 0.017, 0.0, 0.1),
        false,
    );
    b.add(
        "Valve",
        chrome(),
        cylinder(v2(w / 2.0 + 0.03, -0.064), 0.012, 0.0, 0.06),
        false,
    );
    // its outline, hatched across
    let (y0, y1) = (-0.1, -0.03);
    b.outline(rrect(v2(0.0, (y0 + y1) / 2.0), w, y1 - y0, 0.0, 0));
    for i in 0..8 {
        let x = -w / 2.0 + w * (i as f64 + 0.5) / 8.0;
        b.line(v2(x - 0.03, y0), v2(x + 0.03, y1));
    }
    b.at(0.15);
    b.finish(
        "radiator",
        "Radiator 800 × 600",
        "Double panel steel radiator with a thermostatic valve, 150 mm off the floor",
        ObjectCategory::Hvac,
        ("IfcSpaceHeater", "RADIATOR"),
        Mount::Wall,
    )
}

fn diffuser(extract: bool) -> CatalogueObject {
    let mut b = Build::new();
    let (s, c) = (0.4, v2(0.0, -0.2));
    b.add(
        "Face plate",
        appliance_white(),
        stack(&[
            (rrect(c, s - 0.02, s - 0.02, 0.004, 1), 0.0),
            (rrect(c, s, s, 0.004, 1), 0.012),
            (rrect(c, s, s, 0.004, 1), 0.02),
        ]),
        false,
    );
    b.add(
        "Core",
        look(
            "Grille, light grey",
            "metal",
            Hatch::Empty,
            [200, 202, 204],
            0.4,
        ),
        cuboid(
            (c - DVec2::splat(0.12)).extend(0.0),
            (c + DVec2::splat(0.12)).extend(0.006),
        ),
        false,
    );
    b.outline(rrect(c, s, s, 0.0, 0));
    b.outline(rrect(c, 0.24, 0.24, 0.0, 0));
    if extract {
        // an extract grille: its bars
        for i in 1..4 {
            let y = c.y - 0.12 + 0.06 * i as f64;
            b.line(v2(-0.12, y), v2(0.12, y));
        }
    } else {
        // a supply diffuser: the four ways it blows
        for a in [FRAC_PI_4, 3.0 * FRAC_PI_4] {
            let d = DVec2::from_angle(a) * s / 2.0 * std::f64::consts::SQRT_2;
            b.line(c - d, c + d);
        }
    }
    let (key, name, ifc, what) = if extract {
        (
            "extract-grille",
            "Ceiling extract grille",
            "GRILLE",
            "Square extract grille, 400 × 400, in the ceiling",
        )
    } else {
        (
            "supply-diffuser",
            "Ceiling supply diffuser",
            "DIFFUSER",
            "Four-way square supply diffuser, 400 × 400, in the ceiling",
        )
    };
    b.finish(
        key,
        name,
        what,
        ObjectCategory::Hvac,
        ("IfcAirTerminal", ifc),
        Mount::Ceiling,
    )
}

fn split_ac() -> CatalogueObject {
    let mut b = Build::new();
    let (w, d, h) = (0.8, 0.2, 0.28);
    b.add(
        "Casing",
        appliance_white(),
        soft_box(v3(-w / 2.0, -d, 0.0), v3(w / 2.0, 0.0, h), 0.04, 0.03),
        true,
    );
    b.add(
        "Louvre",
        look(
            "Plastic, light grey",
            "plastic",
            Hatch::Empty,
            [214, 216, 218],
            0.4,
        ),
        cuboid(v3(-0.34, -d - 0.012, 0.02), v3(0.34, -d + 0.02, 0.05)),
        false,
    );
    // which way it blows
    for x in [-0.25, 0.0, 0.25] {
        let a = v2(x, -d);
        b.line(a, a - v2(0.0, 0.12));
        b.line(a - v2(0.0, 0.12), a - v2(0.03, 0.09));
        b.line(a - v2(0.0, 0.12), a - v2(-0.03, 0.09));
    }
    b.at(2.0);
    b.finish(
        "split-ac",
        "Split air conditioner",
        "Wall-mounted indoor unit of a split system, at 2000 mm",
        ObjectCategory::Hvac,
        ("IfcUnitaryEquipment", "SPLITSYSTEM"),
        Mount::Wall,
    )
}

fn extract_fan() -> CatalogueObject {
    let mut b = Build::new();
    let s = 0.18;
    b.add(
        "Body",
        accessory_white(),
        soft_box(v3(-s / 2.0, -0.035, 0.0), v3(s / 2.0, 0.0, s), 0.012, 0.006),
        true,
    );
    for i in 0..5 {
        let z = 0.035 + 0.025 * i as f64;
        b.add(
            "Grille slat",
            accessory_white(),
            cuboid(v3(-0.06, -0.04, z), v3(0.06, -0.035, z + 0.012)),
            false,
        );
    }
    // a fan: a circle with its blades
    let c = v2(0.0, -0.11);
    b.line(v2(0.0, -0.035), c + v2(0.0, 0.075));
    b.outline(ring(c, 0.075));
    for k in 0..3 {
        let a = TAU * k as f64 / 3.0 + 0.3;
        b.line(c, c + DVec2::from_angle(a) * 0.06);
    }
    b.at(2.1);
    b.finish(
        "extract-fan",
        "Extract fan",
        "Bathroom or kitchen axial extract fan, 100 mm, at 2100 mm",
        ObjectCategory::Hvac,
        ("IfcFan", "PROPELLORAXIAL"),
        Mount::Wall,
    )
}

/// The services starters, by category.
pub(super) fn starters() -> Vec<CatalogueObject> {
    vec![
        socket(2),
        socket(1),
        light_switch(1),
        light_switch(2),
        consumer_unit(),
        smoke_detector(),
        pendant(),
        downlight(),
        wall_light(),
        floor_drain(),
        boiler(),
        outside_tap(),
        radiator(),
        diffuser(false),
        diffuser(true),
        split_ac(),
        extract_fan(),
    ]
}
