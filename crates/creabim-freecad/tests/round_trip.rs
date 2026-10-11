//! write() -> read() round trips, without FreeCAD (see
//! `tools/verify_in_freecad.py` for the check against a real FreeCAD).

use creabim_freecad::{read, write, FcExport, FcExportGroup, FcExportObject, FcMesh, IDENTITY};

/// Axis-aligned box from the origin, quads counter-clockwise from outside.
fn cuboid(x: f64, y: f64, z: f64) -> FcMesh {
    FcMesh {
        vertices: vec![
            [0.0, 0.0, 0.0],
            [x, 0.0, 0.0],
            [x, y, 0.0],
            [0.0, y, 0.0],
            [0.0, 0.0, z],
            [x, 0.0, z],
            [x, y, z],
            [0.0, y, z],
        ],
        faces: vec![
            vec![0, 3, 2, 1],
            vec![4, 5, 6, 7],
            vec![0, 1, 5, 4],
            vec![1, 2, 6, 5],
            vec![2, 3, 7, 6],
            vec![3, 0, 4, 7],
        ],
    }
}

/// L-shaped prism: concave polygons top and bottom.
fn l_prism(h: f64) -> FcMesh {
    let outline = [
        [0.0, 0.0],
        [3000.0, 0.0],
        [3000.0, 200.0],
        [200.0, 200.0],
        [200.0, 2000.0],
        [0.0, 2000.0],
    ];
    let n = outline.len() as u32;
    let mut vertices: Vec<[f64; 3]> = outline.iter().map(|p| [p[0], p[1], 0.0]).collect();
    vertices.extend(outline.iter().map(|p| [p[0], p[1], h]));
    let mut faces = vec![(0..n).rev().collect::<Vec<u32>>(), (n..2 * n).collect()];
    for i in 0..n {
        let j = (i + 1) % n;
        faces.push(vec![i, j, j + n, i + n]);
    }
    FcMesh { vertices, faces }
}

