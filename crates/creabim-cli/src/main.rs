//! `creabim` command-line tool: build the sample model and export it.
//!
//! Every command that reads a project takes either a `.creabim` database or
//! a legacy `.creabim.json` file, chosen by extension.

use creabim_core::{sample::sample_house, starter::starter_house, Document, Model};
use creabim_geom::sheet::{sheet_svg, sheets, sheets_pdf, SheetInfo};
use creabim_ifc::{export_ifc4, export_ifcx, import_ifc, import_ifcx, ExportOptions, ImportResult};
use std::process::ExitCode;

const USAGE: &str = "\
usage: (<model> is a .creabim project or a legacy .creabim.json file)
  creabim sample <out.creabim|out.creabim.json>
                                       write the starter house, with its sheets
  creabim sample-simple <out.creabim.json>
                                       the simple box house the tests use
  creabim sample-layered <out.creabim.json>
                                       the sample house built of layered assemblies
  creabim sample-typed <out.creabim.json>
                                       the sample house built of element types,
                                       with a curtain wall, openings and a shaft
  creabim sample-furnished <out.creabim.json>
                                       the sample house furnished from the
                                       starter catalogue
  creabim catalogue [out-dir]          list the starter catalogue, or write
                                       each object as a catalogue file
  creabim catalogue-import <file> <dir>
                                       read a .blend, .FCStd, .gltf or .glb
                                       file as one catalogue object into dir
  creabim save-db <in.creabim.json> <out.creabim>
                                       convert a JSON model to a project file
  creabim ifc <model> <out.ifc>        export a model to IFC4
  creabim import-ifc <in.ifc> <out>    import an IFC2X3, IFC4 or IFC4X3 file
  creabim ifcx <model> <out.ifcx>      export to IFC5 IFCX JSON (experimental)
  creabim import-ifcx <in.ifcx> <out>  import an IFC5 IFCX file (experimental)
  creabim plan <model> <level-name> <out.svg>
                                       draw a level's plan at 1:100
  creabim view <model> <view-name> <out.svg>
                                       draw any view (plan, section, elevation)
  creabim views <model>                list the model's views
  creabim label-clashes <model>        list labels, tags and notes that still
                                       overlap in each plan
  creabim sheets <model>               list the model's sheets
  creabim sheet <model> <number> <out.svg>
                                       draw one sheet
  creabim pdf <model> <out.pdf>        export every sheet to one PDF
  creabim schedule <model> <doors|windows|walls|slabs> [out.csv]
                                       print or save a schedule as CSV
  creabim stats <model>                count elements by IFC class
  creabim glb <model> <out.glb>        export glTF for Blender, with element ids
  creabim fcstd <model> <out.FCStd>    export a FreeCAD document
  creabim link <model> <file.blend|file.FCStd> <out-model>
                                       bring in (or reload) a linked file's objects";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["sample", out] if is_json(out) => {
            std::fs::write(out, starter_house().to_json()).map_err(|e| e.to_string())
        }
        ["sample-simple", out] if is_json(out) => {
            std::fs::write(out, sample_house().to_json()).map_err(|e| e.to_string())
        }
        ["sample-layered", out] if is_json(out) => {
            std::fs::write(out, creabim_core::sample::layered_house().to_json())
                .map_err(|e| e.to_string())
        }
        ["sample-typed", out] if is_json(out) => {
            std::fs::write(out, creabim_core::sample::typed_house().to_json())
                .map_err(|e| e.to_string())
        }
        ["sample-furnished", out] if is_json(out) => {
            std::fs::write(out, creabim_core::sample::furnished_house().to_json())
                .map_err(|e| e.to_string())
        }
        ["catalogue"] => {
            for c in creabim_core::catalogue::starters() {
                let s = c.object.size();
                println!(
                    "{:10} {:28} {:.2} × {:.2} × {:.2} m  {} {}",
                    c.object.category.label(),
                    c.name,
                    s.x,
                    s.y,
                    s.z,
                    c.object.ifc.class,
                    c.object.ifc.predefined
                );
            }
            Ok(())
        }
        ["catalogue-import", file, dir] => {
            let path = std::path::Path::new(file);
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("object")
                .to_lowercase()
                .replace(|c: char| !c.is_ascii_alphanumeric(), "-");
            creabim_exchange::catalogue::read_file(path, &format!("user/{stem}"))
                .map_err(|e| e.to_string())
                .and_then(|c| {
                    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
                    let out = std::path::Path::new(dir).join(format!("{stem}.json"));
                    std::fs::write(&out, c.to_json()).map_err(|e| e.to_string())?;
                    let s = c.object.size();
                    println!(
                        "{}: {} parts, {:.2} × {:.2} × {:.2} m, {} {} → {}",
                        c.name,
                        c.object.parts.len(),
                        s.x,
                        s.y,
                        s.z,
                        c.object.ifc.class,
                        c.object.ifc.predefined,
                        out.display()
                    );
                    Ok(())
                })
        }
        ["catalogue", dir] => std::fs::create_dir_all(dir)
            .map_err(|e| e.to_string())
            .and_then(|_| {
                for c in creabim_core::catalogue::starters() {
                    let name = c.key.rsplit('/').next().unwrap_or("object");
                    let path = std::path::Path::new(dir).join(format!("{name}.json"));
                    std::fs::write(&path, c.to_json()).map_err(|e| e.to_string())?;
                }
                Ok(())
            }),
        ["sample", out] => creabim_store::save(out, &Document::from_model(starter_house()))
            .map_err(|e| format!("{out}: {e}")),
        ["save-db", input, out] => creabim_store::import_json(input)
            .map_err(|e| format!("{input}: {e}"))
            .and_then(|doc| creabim_store::save(out, &doc).map_err(|e| format!("{out}: {e}"))),
        ["glb", input, out] => load(input).and_then(|m| {
            std::fs::write(out, creabim_exchange::gltf::export_glb(&m)).map_err(|e| e.to_string())
        }),
        ["fcstd", input, out] => load(input).and_then(|m| {
            let label = std::path::Path::new(out)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            creabim_exchange::freecad::export_fcstd(&m, &label)
                .map_err(|e| e.to_string())
                .and_then(|b| std::fs::write(out, b).map_err(|e| e.to_string()))
        }),
        ["link", input, linked, out] => load(input).and_then(|m| {
            let file = std::fs::canonicalize(linked)
                .map_err(|e| format!("{linked}: {e}"))?
                .display()
                .to_string();
            let path = std::path::Path::new(&file);
            let lower = file.to_ascii_lowercase();
            let incoming = if lower.ends_with(".fcstd") {
                creabim_exchange::freecad::read_file(path)
            } else {
                creabim_exchange::blend::read_file(path)
            }
            .map_err(|e| e.to_string())?;
            let (tx, report) = creabim_exchange::sync(&m, &file, &incoming);
            let mut doc = Document::from_model(m);
            doc.commit(tx).map_err(|e| e.to_string())?;
            println!("{}", report.summary());
            for n in &report.notes {
                println!("  {n}");
            }
            if is_json(out) {
                std::fs::write(out, doc.model().to_json()).map_err(|e| e.to_string())
            } else {
                creabim_store::save(out, &doc).map_err(|e| format!("{out}: {e}"))
            }
        }),
        ["ifc", input, out] => load(input).and_then(|m| {
            let name = std::path::Path::new(out)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "CreaBIM Project".into());
            let opts = ExportOptions {
                project_name: name,
                ..Default::default()
            };
            std::fs::write(out, export_ifc4(&m, &opts)).map_err(|e| e.to_string())
        }),
        ["plan", input, level, out] => load(input).and_then(|m| {
            let (le, _) = m
                .levels()
                .find(|(e, _)| e.name == *level)
                .ok_or_else(|| format!("no level named {level:?}"))?;
            let svg = creabim_geom::plan_view(&m, le.id, 1.2).to_svg(100.0);
            std::fs::write(out, svg).map_err(|e| e.to_string())
        }),
        ["view", input, name, out] => load(input).and_then(|m| {
            let e = m
                .iter()
                .find(|e| e.name == *name && matches!(e.kind, creabim_core::ElementKind::View(_)))
                .ok_or_else(|| format!("no view named {name:?}"))?;
            let scale = match &e.kind {
                creabim_core::ElementKind::View(v) => v.paper_scale(),
                _ => 100.0,
            };
            let d = creabim_geom::section::view_drawing(&m, e.id).expect("is a view");
            std::fs::write(out, d.to_svg(scale)).map_err(|e| e.to_string())
        }),
        ["views", input] => load(input).map(|m| {
            for e in m.iter() {
                if let creabim_core::ElementKind::View(v) = &e.kind {
                    let kind = match v.kind {
                        creabim_core::ViewKind::Plan { .. } => "plan",
                        creabim_core::ViewKind::Section { cut: true, .. } => "section",
                        creabim_core::ViewKind::Section { cut: false, .. } => "elevation",
                        creabim_core::ViewKind::Axonometric { .. } => "3d",
                        creabim_core::ViewKind::Schedule { .. } => "schedule",
                    };
                    println!("{kind:10} 1:{:<5} {}", v.scale, e.name);
                }
            }
        }),
        ["label-clashes", input] => load(input).map(|m| {
            for e in m.iter() {
                let creabim_core::ElementKind::View(v) = &e.kind else {
                    continue;
                };
                if !matches!(v.kind, creabim_core::ViewKind::Plan { .. }) {
                    continue;
                }
                let Some(d) = creabim_geom::section::view_drawing(&m, e.id) else {
                    continue;
                };
                let mm = v.scale / 1000.0;
                let clashes = creabim_geom::declutter::overlapping_labels(&d.items, mm);
                println!("{}: {} overlapping", e.name, clashes.len());
                for (a, b) in clashes {
                    let text = |k: usize| creabim_geom::declutter::label_text(&d.items[k]);
                    println!("  {:?} and {:?}", text(a), text(b));
                }
            }
        }),
        ["sheets", input] => load(input).map(|m| {
            for (id, s) in sheets(&m) {
                let name = m.get(id).map(|e| e.name.as_str()).unwrap_or("");
                let size = s.size_name().unwrap_or("custom");
                println!("{:8} {size:6} {name}", s.number);
            }
        }),
        ["sheet", input, number, out] => load(input).and_then(|m| {
            let (id, _) = sheets(&m)
                .into_iter()
                .find(|(_, s)| s.number == *number)
                .ok_or_else(|| format!("no sheet numbered {number:?}"))?;
            let svg = sheet_svg(&m, id, &info(input)).expect("is a sheet");
            std::fs::write(out, svg).map_err(|e| e.to_string())
        }),
        ["pdf", input, out] => load(input).and_then(|m| {
            if sheets(&m).is_empty() {
                return Err("the model has no sheets".into());
            }
            std::fs::write(out, sheets_pdf(&m, &info(input))).map_err(|e| e.to_string())
        }),
        ["schedule", input, category, rest @ ..] if rest.len() <= 1 => load(input).and_then(|m| {
            let cat = creabim_core::ScheduleCategory::ALL
                .into_iter()
                .find(|c| c.label().eq_ignore_ascii_case(category))
                .ok_or_else(|| {
                    format!("no schedule of {category:?}; try doors, windows, walls, slabs, furniture or fixtures")
                })?;
            let csv = creabim_geom::schedule::to_csv(&creabim_geom::schedule::schedule(&m, cat));
            match rest {
                [out] => std::fs::write(out, csv).map_err(|e| e.to_string()),
                _ => {
                    print!("{csv}");
                    Ok(())
                }
            }
        }),
        ["import-ifc", input, out] => read_text(input)
            .and_then(|text| import_ifc(&text).map_err(|e| format!("{input}: {e}")))
            .and_then(|r| save_import(r, out)),
        ["ifcx", input, out] => load(input)
            .and_then(|m| std::fs::write(out, export_ifcx(&m)).map_err(|e| e.to_string())),
        ["import-ifcx", input, out] => read_text(input)
            .and_then(|text| import_ifcx(&text).map_err(|e| format!("{input}: {e}")))
            .and_then(|r| save_import(r, out)),
        ["stats", input] => load(input).map(|m| {
            let mut counts = std::collections::BTreeMap::new();
            for e in m.iter() {
                *counts.entry(e.ifc_class().to_string()).or_insert(0) += 1;
            }
            for (class, n) in counts {
                println!("{class:28} {n}");
            }
        }),
        _ => Err(USAGE.to_string()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn info(path: &str) -> SheetInfo {
    let name = std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    SheetInfo {
        project: name
            .split('.')
            .next()
            .unwrap_or("CreaBIM project")
            .to_string(),
        date: creabim_geom::sheet::today(),
    }
}

/// Read a text file; IFC files are ASCII by the standard, but some tools
/// write Latin-1 or UTF-8 into strings, so invalid bytes are replaced, not fatal.
fn read_text(path: &str) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// Save an imported model in the format `out` names, reporting what was skipped.
fn save_import(r: ImportResult, out: &str) -> Result<(), String> {
    for w in &r.warnings {
        eprintln!("warning: {w}");
    }
    if !r.details.is_empty() {
        eprintln!("details, element by element:");
        for d in &r.details {
            eprintln!("  {d}");
        }
    }
    // native elements and meshes apart: "13 IfcWall (6 meshes)"
    let mut counts = std::collections::BTreeMap::new();
    for e in r.model.iter() {
        let c: &mut (usize, usize) = counts.entry(e.ifc_class().to_string()).or_default();
        c.0 += 1;
        c.1 += usize::from(matches!(e.kind, creabim_core::ElementKind::FreeMesh(_)));
    }
    let summary: Vec<String> = counts
        .iter()
        .map(|(c, (n, meshes))| match meshes {
            0 => format!("{n} {c}"),
            m if m == n => format!("{n} {c} (meshes)"),
            1 => format!("{n} {c} (1 mesh)"),
            m => format!("{n} {c} ({m} meshes)"),
        })
        .collect();
    eprintln!("imported {} ({})", r.schema, summary.join(", "));
    if is_json(out) {
        std::fs::write(out, r.model.to_json()).map_err(|e| e.to_string())
    } else {
        creabim_store::save(out, &Document::from_model(r.model)).map_err(|e| format!("{out}: {e}"))
    }
}

fn is_json(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".json")
}

/// Read a project in either format; the history is not needed here.
fn load(path: &str) -> Result<Model, String> {
    creabim_store::load(path)
        .map(|doc| doc.into_parts().0)
        .map_err(|e| format!("{path}: {e}"))
}
