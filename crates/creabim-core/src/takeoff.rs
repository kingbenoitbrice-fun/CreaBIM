//! Quantity take-off: what the model is made of, measured.
//!
//! Every wall, slab, roof, ceiling, column, beam and footing gives one
//! line per material it is built of: a layered element one line for each
//! layer of its assembly (air gaps left out), anything else one line for
//! its own material. Each line has an area (a wall's face less its
//! openings, a slab's or ceiling's plan area, a roof's slope area), a
//! volume, a length for walls, columns, beams and strip footings, and a
//! count.
//!
//! A material can carry a [`Rate`]: a price for each square metre, cubic
//! metre, metre or piece. Totals by material then cost themselves out.
//! Rates are plain numbers in whatever currency the project is priced in.

use crate::{assembly_of, ElementId, ElementKind, FootingKind, LayerFunction, Model};
use serde::{Deserialize, Serialize};

/// What a rate is priced per.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RateUnit {
    /// Per square metre of area.
    #[default]
    Area,
    /// Per cubic metre.
    Volume,
    /// Per metre of length.
    Length,
    /// Per piece.
    Each,
}

impl RateUnit {
    pub const ALL: [RateUnit; 4] = [
        RateUnit::Area,
        RateUnit::Volume,
        RateUnit::Length,
        RateUnit::Each,
    ];

    /// "per m²".
    pub fn label(self) -> &'static str {
        match self {
            RateUnit::Area => "per m²",
            RateUnit::Volume => "per m³",
            RateUnit::Length => "per m",
            RateUnit::Each => "each",
        }
    }

    /// The quantity of `q` this unit prices.
    pub fn of(self, q: &Quantity) -> f64 {
        match self {
            RateUnit::Area => q.area,
            RateUnit::Volume => q.volume,
            RateUnit::Length => q.length,
            RateUnit::Each => q.count as f64,
        }
    }
}

/// A material's price: `amount` for each `per`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rate {
    pub amount: f64,
    #[serde(default)]
    pub per: RateUnit,
}

/// Measured amounts, in metres, square metres and cubic metres.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Quantity {
    pub area: f64,
    pub volume: f64,
    pub length: f64,
    pub count: usize,
}

impl Quantity {
    fn add(&mut self, o: &Quantity) {
        self.area += o.area;
        self.volume += o.volume;
        self.length += o.length;
        self.count += o.count;
    }
}

/// One material of one element.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub element: ElementId,
    /// "Walls", "Slabs", "Roofs", "Ceilings", "Columns", "Beams", "Footings".
    pub category: &'static str,
    pub level: Option<ElementId>,
    pub material: Option<ElementId>,
    /// The layer of the element's assembly, counted from its exterior
    /// face or top, and what it does; `None` for an element of one
    /// material.
    pub layer: Option<(usize, LayerFunction)>,
    /// Thickness of the layer, or of the element, metres.
    pub thickness: f64,
    pub quantity: Quantity,
}