/// Rotation about Z by `deg` plus a translation, column-major.
fn placement(deg: f64, t: [f64; 3]) -> [[f64; 4]; 4] {
    let (s, c) = deg.to_radians().sin_cos();
    [
        [c, s, 0.0, 0.0],
        [-s, c, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [t[0], t[1], t[2], 1.0],
    ]
}

fn sample() -> FcExport {
    let mut inside_out = cuboid(100.0, 100.0, 100.0);
    for f in inside_out.faces.iter_mut() {
        f.reverse();
    }
    FcExport {
        label: "Round trip".into(),
        groups: vec![
            FcExportGroup {
                label: "Building".into(),
                parent: None,
            },
            FcExportGroup {
                label: "Ground floor".into(),
                parent: Some(0),
            },
        ],
        objects: vec![
            FcExportObject {
                label: "Wall A & <B>".into(),
                mesh: cuboid(4000.0, 200.0, 2800.0),
                placement: placement(30.0, [1000.0, 2000.0, 0.0]),
                color: Some([0.8, 0.4, 0.2, 1.0]),
                ifc_type: Some("Wall".into()),
                group: Some(1),
                visible: true,
                properties: vec![("Fire_rating".into(), "REI 60".into())],
            },
            FcExportObject {
                label: "Slab".into(),
                mesh: l_prism(250.0),
                placement: placement(0.0, [0.0, 0.0, 2800.0]),
                color: Some([0.5, 0.5, 0.5, 0.25]),
                ifc_type: Some("Slab".into()),
                group: Some(1),
                ..Default::default()
            },
            FcExportObject {
                label: "Inside-out cube".into(),
                mesh: inside_out,
                visible: false,
                ..Default::default()
            },
            FcExportObject {
                label: "Open sheet".into(),
                mesh: FcMesh {
                    vertices: vec![
                        [0.0, 0.0, 0.0],
                        [1000.0, 0.0, 0.0],
                        [1000.0, 1000.0, 0.0],
                        [0.0, 1000.0, 0.0],
                    ],
                    faces: vec![vec![0, 1, 2, 3]],
                },
                ..Default::default()
            },
        ],
    }
}

#[test]
fn write_then_read() {
    let src = sample();
    let bytes = write(&src).expect("write");
    let doc = read(&bytes).expect("read back");
    assert!(doc.warnings.is_empty(), "{:?}", doc.warnings);
    assert_eq!(doc.label, "Round trip");
    assert_eq!(doc.objects.len(), 6);

    let group = |label: &str| {
        doc.objects
            .iter()
            .position(|o| o.label == label)
            .unwrap_or_else(|| panic!("{label} missing"))
    };
    let building = group("Building");
    let ground = group("Ground floor");
    assert_eq!(doc.objects[building].type_id, "App::DocumentObjectGroup");
    assert_eq!(doc.objects[ground].parent, Some(building));

    for (o, e) in doc
        .objects
        .iter()
        .filter(|o| o.type_id == "Part::Feature")
        .zip(&src.objects)
    {
        assert_eq!(o.label, e.label);
        assert_eq!(o.visible, e.visible, "{}", o.label);
        assert_eq!(o.ifc_type, e.ifc_type);
        for (a, b) in o
            .placement
            .iter()
            .flatten()
            .zip(e.placement.iter().flatten())
        {
            assert!((a - b).abs() < 1e-9, "{} placement", o.label);
        }
        let read_local = o.local_mesh().unwrap();
        let expected_vol = e.mesh.volume().abs();
        let got = read_local.volume();
        assert!(
            (got - expected_vol).abs() <= 1e-6 * expected_vol.max(1.0),
            "{}: volume {got} vs {expected_vol}",
            o.label
        );
        let (lo, hi) = read_local.bounding_box().unwrap();
        let (elo, ehi) = e.mesh.bounding_box().unwrap();
        for k in 0..3 {
            assert!((lo[k] - elo[k]).abs() < 1e-6 && (hi[k] - ehi[k]).abs() < 1e-6);
        }
        match e.color {
            Some(c) => {
                let r = o.color.expect("colour");
                for k in 0..4 {
                    assert!(
                        (r[k] - c[k]).abs() <= 1.0 / 255.0,
                        "{} colour {r:?} vs {c:?}",
                        o.label
                    );
                }
            }
            None => assert!(o.color.is_none()),
        }
    }
    let wall = &doc.objects[group("Wall A & <B>")];
    assert_eq!(wall.parent, Some(ground));
    assert_eq!(wall.property("Fire_rating"), Some("REI 60"));
    assert!(wall.mesh.as_ref().unwrap().is_closed());
    // Global coordinates: the 30 degree rotation is applied.
    let (lo, hi) = wall.mesh.as_ref().unwrap().bounding_box().unwrap();
    let c = 30f64.to_radians().cos();
    let s = 30f64.to_radians().sin();
    assert!((hi[0] - (1000.0 + 4000.0 * c)).abs() < 1e-6);
    assert!((lo[0] - (1000.0 - 200.0 * s)).abs() < 1e-6);
    assert!((hi[1] - (2000.0 + 4000.0 * s + 200.0 * c)).abs() < 1e-6);
    // The open sheet stays a shell: its mesh is not closed.
    assert!(!doc.objects[group("Open sheet")]
        .mesh
        .as_ref()
        .unwrap()
        .is_closed());
    // The inside-out cube was flipped into a proper solid.
    assert!(
        doc.objects[group("Inside-out cube")]
            .mesh
            .as_ref()
            .unwrap()
            .volume()
            > 0.0
    );
}

#[test]
fn fixtures_survive_read_write_read() {
    for name in ["parts.FCStd", "arch.FCStd"] {
        let path = format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
        let first = read(&std::fs::read(path).unwrap()).unwrap();
        let second = read(&write(&first.to_export()).unwrap()).unwrap();
        let meshed: Vec<_> = first.objects.iter().filter(|o| o.mesh.is_some()).collect();
        let shapes: Vec<_> = second
            .objects
            .iter()
            .filter(|o| o.type_id == "Part::Feature")
            .collect();
        assert_eq!(meshed.len(), shapes.len(), "{name}");
        for (a, b) in meshed.iter().zip(&shapes) {
            assert_eq!(a.label, b.label);
            assert_eq!(a.ifc_type, b.ifc_type);
            assert_eq!(a.visible, b.visible);
            let (va, vb) = (
                a.mesh.as_ref().unwrap().volume(),
                b.mesh.as_ref().unwrap().volume(),
            );
            assert!(
                (va - vb).abs() <= 1e-6 * va.abs(),
                "{}: {va} vs {vb}",
                a.label
            );
            let (alo, ahi) = a.mesh.as_ref().unwrap().bounding_box().unwrap();
            let (blo, bhi) = b.mesh.as_ref().unwrap().bounding_box().unwrap();
            for k in 0..3 {
                assert!((alo[k] - blo[k]).abs() < 1e-6 && (ahi[k] - bhi[k]).abs() < 1e-6);
            }
        }
    }
}

#[test]
fn empty_and_degenerate_exports_do_not_fail() {
    let bytes = write(&FcExport::default()).unwrap();
    assert!(read(&bytes).unwrap().objects.is_empty());
    let doc = FcExport {
        objects: vec![FcExportObject {
            mesh: FcMesh {
                vertices: vec![[0.0; 3], [f64::NAN, 0.0, 0.0]],
                faces: vec![vec![0, 1, 7], vec![]],
            },
            placement: [
                [2.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0; 4],
            ],
            ..Default::default()
        }],
        ..Default::default()
    };
    let back = read(&write(&doc).unwrap()).unwrap();
    assert_eq!(back.objects.len(), 1);
    assert!(back.objects[0].mesh.is_none());
    let _ = IDENTITY;
}
