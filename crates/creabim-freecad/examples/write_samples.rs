//! Writes `.FCStd` files for checking in a real FreeCAD:
//!
//! ```text
//! cargo run -p creabim-freecad --example write_samples -- out/
//! freecadcmd crates/creabim-freecad/tools/verify_in_freecad.py out/
//! ```
//!
//! Besides a synthetic model it re-exports the committed fixtures
//! (read -> `to_export` -> write). `out/manifest.tsv` lists what FreeCAD
//! must find: file, label, volume (mm3, 0 for shells), placement origin,
//! colour.

use creabim_freecad::{read, write, FcExport, FcExportGroup, FcExportObject, FcMesh};
use std::fmt::Write as _;
use std::path::Path;

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

fn synthetic() -> FcExport {
    let (s, c) = 30f64.to_radians().sin_cos();
    let rotated = [
        [c, s, 0.0, 0.0],
        [-s, c, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [1000.0, 2000.0, 0.0, 1.0],
    ];
    let mut slab = creabim_freecad::IDENTITY;
    slab[3][2] = 2800.0;
    FcExport {
        label: "CreaBIM sample".into(),
        groups: vec![FcExportGroup {
            label: "Ground floor".into(),
            parent: None,
        }],
        objects: vec![
            FcExportObject {
                label: "Wall".into(),
                mesh: cuboid(4000.0, 200.0, 2800.0),
                placement: rotated,
                color: Some([0.8, 0.4, 0.2, 1.0]),
                ifc_type: Some("Wall".into()),
                group: Some(0),
                ..Default::default()
            },
            FcExportObject {
                label: "Slab".into(),
                mesh: cuboid(5000.0, 4000.0, 250.0),
                placement: slab,
                color: Some([0.6, 0.6, 0.6, 0.5]),
                ifc_type: Some("Slab".into()),
                group: Some(0),
                properties: vec![("Fire_rating".into(), "REI 60".into())],
                ..Default::default()
            },
            FcExportObject {
                label: "Sheet".into(),
                mesh: FcMesh {
                    vertices: vec![
                        [0.0, 0.0, 0.0],
                        [1000.0, 0.0, 0.0],
                        [1000.0, 1000.0, 0.0],
                        [0.0, 1000.0, 0.0],
                    ],
                    faces: vec![vec![0, 1, 2, 3]],
                },
                visible: false,
                ..Default::default()
            },
        ],
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "freecad-check".into());
    let out = Path::new(&out);
    std::fs::create_dir_all(out)?;
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures");
    let mut docs = vec![("sample.FCStd".to_string(), synthetic())];
    for name in ["parts", "arch"] {
        let doc = read(&std::fs::read(fixtures.join(format!("{name}.FCStd")))?)?;
        docs.push((format!("{name}_roundtrip.FCStd"), doc.to_export()));
    }
    let mut manifest = String::new();
    for (file, doc) in &docs {
        std::fs::write(out.join(file), write(doc)?)?;
        for o in &doc.objects {
            let vol = if o.mesh.is_closed() {
                o.mesh.volume().abs()
            } else {
                0.0
            };
            let p = o.placement[3];
            let col = o
                .color
                .map(|c| format!("{} {} {} {}", c[0], c[1], c[2], c[3]))
                .unwrap_or_default();
            let _ = writeln!(
                manifest,
                "{file}\t{}\t{vol}\t{} {} {}\t{col}\t{}",
                o.label.replace('\t', " "),
                p[0],
                p[1],
                p[2],
                o.ifc_type.clone().unwrap_or_default()
            );
        }
    }
    std::fs::write(out.join("manifest.tsv"), manifest)?;
    println!("wrote {} files to {}", docs.len(), out.display());
    Ok(())
}
