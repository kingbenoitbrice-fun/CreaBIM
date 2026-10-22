//! Furniture and fixtures in the app: the Catalogue window (the starter
//! objects and the user's own by category, with pictures; click one to
//! place it; import one from Blender, FreeCAD or glTF; save the selection
//! as one; the project's own object types), the user's catalogue kept as
//! one JSON file per object in `<settings>/CreaBIM/catalogue`, and the
//! Properties of objects and of their types.

use super::*;
use creabim_core::objects::{
    self, CatalogueObject, IfcKind, Mount, ObjectCategory, ObjectType, IFC_CLASSES,
};
use std::path::{Path, PathBuf};

/// Side of a catalogue picture, in points.
const THUMB: usize = 76;

/// The Catalogue window's state, for the session.
#[derive(Default)]
pub(super) struct CatalogueUi {
    pub open: bool,
    /// The category shown; none shows them all.
    category: Option<ObjectCategory>,
    filter: String,
    /// The user's own objects, read from their folder when first needed.
    user: Option<Vec<CatalogueObject>>,
    /// Pictures by catalogue key (and, for the project's types, revision).
    thumbs: HashMap<String, egui::TextureHandle>,
    /// The name the selection is saved under.
    save_name: String,
}

/// What a click in the window asks for.
enum CatAsk {
    Place(Box<CatalogueObject>, Option<ElementId>),
    Import,
    SaveSelection,
    Delete(String),
    Reveal,
}

/// The folder of the user's own catalogue.
pub(super) fn user_dir() -> Option<PathBuf> {
    crate::files::settings_dir().map(|d| d.join("catalogue"))
}

/// Every catalogue object in `dir`, by name.
pub(super) fn load_dir(dir: &Path) -> Vec<CatalogueObject> {
    let mut out: Vec<CatalogueObject> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            let text = std::fs::read_to_string(e.path()).ok()?;
            let mut c = CatalogueObject::from_json(&text).ok()?;
            // the file is the object: its name is its key
            let stem = e.path().file_stem()?.to_str()?.to_string();
            c.key = format!("user/{stem}");
            c.object.catalogue = Some(c.key.clone());
            Some(c)
        })
        .collect();
    out.sort_by_key(|c| c.name.to_lowercase());
    out
}

/// A file name for an object called `name` in `dir`, not yet taken.
pub(super) fn free_key(dir: &Path, name: &str) -> String {
    let mut slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while slug.contains("--") {
        slug = slug.replace("--", "-");
    }
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "object" } else { slug };
    let mut n = 1;
    loop {
        let stem = if n == 1 {
            slug.to_string()
        } else {
            format!("{slug}-{n}")
        };
        if !dir.join(format!("{stem}.json")).exists() {
            return format!("user/{stem}");
        }
        n += 1;
    }
}

/// Write `c` into `dir` under its key.
pub(super) fn save_to(dir: &Path, c: &CatalogueObject) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let stem = c.key.strip_prefix("user/").unwrap_or(&c.key);
    let path = dir.join(format!("{stem}.json"));
    std::fs::write(&path, c.to_json())?;
    Ok(path)
}

/// Remove the object keyed `key` from `dir`.
pub(super) fn delete_from(dir: &Path, key: &str) -> std::io::Result<()> {
    let stem = key.strip_prefix("user/").unwrap_or(key);
    std::fs::remove_file(dir.join(format!("{stem}.json")))
}

