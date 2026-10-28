//! How a plan view draws on paper: its pens' line weights, the tinted
//! footprints of what lies below the cut, and how far below its floor it
//! looks (its view depth).

use serde::{Deserialize, Serialize};

/// Pen widths on paper, in millimetres, from heavy to light.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LineWeights {
    /// Material cut by the plan plane: walls, columns, footings.
    pub cut: f64,
    /// Seen below the cut, or beyond it: slab edges, the stair, low walls.
    pub projection: f64,
    /// Symbols: door leaves and swings, window frames, the walking line.
    pub symbol: f64,
    /// Small things: furniture, fixtures, sanitary ware.
    pub fine: f64,
    /// Hatching inside cut fills.
    pub hatch: f64,
    /// Dashed lines for what is above the cut or hidden.
    pub hidden: f64,
}

impl LineWeights {
    /// The pens of a drawing at 1:`scale`: heavier at 1:50 and larger,
    /// where there is room for a clear hierarchy, lighter at 1:100 and
    /// smaller.
    pub fn for_scale(scale: f64) -> Self {
        if scale <= 75.0 {
            Self {
                cut: 0.70,
                projection: 0.35,
                symbol: 0.25,
                fine: 0.18,
                hatch: 0.13,
                hidden: 0.18,
            }
        } else {
            Self {
                cut: 0.50,
                projection: 0.25,
                symbol: 0.18,
                fine: 0.13,
                hatch: 0.09,
                hidden: 0.13,
            }
        }
    }

    /// Every weight, with its name, for editing.
    pub fn fields_mut(&mut self) -> [(&'static str, &mut f64); 6] {
        [
            ("Cut", &mut self.cut),
            ("Projection", &mut self.projection),
            ("Symbols", &mut self.symbol),
            ("Furniture and fixtures", &mut self.fine),
            ("Hatching", &mut self.hatch),
            ("Hidden (dashed)", &mut self.hidden),
        ]
    }
}

/// A plan's graphics. Every field has a default, so views saved before
/// it read as they drew then, with the weights tuned to their scale.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanGraphics {
    /// Pen weights of the user's own; none takes them from the scale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weights: Option<LineWeights>,
    /// Fill what lies wholly below the cut (furniture, fittings, imported
    /// meshes) with a light tint of its material's colour.
    #[serde(default = "yes")]
    pub below_fill: bool,
    /// How strong that tint is, 0 (none) to 1 (the material's colour).
    #[serde(default = "default_strength")]
    pub below_strength: f32,
    /// Fill it in light grey rather than in its material's colour.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub below_grey: bool,
    /// How far below its level's floor the plan looks, in metres: what
    /// lies between the floor and the cut is *below* it, drawn in
    /// projection; what lies under the floor, down to this depth and seen
    /// through a hole in it (a stair from the level below, in its well), is
    /// *beyond*. None looks down to the level below.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view_depth: Option<f64>,
}

fn yes() -> bool {
    true
}

fn default_strength() -> f32 {
    0.25
}

impl Default for PlanGraphics {
    fn default() -> Self {
        Self {
            weights: None,
            below_fill: true,
            below_strength: default_strength(),
            below_grey: false,
            view_depth: None,
        }
    }
}

impl PlanGraphics {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// The pens a view at 1:`scale` draws with.
    pub fn weights_at(&self, scale: f64) -> LineWeights {
        self.weights
            .unwrap_or_else(|| LineWeights::for_scale(scale))
    }

    /// The tint of a material colour (sRGB, 0..1) for what is below the
    /// cut, mixed toward white; none when the fill is off.
    pub fn below_tint(&self, rgb: [f32; 3]) -> Option<[f32; 3]> {
        if !self.below_fill || self.below_strength <= 0.0 {
            return None;
        }
        let rgb = if self.below_grey { [0.55; 3] } else { rgb };
        let k = self.below_strength.clamp(0.0, 1.0);
        Some(rgb.map(|c| 1.0 - (1.0 - c) * k))
    }

    /// How far below the floor of a level at `elevation` the plan looks,
    /// with `below` the elevation of the next level down, if any.
    pub fn depth(&self, elevation: f64, below: Option<f64>) -> f64 {
        self.view_depth
            .unwrap_or_else(|| below.map_or(3.0, |b| elevation - b))
            .max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weights_step_down_from_cut_to_hatch() {
        for s in [50.0, 100.0] {
            let w = LineWeights::for_scale(s);
            assert!(w.cut > w.projection && w.projection > w.symbol);
            assert!(w.symbol > w.fine && w.fine > w.hatch);
            assert!(w.hidden <= w.symbol);
        }
        assert!(LineWeights::for_scale(50.0).cut > LineWeights::for_scale(100.0).cut);
    }

    #[test]
    fn an_old_view_reads_with_the_default_graphics() {
        let g: PlanGraphics = serde_json::from_str("{}").unwrap();
        assert_eq!(g, PlanGraphics::default());
        assert!(g.below_fill);
        let tint = g.below_tint([0.0, 0.5, 1.0]).unwrap();
        assert!(tint.iter().all(|c| *c >= 0.74));
        // a plan saved before views had graphics
        let old = r#"{"kind":{"Plan":{"level":"00000000-0000-0000-0000-000000000001","cut_height":1.2}},"scale":50.0}"#;
        let v: crate::View = serde_json::from_str(old).unwrap();
        assert!(v.graphics.is_default());
        assert_eq!(v.graphics.weights_at(v.scale), LineWeights::for_scale(50.0));
        // the defaults write nothing; a change is kept
        assert!(!serde_json::to_string(&v).unwrap().contains("graphics"));
        let mut changed = v.clone();
        changed.graphics.view_depth = Some(1.0);
        changed.graphics.below_fill = false;
        let back: crate::View =
            serde_json::from_str(&serde_json::to_string(&changed).unwrap()).unwrap();
        assert_eq!(back.graphics, changed.graphics);
    }
}
