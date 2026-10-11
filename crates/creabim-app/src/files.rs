//! Opening, saving, importing and exporting, and watching linked files so
//! a model saved in Blender shows up here without asking.

use creabim_core::Model;
use creabim_exchange::Incoming;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime};

/// The kinds of file that come in as linked objects.
pub const LINKABLE: &[&str] = &["blend", "fcstd"];

fn ext(path: &str) -> String {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

/// Is this a project file (opened in place of the current project)?
pub fn is_project(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".creabim") || ext(path) == "json"
}

pub fn is_linkable(path: &str) -> bool {
    LINKABLE.contains(&ext(path).as_str())
}

/// The objects in a linked file.
pub fn read_incoming(path: &str) -> Result<Vec<Incoming>, String> {
    match ext(path).as_str() {
        "blend" => creabim_exchange::blend::read_file(Path::new(path)).map_err(|e| e.to_string()),
        "fcstd" => creabim_exchange::freecad::read_file(Path::new(path)).map_err(|e| e.to_string()),
        other => Err(format!("CreaBIM can't read .{other} files yet.")),
    }
}

/// The linked files the model mirrors.
pub fn linked_files(model: &Model) -> Vec<String> {
    let mut files: Vec<String> = model
        .iter()
        .filter_map(|e| e.source.as_ref().map(|s| s.file.clone()))
        .filter(|f| is_linkable(f))
        .collect();
    files.sort();
    files.dedup();
    files
}

/// Watches linked files for saves.
#[derive(Default)]
pub struct Watch {
    seen: HashMap<String, Option<SystemTime>>,
    last: Option<Instant>,
}

impl Watch {
    /// Remember a file as it is now, e.g. right after reading it.
    pub fn mark(&mut self, file: &str) {
        self.seen.insert(file.to_string(), modified(file));
    }

    /// Files saved since they were last seen. Looks at most once a second.
    pub fn changed(&mut self, model: &Model) -> Vec<String> {
        if self
            .last
            .is_some_and(|t| t.elapsed() < Duration::from_secs(1))
        {
            return vec![];
        }
        self.last = Some(Instant::now());
        let mut out = Vec::new();
        for f in linked_files(model) {
            let now = modified(&f);
            match self.seen.get(&f) {
                None => {
                    self.seen.insert(f, now);
                }
                // a save in progress is left to finish before reading
                Some(before)
                    if *before != now
                        && now.is_some_and(|t| {
                            t.elapsed().unwrap_or_default() > Duration::from_millis(300)
                        }) =>
                {
                    self.seen.insert(f.clone(), now);
                    out.push(f);
                }
                _ => {}
            }
        }
        out
    }
}

fn modified(file: &str) -> Option<SystemTime> {
    std::fs::metadata(file).and_then(|m| m.modified()).ok()
}
