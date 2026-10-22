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

/// Files opened or saved lately, newest first, kept in the user's settings
/// folder between sessions.
#[derive(Debug, Default)]
pub struct Recent {
    pub files: Vec<String>,
    path: Option<std::path::PathBuf>,
}

const RECENT_MAX: usize = 10;

impl Recent {
    /// Read the list from `<settings>/CreaBIM/recent.json`, where settings
    /// is %APPDATA% on Windows and $XDG_CONFIG_HOME or ~/.config elsewhere.
    pub fn load() -> Self {
        let path = settings_dir().map(|d| d.join("recent.json"));
        let files = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str::<Vec<String>>(&t).ok())
            .unwrap_or_default();
        Self { files, path }
    }

    /// Put `file` at the top of the list and save it.
    pub fn add(&mut self, file: &str) {
        let full = std::fs::canonicalize(file)
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| file.to_string());
        self.files.retain(|f| *f != full);
        self.files.insert(0, full);
        self.files.truncate(RECENT_MAX);
        self.store();
    }

    pub fn clear(&mut self) {
        self.files.clear();
        self.store();
    }

    fn store(&self) {
        if let Some(p) = &self.path {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(t) = serde_json::to_string_pretty(&self.files) {
                let _ = std::fs::write(p, t);
            }
        }
    }
}

/// Interface choices kept between sessions in `<settings>/CreaBIM/settings.json`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    /// Width of the project browser and of the properties panel, in points.
    pub left_width: f32,
    pub right_width: f32,
    /// One view at a time instead of the 3D view and a drawing side by
    /// side, and which of the two it shows.
    pub single_view: bool,
    pub single_shows_3d: bool,
    /// The ground grid in the 3D view, and its spacing in metres.
    pub ground_grid: bool,
    pub ground_grid_spacing: f64,
    /// The kinds of element the 3D view leaves out (grids, annotation,
    /// guides…), from its Visibility settings.
    pub hidden_3d: Vec<creabim_core::Category>,
    /// The 3D view in X-ray: walls, slabs, roofs and masses see-through, so
    /// what is inside them shows.
    pub xray: bool,
    /// The 3D view shows what is hidden inside walls, slabs and roofs in its
    /// own colours, faintly through what hides it.
    pub hidden_colour_3d: bool,
    /// Guides hidden everywhere (they are kept, and come back when shown).
    pub guides_hidden: bool,
    /// Width of the Help panel, in points.
    pub help_width: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            left_width: 264.0,
            right_width: 290.0,
            single_view: false,
            single_shows_3d: true,
            ground_grid: true,
            ground_grid_spacing: 1.0,
            hidden_3d: vec![],
            xray: false,
            hidden_colour_3d: false,
            guides_hidden: false,
            help_width: 400.0,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        settings_dir()
            .and_then(|d| std::fs::read_to_string(d.join("settings.json")).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn store(&self) {
        // tests change settings without touching the user's own
        if cfg!(test) {
            return;
        }
        if let Some(dir) = settings_dir() {
            let _ = std::fs::create_dir_all(&dir);
            if let Ok(t) = serde_json::to_string_pretty(self) {
                let _ = std::fs::write(dir.join("settings.json"), t);
            }
        }
    }
}

fn settings_dir() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("APPDATA")
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME"))
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".config")))?;
    Some(base.join("CreaBIM"))
}

/// Ask for a file to open with the system's own file picker.
pub fn pick_open() -> Option<String> {
    rfd::FileDialog::new()
        .set_title("Open or link a file")
        .add_filter(
            "Everything CreaBIM reads",
            &[
                "creabim", "json", "ifc", "ifcx", "blend", "FCStd", "fcstd", "FCMat", "fcmat",
                "gltf", "glb",
            ],
        )
        .add_filter("CreaBIM project", &["creabim"])
        .add_filter("IFC", &["ifc", "ifcx"])
        .add_filter("Blender and FreeCAD (linked)", &["blend", "FCStd", "fcstd"])
        .add_filter(
            "Materials (FreeCAD cards, glTF)",
            &["FCMat", "fcmat", "gltf", "glb"],
        )
        .pick_file()
        .map(|p| p.display().to_string())
}

/// Ask where to save, starting from `current`.
pub fn pick_save(current: &str, title: &str, ext: &str) -> Option<String> {
    let p = Path::new(current);
    let mut d = rfd::FileDialog::new()
        .set_title(title)
        .add_filter(ext.to_uppercase(), &[ext]);
    if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
        d = d.set_file_name(name);
    }
    if let Some(dir) = p.parent().filter(|d| d.is_dir()) {
        d = d.set_directory(dir);
    }
    d.save_file().map(|p| p.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_from_before_the_layout_options_still_read() {
        let s: Settings = serde_json::from_str(r#"{"theme": "paper", "left_width": 300.0}"#)
            .expect("old settings read");
        assert_eq!(s.theme, "paper");
        assert!(!s.single_view);
        assert!(s.ground_grid);
        assert_eq!(s.ground_grid_spacing, 1.0);
        assert!(s.hidden_3d.is_empty() && !s.xray && !s.guides_hidden && !s.hidden_colour_3d);
    }

    #[test]
    fn the_3d_views_visibility_round_trips_through_the_settings() {
        let s = Settings {
            hidden_3d: vec![
                creabim_core::Category::Grids,
                creabim_core::Category::Guides,
            ],
            xray: true,
            hidden_colour_3d: true,
            guides_hidden: true,
            ground_grid_spacing: 1.2,
            ..Default::default()
        };
        let back: Settings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn recent_files_keep_the_newest_first_without_repeats() {
        let mut r = Recent::default();
        for f in ["a.creabim", "b.creabim", "a.creabim"] {
            r.add(f);
        }
        assert_eq!(r.files.len(), 2);
        assert!(r.files[0].ends_with("a.creabim"));
        for i in 0..20 {
            r.add(&format!("{i}.creabim"));
        }
        assert_eq!(r.files.len(), RECENT_MAX);
    }
}
