//! Ear-clipping triangulation, shared with the model core.

pub use creabim_core::polygon::{signed_area2, triangulate};

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec2;

    #[test]
    fn l_shape_triangulates_to_full_area() {
        let l = [
            DVec2::new(0.0, 0.0),
            DVec2::new(4.0, 0.0),
            DVec2::new(4.0, 2.0),
            DVec2::new(2.0, 2.0),
            DVec2::new(2.0, 4.0),
            DVec2::new(0.0, 4.0),
        ];
        let tris = triangulate(&l);
        assert_eq!(tris.len(), 4);
        let area: f64 = tris
            .iter()
            .map(|t| signed_area2(&[l[t[0]], l[t[1]], l[t[2]]]) / 2.0)
            .sum();
        assert!((area - 12.0).abs() < 1e-9);
    }

    #[test]
    fn clockwise_input_is_handled() {
        let sq = [
            DVec2::new(0.0, 0.0),
            DVec2::new(0.0, 1.0),
            DVec2::new(1.0, 1.0),
            DVec2::new(1.0, 0.0),
        ];
        let tris = triangulate(&sq);
        assert_eq!(tris.len(), 2);
        for t in tris {
            assert!(signed_area2(&[sq[t[0]], sq[t[1]], sq[t[2]]]) > 0.0);
        }
    }
}