/// A picture of `c`: its parts in their colours, seen from the front,
/// a little to the right and above, drawn in software.
pub(super) fn thumbnail(c: &CatalogueObject, size: usize) -> egui::ColorImage {
    let ss = 2;
    let n = size * ss;
    let d = DVec3::new(-0.55, 1.0, -0.75).normalize();
    let r = d.cross(DVec3::Z).normalize();
    let u = r.cross(d);
    let light = DVec3::new(0.35, -0.65, 0.85).normalize();
    let tris: Vec<([DVec3; 3], [f32; 3])> = c
        .object
        .parts
        .iter()
        .enumerate()
        .flat_map(|(i, p)| {
            let color = c
                .materials
                .get(i)
                .and_then(|m| m.as_ref())
                .map_or([0.8, 0.78, 0.74], |m| m.1.color);
            let v = &p.mesh.vertices;
            p.mesh.faces.iter().flat_map(move |f| {
                (1..f.len().saturating_sub(1)).filter_map(move |k| {
                    let t = [
                        *v.get(f[0] as usize)?,
                        *v.get(f[k] as usize)?,
                        *v.get(f[k + 1] as usize)?,
                    ];
                    Some((t, color))
                })
            })
        })
        .collect();
    let mut img = vec![[0u8; 4]; n * n];
    if tris.is_empty() {
        return egui::ColorImage::new([size, size], vec![egui::Color32::TRANSPARENT; size * size]);
    }
    let proj = |p: DVec3| DVec3::new(p.dot(r), p.dot(u), p.dot(d));
    let (mut lo, mut hi) = (DVec3::splat(f64::MAX), DVec3::splat(f64::MIN));
    for (t, _) in &tris {
        for p in t {
            let q = proj(*p);
            lo = lo.min(q);
            hi = hi.max(q);
        }
    }
    let span = (hi.x - lo.x).max(hi.y - lo.y).max(1e-6);
    let scale = (n as f64 * 0.9) / span;
    let mid = (lo + hi) / 2.0;
    let to_px = |q: DVec3| {
        DVec3::new(
            (q.x - mid.x) * scale + n as f64 / 2.0,
            n as f64 / 2.0 - (q.y - mid.y) * scale,
            q.z,
        )
    };
    let mut depth = vec![f64::MAX; n * n];
    for (t, color) in &tris {
        let mut normal = (t[1] - t[0]).cross(t[2] - t[0]);
        if normal.length_squared() < 1e-18 {
            continue;
        }
        normal = normal.normalize();
        if normal.dot(d) > 0.0 {
            normal = -normal;
        }
        let shade = 0.5 + 0.5 * normal.dot(light).max(0.0) as f32;
        let rgb = color.map(|c| ((c * shade).clamp(0.0, 1.0) * 255.0) as u8);
        let s = t.map(|p| to_px(proj(p)));
        let x0 = s
            .iter()
            .map(|p| p.x)
            .fold(f64::MAX, f64::min)
            .floor()
            .max(0.0) as usize;
        let x1 = (s.iter().map(|p| p.x).fold(f64::MIN, f64::max).ceil() as usize).min(n - 1);
        let y0 = s
            .iter()
            .map(|p| p.y)
            .fold(f64::MAX, f64::min)
            .floor()
            .max(0.0) as usize;
        let y1 = (s.iter().map(|p| p.y).fold(f64::MIN, f64::max).ceil() as usize).min(n - 1);
        let area = (s[1].x - s[0].x) * (s[2].y - s[0].y) - (s[2].x - s[0].x) * (s[1].y - s[0].y);
        if area.abs() < 1e-12 || x0 > x1 || y0 > y1 {
            continue;
        }
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                let w0 = ((s[1].x - px) * (s[2].y - py) - (s[2].x - px) * (s[1].y - py)) / area;
                let w1 = ((s[2].x - px) * (s[0].y - py) - (s[0].x - px) * (s[2].y - py)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * s[0].z + w1 * s[1].z + w2 * s[2].z;
                let i = y * n + x;
                if z < depth[i] {
                    depth[i] = z;
                    img[i] = [rgb[0], rgb[1], rgb[2], 255];
                }
            }
        }
    }
    // down to size, averaging each block
    let mut px = Vec::with_capacity(size * size);
    for y in 0..size {
        for x in 0..size {
            let mut acc = [0u32; 4];
            for dy in 0..ss {
                for dx in 0..ss {
                    let p = img[(y * ss + dy) * n + x * ss + dx];
                    let a = p[3] as u32;
                    for k in 0..3 {
                        acc[k] += p[k] as u32 * a / 255;
                    }
                    acc[3] += a;
                }
            }
            let k = (ss * ss) as u32;
            px.push(egui::Color32::from_rgba_premultiplied(
                (acc[0] / k) as u8,
                (acc[1] / k) as u8,
                (acc[2] / k) as u8,
                (acc[3] / k) as u8,
            ));
        }
    }
    egui::ColorImage::new([size, size], px)
}

