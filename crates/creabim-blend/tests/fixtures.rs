//! Reads every committed fixture (built by `fixtures/make_fixtures.py`
//! with real Blender builds) and checks the scene it describes.

use std::path::{Path, PathBuf};

use creabim_blend::{read, read_file, BlendError, BlendObject, BlendScene, ObjectKind};

const FIXTURES: &[(&str, &str)] = &[
    ("scene_2_82.blend", "2.82"),
    ("scene_2_82_gz.blend", "2.82"),
    ("scene_3_0_zstd.blend", "3.0"),
    ("scene_4_0.blend", "4.0"),
    ("scene_4_0_zstd.blend", "4.0"),
    ("scene_4_2_zstd.blend", "4.2"),
    ("scene_5_2.blend", "5.2"),
    ("scene_5_2_zstd.blend", "5.2"),
];

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join(name)
}

fn object<'a>(scene: &'a BlendScene, name: &str) -> &'a BlendObject {
    scene
        .objects
        .iter()
        .find(|o| o.name == name)
        .unwrap_or_else(|| panic!("no object {name}"))
}

/// World matrices as Blender computed them, written by the generator.
fn expected_worlds() -> Vec<(String, [[f64; 4]; 4])> {
    let text = std::fs::read_to_string(fixture("expected_worlds.txt")).unwrap();
    text.lines()
        .map(|line| {
            let (name, nums) = line.split_once('|').unwrap();
            let v: Vec<f64> = nums
                .split_whitespace()
                .map(|n| n.parse().unwrap())
                .collect();
            let mut m = [[0.0; 4]; 4];
            for (i, x) in v.iter().enumerate() {
                m[i / 4][i % 4] = *x;
            }
            (name.to_string(), m)
        })
        .collect()
}

fn sorted(v: &[String]) -> Vec<&str> {
    let mut v: Vec<&str> = v.iter().map(String::as_str).collect();
    v.sort();
    v
}

