//! The sample house through a FreeCAD file and back.

use creabim_core::*;
use creabim_exchange::{freecad, sync};

#[test]
fn a_house_written_for_freecad_comes_back_as_the_same_elements() {
    let model = sample::sample_house();
    let bytes = freecad::export_fcstd(&model, "House").unwrap();
    let dir = std::env::temp_dir().join(format!("creabim-fc-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("house.FCStd");
    std::fs::write(&path, bytes).unwrap();

    let incoming = freecad::read_file(&path).unwrap();
    let walls = model
        .iter()
        .filter(|e| matches!(e.kind, ElementKind::Wall(_)))
        .count();
    assert!(incoming.len() >= walls);
    // every object knows the element it came from
    assert!(incoming
        .iter()
        .all(|i| i.element.is_some_and(|id| model.get(id).is_some())));
    // and nothing needs changing, since nothing changed
    let (_, report) = sync(&model, &path.display().to_string(), &incoming);
    assert_eq!((report.added, report.updated), (0, 0), "{report:?}");
    std::fs::remove_dir_all(dir).ok();
}