/// The selection as a catalogue object called `name`: an object's type,
/// free meshes as they are, or any other elements as their 3D shapes.
pub(super) fn from_selection(
    model: &Model,
    ids: &[ElementId],
    name: &str,
    key: &str,
) -> Option<CatalogueObject> {
    if let [id] = ids {
        if let Some(e) = model.get(*id) {
            if let ElementKind::Object(_) = &e.kind {
                let (t, _) = creabim_core::types::type_of(model, e)?;
                let mut c = objects::from_type(model, t.id, key)?;
                c.name = name.to_string();
                return Some(c);
            }
            if let ElementKind::ElementType(_) = &e.kind {
                let mut c = objects::from_type(model, *id, key)?;
                c.name = name.to_string();
                return Some(c);
            }
        }
    }
    let meshes: Vec<ElementId> = ids
        .iter()
        .copied()
        .filter(|id| {
            model
                .get(*id)
                .is_some_and(|e| matches!(e.kind, ElementKind::FreeMesh(_)))
        })
        .collect();
    if meshes.len() == ids.len() {
        return objects::from_meshes(model, &meshes, name, key);
    }
    // anything else: its shape in 3D, each piece in its colour
    let items: Vec<_> = creabim_geom::scene::build_scene(model)
        .into_iter()
        .filter(|i| ids.contains(&i.id) && !i.ghost)
        .collect();
    let mut parts = vec![];
    let mut materials = vec![];
    for item in items {
        let mesh = creabim_core::EditMesh {
            vertices: item
                .mesh
                .positions
                .iter()
                .map(|p| DVec3::new(p[0] as f64, p[1] as f64, p[2] as f64))
                .collect(),
            faces: item.mesh.indices.chunks(3).map(|t| t.to_vec()).collect(),
        };
        let [r, g, b] = item.color.map(|c| (c * 255.0) as u8);
        let look = item
            .finish
            .map(str::to_string)
            .unwrap_or_else(|| format!("Colour #{r:02x}{g:02x}{b:02x}"));
        let mut m = creabim_core::Material::new(creabim_core::Hatch::Empty, item.color);
        m.roughness = item.surface[0];
        m.metallic = item.surface[1];
        m.opacity = item.surface[2];
        parts.push(objects::ObjectPart {
            name: model.get(item.id).map_or(String::new(), |e| e.name.clone()),
            mesh,
            material: None,
        });
        materials.push(Some((look, m)));
    }
    if parts.is_empty() {
        return None;
    }
    objects::recentre(&mut parts);
    let (category, ifc, mount) = creabim_exchange::catalogue::guess(name);
    Some(CatalogueObject {
        key: key.to_string(),
        name: name.to_string(),
        description: String::new(),
        object: ObjectType {
            category,
            ifc,
            mount,
            parts,
            symbol: vec![],
            catalogue: Some(key.to_string()),
        },
        materials,
    })
}

fn mount_label(m: Mount) -> &'static str {
    match m {
        Mount::Wall => "Against a wall",
        Mount::Floor => "Free-standing",
    }
}

/// A part's material: one of the project's, or none.
fn part_material(
    ui: &mut egui::Ui,
    model: &Model,
    salt: usize,
    current: &mut Option<ElementId>,
) -> bool {
    let shown = current
        .and_then(|c| model.get(c))
        .map_or("None".to_string(), |e| e.name.clone());
    let mut changed = false;
    egui::ComboBox::from_id_salt(("part_material", salt))
        .selected_text(shown)
        .width(170.0)
        .show_ui(ui, |ui| {
            changed |= ui.selectable_value(current, None, "None").changed();
            for e in model
                .iter()
                .filter(|e| matches!(e.kind, ElementKind::Material(_)))
            {
                changed |= ui.selectable_value(current, Some(e.id), &e.name).changed();
            }
        });
    changed
}