/// Every take-off line of the model, element by element in model order.
pub fn lines(model: &Model) -> Vec<Line> {
    let mut out = Vec::new();
    for e in model.iter() {
        // the measure of the whole element and its thickness
        let (category, level, whole, thickness) = match &e.kind {
            ElementKind::Wall(w) => {
                let openings: f64 = model
                    .dependents(e.id)
                    .into_iter()
                    .filter_map(|d| match &model.get(d)?.kind {
                        ElementKind::Opening(o) => Some(o.width * o.height),
                        _ => None,
                    })
                    .sum();
                let area = (w.length() * w.height - openings).max(0.0);
                (
                    "Walls",
                    w.level,
                    Quantity {
                        area,
                        volume: area * w.thickness,
                        length: w.length(),
                        count: 1,
                    },
                    w.thickness,
                )
            }
            ElementKind::Slab(s) => {
                let area = s.area();
                (
                    "Slabs",
                    s.level,
                    Quantity {
                        area,
                        volume: area * s.thickness,
                        length: 0.0,
                        count: 1,
                    },
                    s.thickness,
                )
            }
            ElementKind::Roof(r) => {
                // each face's plan area, tilted up its slope
                let area: f64 = r
                    .faces()
                    .iter()
                    .map(|f| {
                        let plan = crate::polygon::signed_area2(&f.outline).abs() / 2.0;
                        plan * (1.0 + f.g.length_squared()).sqrt()
                    })
                    .sum();
                (
                    "Roofs",
                    r.level,
                    Quantity {
                        area,
                        volume: area * r.thickness,
                        length: 0.0,
                        count: 1,
                    },
                    r.thickness,
                )
            }
            ElementKind::Ceiling(c) => {
                let outline = c.outline(model).unwrap_or_default();
                let area = crate::polygon::signed_area2(&outline).abs() / 2.0;
                (
                    "Ceilings",
                    c.level,
                    Quantity {
                        area,
                        volume: area * c.thickness,
                        length: 0.0,
                        count: 1,
                    },
                    c.thickness,
                )
            }
            ElementKind::Column(c) => {
                let height = c.height_in(model);
                (
                    "Columns",
                    c.level,
                    Quantity {
                        area: 0.0,
                        volume: c.profile.area() * height,
                        length: height,
                        count: 1,
                    },
                    0.0,
                )
            }
            ElementKind::Beam(b) => (
                "Beams",
                b.level,
                Quantity {
                    area: 0.0,
                    volume: b.width * b.depth * b.length(),
                    length: b.length(),
                    count: 1,
                },
                0.0,
            ),
            ElementKind::Footing(f) => {
                let (area, length) = match f.kind {
                    FootingKind::Strip => {
                        let (a, b) = f.line(model);
                        (a.distance(b) * f.width, a.distance(b))
                    }
                    FootingKind::Pad => (
                        crate::polygon::signed_area2(&f.pad_outline(model)).abs() / 2.0,
                        0.0,
                    ),
                };
                (
                    "Footings",
                    f.level,
                    Quantity {
                        area,
                        volume: area * f.thickness,
                        length,
                        count: 1,
                    },
                    f.thickness,
                )
            }
            _ => continue,
        };
        let layered = match &e.kind {
            ElementKind::Wall(_) | ElementKind::Slab(_) | ElementKind::Roof(_) => {
                assembly_of(model, e)
            }
            _ => None,
        };
        if let Some((_, asm)) = layered {
            for (i, l) in asm.layers.iter().enumerate() {
                if l.function == LayerFunction::AirGap && l.material.is_none() {
                    continue;
                }
                out.push(Line {
                    element: e.id,
                    category,
                    level: Some(level),
                    material: l.material,
                    layer: Some((i, l.function)),
                    thickness: l.thickness,
                    quantity: Quantity {
                        volume: whole.area * l.thickness,
                        ..whole
                    },
                });
            }
        } else {
            out.push(Line {
                element: e.id,
                category,
                level: Some(level),
                material: crate::material_of(model, e).map(|(m, _)| m.id),
                layer: None,
                thickness,
                quantity: whole,
            });
        }
    }
    out
}

/// The quantities of one material, or of one level, added up.
#[derive(Clone, Debug, PartialEq)]
pub struct Total {
    /// The material, or the level; `None` for lines without one.
    pub key: Option<ElementId>,
    pub quantity: Quantity,
    /// How many elements it is in (an element counts once even with two
    /// layers of it).
    pub elements: usize,
    /// The material's rate, and what it comes to.
    pub rate: Option<Rate>,
    pub cost: Option<f64>,
}

fn total_by(
    model: &Model,
    lines: &[Line],
    key: impl Fn(&Line) -> Option<ElementId>,
    priced: bool,
) -> Vec<Total> {
    let mut order: Vec<Option<ElementId>> = Vec::new();
    let mut sums: std::collections::HashMap<Option<ElementId>, (Quantity, Vec<ElementId>)> =
        Default::default();
    for l in lines {
        let k = key(l);
        let entry = sums.entry(k).or_insert_with(|| {
            order.push(k);
            Default::default()
        });
        entry.0.add(&l.quantity);
        if !entry.1.contains(&l.element) {
            entry.1.push(l.element);
        }
    }
    order
        .into_iter()
        .map(|k| {
            let (mut quantity, elements) = sums.remove(&k).unwrap_or_default();
            // a piece is an element, not a layer of one
            quantity.count = elements.len();
            let rate = if priced {
                k.and_then(|m| rate_of(model, m))
            } else {
                None
            };
            let cost = rate.map(|r| r.amount * r.per.of(&quantity));
            Total {
                key: k,
                quantity,
                elements: elements.len(),
                rate,
                cost,
            }
        })
        .collect()
}

/// The rate set on material `id`.
pub fn rate_of(model: &Model, id: ElementId) -> Option<Rate> {
    match &model.get(id)?.kind {
        ElementKind::Material(m) => m.rate,
        _ => None,
    }
}

/// Totals for each material, in the order they first appear, priced by
/// each material's rate.
pub fn by_material(model: &Model, lines: &[Line]) -> Vec<Total> {
    total_by(model, lines, |l| l.material, true)
}