fn check(file: &str, version: &str) {
    let scene = read_file(&fixture(file)).unwrap_or_else(|e| panic!("{file}: {e}"));
    assert_eq!(scene.version, version, "{file}");
    assert!((scene.unit_scale - 0.01).abs() < 1e-6, "{file}");

    let mut names: Vec<&str> = scene.objects.iter().map(|o| o.name.as_str()).collect();
    names.sort();
    assert_eq!(
        names,
        [
            "Floor Plane",
            "Hex Prism",
            "Lamp Shade",
            "Marker",
            "Sun",
            "Wall Block"
        ],
        "{file}"
    );

    // Materials.
    assert_eq!(scene.materials.len(), 2, "{file}");
    let mat = |name: &str| {
        scene
            .materials
            .iter()
            .position(|m| m.name == name)
            .unwrap_or_else(|| panic!("{file}: no material {name}"))
    };
    let (concrete, wood) = (mat("Concrete"), mat("Wood"));
    let close = |a: [f32; 4], b: [f32; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
    assert!(close(
        scene.materials[concrete].color,
        [0.8, 0.8, 0.75, 1.0]
    ));
    assert!(close(scene.materials[wood].color, [0.6, 0.4, 0.2, 0.5]));
    for m in &scene.materials {
        // viewport display defaults of a new material
        assert!((m.roughness - 0.4).abs() < 1e-6, "{file}");
        assert_eq!(m.metallic, 0.0, "{file}");
        // the generator doesn't turn nodes on; from Blender 5 every
        // material has them, with the Principled BSDF's defaults
        if scene.version.starts_with('5') {
            let p = m
                .principled
                .unwrap_or_else(|| panic!("{file}: no Principled BSDF"));
            assert!(close(p.base_color, [0.8, 0.8, 0.8, 1.0]), "{file}");
            assert!((p.roughness - 0.5).abs() < 1e-6, "{file}");
            assert_eq!((p.metallic, p.alpha), (0.0, 1.0), "{file}");
        } else {
            assert_eq!(m.principled, None, "{file}");
        }
    }

    // World matrices, against Blender's own.
    for (name, want) in expected_worlds() {
        let got = object(&scene, &name).world;
        for c in 0..4 {
            for r in 0..4 {
                assert!(
                    (got[c][r] - want[c][r]).abs() < 1e-5,
                    "{file}: {name} world[{c}][{r}] = {} not {}",
                    got[c][r],
                    want[c][r]
                );
            }
        }
    }

    // The wall: a unit cube scaled 4 x 0.2 x 3 by its object transform.
    let wall = object(&scene, "Wall Block");
    assert_eq!(wall.kind, ObjectKind::Mesh);
    assert_eq!(wall.data_name.as_deref(), Some("Wall Block Mesh"));
    assert_eq!(sorted(&wall.collections), ["Structure"], "{file}");
    assert_eq!(wall.material_slots, [Some(concrete)], "{file}");
    assert!(!wall.hidden);
    assert_eq!(wall.parent, None);
    let props = &wall.custom_props;
    assert!(
        props.contains(&("IfcClass".into(), "IfcWall".into())),
        "{file}: {props:?}"
    );
    assert!(
        props.contains(&("FireRating".into(), "60".into())),
        "{file}: {props:?}"
    );
    let mesh = wall.mesh.as_ref().unwrap();
    assert_eq!((mesh.vertices.len(), mesh.faces.len()), (8, 6), "{file}");
    assert!(mesh.faces.iter().all(|f| f.len() == 4));
    assert!(mesh
        .vertices
        .iter()
        .all(|v| v.iter().all(|c| (c.abs() - 0.5).abs() < 1e-6)));
    // Polygons keep Blender's winding: the first face is the bottom.
    assert_eq!(mesh.faces[0], [0, 3, 2, 1], "{file}");

    let plane = object(&scene, "Floor Plane");
    let mesh = plane.mesh.as_ref().unwrap();
    assert_eq!((mesh.vertices.len(), mesh.faces.len()), (4, 1), "{file}");
    assert_eq!(mesh.vertices[2], [5.0, 5.0, 0.0]);
    assert_eq!(plane.material_slots, [Some(wood)], "{file}");

    // The prism: an ngon cap on each end, the top one in slot 1.
    let prism = object(&scene, "Hex Prism");
    let mesh = prism.mesh.as_ref().unwrap();
    assert_eq!((mesh.vertices.len(), mesh.faces.len()), (12, 8), "{file}");
    assert_eq!(mesh.faces[0], [5, 4, 3, 2, 1, 0], "{file}");
    assert_eq!(mesh.faces[1], [6, 7, 8, 9, 10, 11], "{file}");
    assert_eq!(mesh.face_material, [0, 1, 0, 0, 0, 0, 0, 0], "{file}");
    assert_eq!(prism.material_slots, [Some(concrete), Some(wood)], "{file}");
    assert_eq!(sorted(&prism.collections), ["Furniture"], "{file}");

    // Parenting and membership of two collections.
    let prism_idx = scene.objects.iter().position(|o| o.name == "Hex Prism");
    let shade = object(&scene, "Lamp Shade");
    assert_eq!(shade.parent, prism_idx, "{file}");
    assert_eq!(
        sorted(&shade.collections),
        ["Furniture", "Structure"],
        "{file}"
    );
    assert_eq!(shade.mesh.as_ref().unwrap().vertices.len(), 8);

    // Non-mesh objects are listed with their kind, and hidden flags read.
    let marker = object(&scene, "Marker");
    assert_eq!(marker.kind, ObjectKind::Empty);
    assert!(marker.mesh.is_none() && marker.data_name.is_none());
    assert!(marker.hidden, "{file}: hide_viewport");
    let sun = object(&scene, "Sun");
    assert_eq!(sun.kind, ObjectKind::Light);
    assert_eq!(sun.data_name.as_deref(), Some("Sun"));
    assert!(sun.collections.is_empty(), "{file}: only in the scene root");
    assert!(sun.hidden, "{file}: hidden in view layer");
}

#[test]
fn all_fixtures() {
    for (file, version) in FIXTURES {
        check(file, version);
    }
}

#[test]
fn compressed_matches_plain() {
    let plain = read_file(&fixture("scene_4_0.blend")).unwrap();
    let zstd = read_file(&fixture("scene_4_0_zstd.blend")).unwrap();
    assert_eq!(plain, zstd);
    let plain = read_file(&fixture("scene_2_82.blend")).unwrap();
    let gz = read_file(&fixture("scene_2_82_gz.blend")).unwrap();
    assert_eq!(plain, gz);
}

#[test]
fn rejects_non_blend_input() {
    assert!(matches!(read(b""), Err(BlendError::NotBlend)));
    assert!(matches!(read(b"PK\x03\x04 zip"), Err(BlendError::NotBlend)));
    assert!(matches!(read(b"BLENDER-v4"), Err(BlendError::NotBlend)));
    assert!(read(&[0x1f, 0x8b, 0, 0]).is_err());
    assert!(read(&[0x28, 0xb5, 0x2f, 0xfd, 1, 2, 3]).is_err());
}

/// Truncated and corrupted files must give errors or partial scenes,
/// never panics or runaway allocations.
#[test]
fn survives_damaged_files() {
    for file in [
        "scene_2_82.blend",
        "scene_2_82_gz.blend",
        "scene_4_0.blend",
        "scene_4_0_zstd.blend",
        "scene_5_2.blend",
    ] {
        let bytes = std::fs::read(fixture(file)).unwrap();
        for i in 0..64 {
            let _ = read(&bytes[..bytes.len() * i / 64]);
        }
        // A small LCG keeps the corruption deterministic.
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        for _ in 0..150 {
            let mut damaged = bytes.clone();
            for _ in 0..16 {
                seed = seed
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let at = (seed >> 33) as usize % damaged.len();
                damaged[at] = (seed >> 24) as u8;
            }
            let _ = read(&damaged);
        }
    }
}