/// An object type's fields. Returns the undo label of what changed, or "".
pub(super) fn type_fields(ui: &mut egui::Ui, model: &Model, o: &mut ObjectType) -> &'static str {
    let mut label = "";
    let s = o.size();
    ui.label(format!(
        "{} wide, {} deep, {} high",
        crate::units::len(s.x),
        crate::units::len(s.y),
        crate::units::len(s.z)
    ));
    ui.horizontal(|ui| {
        ui.label("Category");
        egui::ComboBox::from_id_salt("object_category")
            .selected_text(o.category.label())
            .show_ui(ui, |ui| {
                for c in ObjectCategory::ALL {
                    if ui.selectable_value(&mut o.category, c, c.label()).changed() {
                        label = "Object category";
                    }
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label("Placed");
        egui::ComboBox::from_id_salt("object_mount")
            .selected_text(mount_label(o.mount))
            .show_ui(ui, |ui| {
                for m in [Mount::Wall, Mount::Floor] {
                    if ui
                        .selectable_value(&mut o.mount, m, mount_label(m))
                        .changed()
                    {
                        label = "Object placement";
                    }
                }
            })
            .response
            .on_hover_text("Against a wall: the Object tool backs it onto the nearest wall face");
    });
    ui.horizontal(|ui| {
        ui.label("IFC");
        let mut class = o.ifc.class.clone();
        egui::ComboBox::from_id_salt("object_class")
            .selected_text(&class)
            .show_ui(ui, |ui| {
                for (c, _, _) in IFC_CLASSES {
                    ui.selectable_value(&mut class, c.to_string(), *c);
                }
            });
        if class != o.ifc.class {
            o.ifc = IfcKind::new(&class, "NOTDEFINED").normalized();
            label = "Object IFC class";
        }
        let mut pre = o.ifc.predefined.clone();
        egui::ComboBox::from_id_salt("object_predefined")
            .selected_text(&pre)
            .show_ui(ui, |ui| {
                for p in objects::predefined_types(&o.ifc.class) {
                    ui.selectable_value(&mut pre, p.to_string(), *p);
                }
            });
        if pre != o.ifc.predefined {
            o.ifc.predefined = pre;
            label = "Object IFC type";
        }
    });
    if let Some(k) = &o.catalogue {
        ui.label(egui::RichText::new(format!("From the catalogue: {k}")).weak());
    }
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Parts").strong());
    egui::Grid::new("object_parts")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            for (i, p) in o.parts.iter_mut().enumerate() {
                ui.label(if p.name.is_empty() {
                    format!("Part {}", i + 1)
                } else {
                    p.name.clone()
                });
                if part_material(ui, model, i, &mut p.material) {
                    label = "Part material";
                }
                ui.end_row();
            }
        });
    label
}

/// A placed object's own fields. Returns the undo label, or "".
pub(super) fn object_fields(ui: &mut egui::Ui, o: &mut objects::Object) -> &'static str {
    let mut label = "";
    ui.horizontal(|ui| {
        ui.label("Above its level");
        if ui
            .add(crate::units::drag(&mut o.offset).range(-10.0..=100.0))
            .changed()
        {
            label = "Object height";
        }
    });
    ui.horizontal(|ui| {
        ui.label("Turn");
        if ui
            .add(
                egui::DragValue::new(&mut o.rotation)
                    .speed(1.0)
                    .suffix("°")
                    .range(-360.0..=720.0),
            )
            .changed()
        {
            o.rotation = o.rotation.rem_euclid(360.0);
            label = "Turn object";
        }
        if ui
            .small_button("Turn 90°")
            .on_hover_text("A quarter turn anticlockwise about its back edge")
            .clicked()
        {
            o.rotation = (o.rotation + 90.0).rem_euclid(360.0);
            label = "Turn object";
        }
    });
    label
}