/// Totals for each level, lowest first. Their cost is the sum of the
/// priced lines on that level.
pub fn by_level(model: &Model, lines: &[Line]) -> Vec<Total> {
    let mut totals = total_by(model, lines, |l| l.level, false);
    for t in &mut totals {
        let cost: Vec<f64> = lines
            .iter()
            .filter(|l| l.level == t.key)
            .filter_map(|l| {
                let r = rate_of(model, l.material?)?;
                // pieces are counted per element on each line
                Some(r.amount * r.per.of(&l.quantity))
            })
            .collect();
        t.cost = (!cost.is_empty()).then(|| cost.iter().sum());
    }
    totals.sort_by(|a, b| {
        let z = |t: &Total| {
            t.key
                .and_then(|l| model.level(l))
                .map_or(f64::NEG_INFINITY, |l| l.elevation)
        };
        z(a).total_cmp(&z(b))
    });
    totals
}

/// What the priced materials of the model come to.
pub fn total_cost(model: &Model, lines: &[Line]) -> f64 {
    by_material(model, lines)
        .iter()
        .filter_map(|t| t.cost)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::{layered_house, sample_house};
    use crate::{Document, Element, Transaction};

    #[test]
    fn a_plain_wall_measures_its_face_less_openings_and_its_volume() {
        let model = sample_house();
        let all = lines(&model);
        let (e, w) = model
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Wall(w) => Some((e, w)),
                _ => None,
            })
            .unwrap();
        let mine: Vec<&Line> = all.iter().filter(|l| l.element == e.id).collect();
        assert_eq!(mine.len(), 1);
        let q = mine[0].quantity;
        assert!(q.area <= w.length() * w.height + 1e-9);
        assert!((q.volume - q.area * w.thickness).abs() < 1e-9);
        assert!((q.length - w.length()).abs() < 1e-9);
        assert_eq!(q.count, 1);
        assert_eq!(mine[0].category, "Walls");
    }

    #[test]
    fn a_layered_wall_gives_a_line_per_layer_whose_volumes_add_up() {
        let model = layered_house();
        let all = lines(&model);
        let (e, asm) = model
            .iter()
            .find_map(|e| match &e.kind {
                ElementKind::Wall(_) => assembly_of(&model, e).map(|(_, a)| (e, a.clone())),
                _ => None,
            })
            .expect("the layered house has a layered wall");
        let mine: Vec<&Line> = all.iter().filter(|l| l.element == e.id).collect();
        let solid = asm
            .layers
            .iter()
            .filter(|l| !(l.function == LayerFunction::AirGap && l.material.is_none()))
            .count();
        assert_eq!(mine.len(), solid);
        let area = mine[0].quantity.area;
        for l in &mine {
            assert!(
                (l.quantity.area - area).abs() < 1e-9,
                "each layer covers the face"
            );
            assert!((l.quantity.volume - area * l.thickness).abs() < 1e-9);
            assert!(l.layer.is_some());
        }
    }

    #[test]
    fn rates_cost_out_totals_by_material_and_by_level() {
        let mut doc = Document::from_model(sample_house());
        let all = lines(doc.model());
        let walls = all.iter().find(|l| l.category == "Walls").unwrap();
        let brick = walls.material.expect("the sample walls have a material");
        // 45 for each square metre of it
        let mut m: Element = doc.model().get(brick).unwrap().clone();
        if let ElementKind::Material(mat) = &mut m.kind {
            mat.rate = Some(Rate {
                amount: 45.0,
                per: RateUnit::Area,
            });
        }
        doc.commit(Transaction::new("Rate").replace(doc.model().get(brick).unwrap().clone(), m))
            .unwrap();
        let model = doc.model();
        let all = lines(model);
        let totals = by_material(model, &all);
        let t = totals.iter().find(|t| t.key == Some(brick)).unwrap();
        let area: f64 = all
            .iter()
            .filter(|l| l.material == Some(brick))
            .map(|l| l.quantity.area)
            .sum();
        assert!((t.quantity.area - area).abs() < 1e-9);
        assert!((t.cost.unwrap() - 45.0 * area).abs() < 1e-6);
        // unpriced materials cost nothing
        assert!(totals
            .iter()
            .filter(|t| t.key != Some(brick))
            .all(|t| t.cost.is_none()));
        // the levels' costs add up to the whole
        let levels = by_level(model, &all);
        let sum: f64 = levels.iter().filter_map(|t| t.cost).sum();
        assert!((sum - total_cost(model, &all)).abs() < 1e-6);
        assert!(levels.len() >= 2);
    }

    #[test]
    fn a_rate_survives_saving_and_a_material_without_one_saves_as_before() {
        let r = Rate {
            amount: 12.5,
            per: RateUnit::Volume,
        };
        let text = serde_json::to_string(&r).unwrap();
        assert_eq!(serde_json::from_str::<Rate>(&text).unwrap(), r);
        let plain = serde_json::to_string(&crate::Material::default()).unwrap();
        assert!(!plain.contains("rate"));
    }
}
