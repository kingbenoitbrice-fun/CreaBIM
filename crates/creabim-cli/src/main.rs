//! `creabim` command-line tool: build the sample model and export it.
//!
//! Every command that reads a project takes either a `.creabim` database or
//! a legacy `.creabim.json` file, chosen by extension.

use creabim_core::{sample::sample_house, Document, Model};
use creabim_geom::sheet::{sheet_svg, sheets, sheets_pdf, SheetInfo};
use creabim_ifc::{export_ifc4, export_ifcx, import_ifc, import_ifcx, ExportOptions, ImportResult};
use std::process::ExitCode;

const USAGE: &str = "\
usage: (<model> is a .creabim project or a legacy .creabim.json file)
  creabim sample <out.creabim|out.creabim.json>
                                       write the sample house model
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
            std::fs::write(out, sample_house().to_json()).map_err(|e| e.to_string())
        }
        ["sample", out] => creabim_store::save(out, &Document::from_model(sample_house()))
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
                    format!("no schedule of {category:?}; try doors, windows, walls or slabs")
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
    let mut counts = std::collections::BTreeMap::new();
    for e in r.model.iter() {
        *counts.entry(e.ifc_class().to_string()).or_insert(0) += 1;
    }
    let summary: Vec<String> = counts.iter().map(|(c, n)| format!("{n} {c}")).collect();
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