impl CreaBimApp {
    /// The user's own catalogue, read when first wanted.
    fn user_catalogue(&mut self) -> &[CatalogueObject] {
        if self.catalogue.user.is_none() {
            self.catalogue.user = Some(user_dir().map(|d| load_dir(&d)).unwrap_or_default());
        }
        self.catalogue.user.as_deref().unwrap_or(&[])
    }

    /// Run a catalogue action. False when the action isn't one.
    pub(super) fn objects_action(&mut self, head: &str, _arg: &str) -> bool {
        match head {
            "catalogue" => self.catalogue.open = true,
            "catalogue.import" => self.run_catalogue(CatAsk::Import),
            "catalogue.save" => self.run_catalogue(CatAsk::SaveSelection),
            _ => return false,
        }
        true
    }

    /// Properties of an object's type, under the object's own.
    pub(super) fn object_type_section(&mut self, ui: &mut egui::Ui, id: ElementId) {
        let model = self.doc.model();
        let Some((t, _)) = model
            .get(id)
            .and_then(|e| creabim_core::types::type_of(model, e))
        else {
            return;
        };
        let ElementKind::ElementType(ty) = &t.kind else {
            return;
        };
        let (tid, tname) = (t.id, t.name.clone());
        let mut ty = ty.clone();
        let creabim_core::TypeParams::Object(o) = &mut ty.params else {
            return;
        };
        ui.separator();
        ui.label(egui::RichText::new(format!("Its type, {tname}")).strong());
        let label = type_fields(ui, model, o);
        if ui
            .small_button("Save to catalogue")
            .on_hover_text("Keep this object in your own catalogue, for every project")
            .clicked()
        {
            self.catalogue.save_name = tname.clone();
            self.run_catalogue(CatAsk::SaveSelection);
        }
        if !label.is_empty() {
            self.run_types(typesui::TypeAsk::Edit(tid, label, ty));
        }
    }

