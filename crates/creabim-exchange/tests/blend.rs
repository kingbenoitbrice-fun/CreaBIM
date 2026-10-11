//! A real Blender file, read into a model and reloaded.

use creabim_core::*;
use creabim_exchange::{blend, sync};
use std::path::Path;

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../creabim-blend/fixtures/scene_4_2_zstd.blend")
}

#[test]
fn a_blend_file_comes_in_with_its_wall_as_a_wall() {
    let mut incoming = blend::read_file(&fixture()).unwrap();
    // the fixture works in centimetres (unit scale 0.01): the wall block,
    // 4 units long, comes in 4 cm long
    let wall_in = incoming.iter().find(|i| i.name == "Wall Block").unwrap();
    let xs = wall_in.mesh.vertices.iter().map(|v| v.x);
    let span = xs.clone().fold(f64::MIN, f64::max) - xs.fold(f64::MAX, f64::min);
    assert!(span < 0.05, "{span}");
    // as if it were drawn in metres
    for i in &mut incoming {
        for v in &mut i.mesh.vertices {
            *v *= 100.0;
        }
    }
    let names: Vec<&str> = incoming.iter().map(|i| i.name.as_str()).collect();
    assert!(names.contains(&"Wall Block"), "{names:?}");
    // hidden objects and non-meshes stay out
    assert!(!names.contains(&"Marker") && !names.contains(&"Sun"));

    let mut doc = Document::from_model(sample::sample_house());
    let file = fixture().display().to_string();
    let (tx, report) = sync(doc.model(), &file, &incoming);
    assert_eq!(report.added, incoming.len(), "{report:?}");
    doc.commit(tx).unwrap();
    let wall = doc.model().iter().find(|e| e.name == "Wall Block").unwrap();
    let ElementKind::Wall(w) = &wall.kind else {
        panic!("{:?}", wall.kind)
    };
    assert!((w.length() - 4.0).abs() < 1e-6, "{}", w.length());
    assert!((w.thickness - 0.2).abs() < 1e-6);
    assert!((w.height - 3.0).abs() < 1e-6);

    // reading it again changes nothing
    let (_, again) = sync(doc.model(), &file, &incoming);
    assert_eq!(
        (again.added, again.updated, again.removed),
        (0, 0, 0),
        "{again:?}"
    );
}
