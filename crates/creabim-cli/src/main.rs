//! `creabim` command-line tool: build the sample model and export it.

use creabim_core::{sample::sample_house, Model};
use creabim_geom::sheet::{sheet_svg, sheets, sheets_pdf, SheetInfo};
use creabim_ifc::{export_ifc4, ExportOptions};
use std::process::ExitCode;

const USAGE: &str = "\
usage:
  creabim sample <out.creabim.json>          write the sample house model
  creabim ifc <model.creabim.json> <out.ifc> export a model to IFC4
  creabim plan <model.creabim.json> <level-name> <out.svg>
                                             draw a level's plan at 1:100
  creabim view <model.creabim.json> <view-name> <out.svg>
                                             draw any view (plan, section, elevation)
  creabim views <model.creabim.json>         list the model's views
  creabim sheets <model.creabim.json>        list the model's sheets
  creabim sheet <model.creabim.json> <number> <out.svg>
                                             draw one sheet
  creabim pdf <model.creabim.json> <out.pdf> export every sheet to one PDF
  creabim stats <model.creabim.json>         count elements by IFC class";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["sample", out] => std::fs::write(out, sample_house().to_json()).map_err(|e| e.to_string()),
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
                creabim_core::ElementKind::View(v) => v.scale,
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

fn load(path: &str) -> Result<Model, String> {
    let s = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    Model::from_json(&s).map_err(|e| format!("{path}: {e}"))
}