    /// The Catalogue window, when open.
    pub(super) fn catalogue_window(&mut self, ctx: &egui::Context) {
        if !self.catalogue.open {
            return;
        }
        // pictures of the starters, the user's own and the project's types
        let mut shown: Vec<(CatalogueObject, Option<ElementId>, bool)> =
            creabim_core::catalogue::starters()
                .into_iter()
                .map(|c| (c, None, false))
                .collect();
        let user: Vec<CatalogueObject> = self.user_catalogue().to_vec();
        shown.extend(user.into_iter().map(|c| (c, None, true)));
        let model = self.doc.model();
        let project: Vec<(CatalogueObject, Option<ElementId>, bool)> =
            creabim_core::types::types(model)
                .into_iter()
                .filter(|(_, _, t)| matches!(t.params, creabim_core::TypeParams::Object(_)))
                .filter_map(|(id, name, _)| {
                    let mut c = objects::from_type(model, id, &format!("project/{}", id.0))?;
                    c.name = name.to_string();
                    Some((c, Some(id), false))
                })
                .collect();
        let rev = self.doc.model().revision();
        for (c, own, _) in shown.iter().chain(&project) {
            let key = match own {
                Some(_) => format!("{}@{rev}", c.key),
                None => c.key.clone(),
            };
            self.catalogue.thumbs.entry(key).or_insert_with_key(|key| {
                ctx.load_texture(key, thumbnail(c, THUMB), egui::TextureOptions::LINEAR)
            });
        }
        let selected = self.selection.all();
        let mut open = true;
        let mut asks = vec![];
        let area = self.mat.area.unwrap_or_else(|| ctx.content_rect());
        let current = self.plan.objects.current.as_ref().map(|c| c.key.clone());
        // as wide as five pictures, or what the views leave
        let width = (area.width() - 120.0).clamp(420.0, 5.0 * (THUMB as f32 + 26.0));
        egui::Window::new("Catalogue")
            .resizable(true)
            .open(&mut open)
            .default_pos(area.min + Vec2::new(40.0, 64.0))
            .constrain_to(area)
            .default_size([width, (area.height() - 90.0).clamp(360.0, 640.0)])
            .collapsible(false)
            .show(ctx, |ui| {
                let cat = &mut self.catalogue;
                ui.horizontal_wrapped(|ui| {
                    ui.selectable_value(&mut cat.category, None, "All");
                    for c in ObjectCategory::ALL {
                        ui.selectable_value(&mut cat.category, Some(c), c.label());
                    }
                });
                ui.add(
                    egui::TextEdit::singleline(&mut cat.filter)
                        .hint_text("Search")
                        .desired_width(220.0),
                );
                ui.separator();
                let filter = cat.filter.to_lowercase();
                let wanted = |c: &CatalogueObject| {
                    cat.category.is_none_or(|k| k == c.object.category)
                        && (filter.is_empty()
                            || c.name.to_lowercase().contains(&filter)
                            || c.description.to_lowercase().contains(&filter))
                };
                let tile = |ui: &mut egui::Ui,
                            c: &CatalogueObject,
                            own: Option<ElementId>,
                            user: bool,
                            asks: &mut Vec<CatAsk>| {
                    let key = match own {
                        Some(_) => format!("{}@{rev}", c.key),
                        None => c.key.clone(),
                    };
                    let tex = cat.thumbs.get(&key);
                    let picked = current.as_deref() == Some(c.key.as_str());
                    let s = c.object.size();
                    ui.vertical(|ui| {
                        ui.set_width(THUMB as f32 + 16.0);
                        let r = match tex {
                            Some(t) => ui.add(
                                egui::Button::image(
                                    egui::Image::new(t)
                                        .fit_to_exact_size(Vec2::splat(THUMB as f32)),
                                )
                                .selected(picked),
                            ),
                            None => ui.add(egui::Button::new(&c.name).selected(picked)),
                        };
                        let r = r.on_hover_text(format!(
                            "{}\n{} × {} × {}\n{} {}\n{}",
                            c.name,
                            crate::units::len(s.x),
                            crate::units::len(s.y),
                            crate::units::len(s.z),
                            c.object.ifc.class,
                            c.object.ifc.predefined,
                            if c.description.is_empty() {
                                "Click to place it"
                            } else {
                                &c.description
                            }
                        ));
                        if r.clicked() {
                            asks.push(CatAsk::Place(Box::new(c.clone()), own));
                        }
                        if user {
                            r.context_menu(|ui| {
                                if ui.button("Delete from the catalogue").clicked() {
                                    asks.push(CatAsk::Delete(c.key.clone()));
                                    ui.close();
                                }
                            });
                        }
                        ui.add(egui::Label::new(egui::RichText::new(&c.name).size(11.5)).wrap());
                    });
                };
                let bottom = 118.0;
                egui::ScrollArea::vertical()
                    .id_salt("catalogue_items")
                    .max_height((ui.available_height() - bottom).max(160.0))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for (heading, rows) in [
                            ("Starters", shown.iter().filter(|r| !r.2).collect::<Vec<_>>()),
                            ("Yours", shown.iter().filter(|r| r.2).collect()),
                            ("In this project", project.iter().collect()),
                        ] {
                            let rows: Vec<_> = rows.into_iter().filter(|r| wanted(&r.0)).collect();
                            if rows.is_empty() {
                                if heading == "Yours" && cat.category.is_none() && filter.is_empty()
                                {
                                    ui.label(
                                        egui::RichText::new(
                                            "YOURS: nothing yet. Import one from Blender, FreeCAD or glTF, or save a selection.",
                                        )
                                        .size(10.5)
                                        .color(ui.visuals().weak_text_color()),
                                    );
                                }
                                continue;
                            }
                            ui.label(
                                egui::RichText::new(heading.to_uppercase())
                                    .size(10.5)
                                    .strong()
                                    .color(ui.visuals().weak_text_color()),
                            );
                            // rows of as many pictures as fit across
                            let per = ((ui.available_width() / (THUMB as f32 + 26.0)) as usize).max(1);
                            for chunk in rows.chunks(per) {
                                ui.horizontal_top(|ui| {
                                    for (c, own, user) in chunk {
                                        tile(ui, c, *own, *user, &mut asks);
                                    }
                                });
                            }
                            ui.add_space(6.0);
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    if ui
                        .button("Import from Blender, FreeCAD or glTF…")
                        .on_hover_text("A .blend, .FCStd, .gltf or .glb file becomes one object of your catalogue, each mesh a part in its material")
                        .clicked()
                    {
                        asks.push(CatAsk::Import);
                    }
                    if ui
                        .small_button("Open folder")
                        .on_hover_text("Where your catalogue is kept: one .json file per object")
                        .clicked()
                    {
                        asks.push(CatAsk::Reveal);
                    }
                });
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut cat.save_name)
                            .hint_text("Name")
                            .desired_width(200.0),
                    );
                    let r = ui.add_enabled(
                        !selected.is_empty(),
                        egui::Button::new("Save selection to catalogue"),
                    );
                    if r.on_hover_text("An object's type, or masses and other elements as one new object, kept for every project")
                        .on_disabled_hover_text("Select an object, masses or other elements first")
                        .clicked()
                    {
                        asks.push(CatAsk::SaveSelection);
                    }
                });
                ui.label(
                    egui::RichText::new(
                        "Click an object, then click in a plan or the 3D view to place it. Space turns it.",
                    )
                    .weak(),
                );
            });
        self.catalogue.open = open;
        for a in asks {
            self.run_catalogue(a);
        }
    }

    fn run_catalogue(&mut self, ask: CatAsk) {
        match ask {
            CatAsk::Place(c, own) => {
                self.status = format!(
                    "{}: click in a plan or the 3D view to place it. Space turns it; Esc ends.",
                    c.name
                );
                self.plan.objects.current = Some(*c);
                self.plan.objects.project_type = own;
                self.plan.cancel();
                self.plan.tool = Tool::Object;
            }
            CatAsk::Import => {
                let Some(path) = rfd::FileDialog::new()
                    .set_title("Import an object")
                    .add_filter(
                        "Objects (Blender, FreeCAD, glTF)",
                        &["blend", "FCStd", "fcstd", "gltf", "glb"],
                    )
                    .pick_file()
                else {
                    return;
                };
                self.import_object(&path);
            }
            CatAsk::SaveSelection => {
                let Some(dir) = user_dir() else {
                    self.status = "There is no settings folder to keep the catalogue in.".into();
                    return;
                };
                let ids = self.selection.all();
                let model = self.doc.model();
                let name = match self.catalogue.save_name.trim() {
                    "" => ids
                        .first()
                        .and_then(|i| model.get(*i))
                        .and_then(|e| match &e.kind {
                            ElementKind::Object(_) => {
                                creabim_core::types::type_of(model, e).map(|t| t.0.name.clone())
                            }
                            _ => Some(e.name.clone()),
                        })
                        .unwrap_or_else(|| "Object".into()),
                    n => n.to_string(),
                };
                let key = free_key(&dir, &name);
                let Some(c) = from_selection(model, &ids, &name, &key) else {
                    self.status = "Nothing in the selection has a shape to keep.".into();
                    return;
                };
                match save_to(&dir, &c) {
                    Ok(_) => {
                        self.status = format!("Saved {} to your catalogue.", c.name);
                        self.catalogue.user = None;
                        self.catalogue.open = true;
                        self.catalogue.save_name.clear();
                    }
                    Err(e) => self.status = format!("Couldn't save it: {e}"),
                }
            }
            CatAsk::Delete(key) => {
                if let Some(dir) = user_dir() {
                    match delete_from(&dir, &key) {
                        Ok(()) => {
                            self.status =
                                "Deleted from your catalogue. Objects placed from it stay.".into()
                        }
                        Err(e) => self.status = format!("Couldn't delete it: {e}"),
                    }
                }
                self.catalogue.user = None;
                self.catalogue.thumbs.remove(&key);
            }
            CatAsk::Reveal => {
                if let Some(dir) = user_dir() {
                    let _ = std::fs::create_dir_all(&dir);
                    self.status = format!("Your catalogue is in {}", dir.display());
                }
            }
        }
    }

    /// Read a file as a new object of the user's catalogue.
    pub(super) fn import_object(&mut self, path: &Path) {
        let Some(dir) = user_dir() else {
            self.status = "There is no settings folder to keep the catalogue in.".into();
            return;
        };
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("object")
            .to_string();
        let key = free_key(&dir, &stem);
        match creabim_exchange::catalogue::read_file(path, &key) {
            Ok(c) => match save_to(&dir, &c) {
                Ok(_) => {
                    let s = c.object.size();
                    self.status = format!(
                        "Added {} to your catalogue: {} parts, {} × {} × {}, as {}. Click it to place it; set its category and IFC class in Properties once placed.",
                        c.name,
                        c.object.parts.len(),
                        crate::units::len(s.x),
                        crate::units::len(s.y),
                        crate::units::len(s.z),
                        c.object.ifc.class
                    );
                    self.catalogue.user = None;
                    self.catalogue.open = true;
                    self.catalogue.category = None;
                }
                Err(e) => self.status = format!("Couldn't save it: {e}"),
            },
            Err(e) => self.status = format!("Couldn't read it: {e}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("creabim-cat-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn the_users_catalogue_keeps_objects_as_files() {
        let dir = scratch("keep");
        let sofa = creabim_core::catalogue::starter("starter/sofa-3").unwrap();
        let key = free_key(&dir, "My sofa!");
        assert_eq!(key, "user/my-sofa");
        let mut mine = sofa.clone();
        mine.key = key.clone();
        mine.name = "My sofa".into();
        save_to(&dir, &mine).unwrap();
        assert_eq!(free_key(&dir, "my sofa"), "user/my-sofa-2");
        let back = load_dir(&dir);
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].key, key);
        assert_eq!(back[0].object.catalogue.as_deref(), Some(key.as_str()));
        assert_eq!(back[0].object.parts.len(), sofa.object.parts.len());
        delete_from(&dir, &key).unwrap();
        assert!(load_dir(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn thumbnails_show_the_object() {
        let c = creabim_core::catalogue::starter("starter/bathtub").unwrap();
        let img = thumbnail(&c, 48);
        assert_eq!(img.size, [48, 48]);
        let solid = img.pixels.iter().filter(|p| p.a() > 200).count();
        assert!(solid > 48 * 48 / 8, "{solid}");
        // and leaves the background clear
        assert_eq!(img.pixels[0].a(), 0);
    }

    #[test]
    fn a_selection_becomes_a_catalogue_object() {
        let model = creabim_core::sample::furnished_house();
        let obj = model
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Object(_)))
            .unwrap();
        let c = from_selection(&model, &[obj.id], "Mine", "user/mine").unwrap();
        assert_eq!(c.name, "Mine");
        assert_eq!(c.object.catalogue.as_deref(), Some("user/mine"));
        assert!(!c.object.parts.is_empty());
        // a wall, as its shape
        let wall = model
            .iter()
            .find(|e| matches!(e.kind, ElementKind::Wall(_)))
            .unwrap();
        let w = from_selection(&model, &[wall.id], "Partition", "user/p").unwrap();
        let (lo, hi) = w.object.bounds();
        assert!(lo.z.abs() < 1e-6 && hi.y.abs() < 1e-6, "{lo} {hi}");
        assert!(w.materials.iter().all(|m| m.is_some()));
    }
}
