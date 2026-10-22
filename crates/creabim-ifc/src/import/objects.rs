//! Furniture and fixtures: an IfcFurnitureType, IfcSanitaryTerminalType,
//! IfcElectricApplianceType or IfcSystemFurnitureElementType with a body
//! among its representation maps becomes an object type, each item of the
//! body a part in the material its style belongs to, a 'FootPrint' map its
//! plan symbol; the products of such a type become objects placed where
//! the file has them.

use super::{material_from, Importer};
use creabim_core::objects::{self, IfcKind, ObjectPart, SymbolItem};
use creabim_core::{EditMesh, Element, ElementKind, TypeCategory, TypeRef};
use glam::{DMat4, DVec2};
use std::collections::HashMap;

impl Importer<'_> {
    /// The object types of the file.
    pub(super) fn read_object_types(&mut self) {
        let f = self.f;
        // which material each surface style is the look of, and each
        // item's style, to give each part its material
        let mut style_material: HashMap<u64, u64> = HashMap::new();
        for &mdr in self.ids("IFCMATERIALDEFINITIONREPRESENTATION") {
            let Some(e) = f.get(mdr) else { continue };
            let Some(m) = e.arg(3).as_ref() else { continue };
            for rep in e.arg(2).refs().filter_map(|r| f.get(r)) {
                for si in rep.arg(3).refs().filter_map(|i| f.get(i)) {
                    for s in si.arg(1).refs() {
                        style_material.entry(s).or_insert(m);
                    }
                }
            }
        }
        let mut item_style: HashMap<u64, u64> = HashMap::new();
        for &si in self.ids("IFCSTYLEDITEM") {
            let Some(e) = f.get(si) else { continue };
            if let (Some(item), Some(style)) = (e.arg(0).as_ref(), e.arg(1).refs().next()) {
                item_style.insert(item, style);
            }
        }
        let mut ids: Vec<(u64, &'static str)> = f
            .entities
            .iter()
            .filter_map(|(id, e)| {
                let class = objects::IFC_CLASSES
                    .iter()
                    .find(|c| c.1.eq_ignore_ascii_case(&e.class))?;
                Some((*id, class.0))
            })
            .collect();
        ids.sort_unstable();
        for (id, class) in ids {
            let Some(e) = f.get(id) else { continue };
            let mut parts: Vec<ObjectPart> = vec![];
            let mut symbol: Vec<SymbolItem> = vec![];
            for map in e.arg(6).refs().filter_map(|m| f.get(m)) {
                let origin = self.g.axis_placement(map.arg(0));
                let Some(rep) = self.g.get(map.arg(1)) else {
                    continue;
                };
                let ident = rep.arg(1).as_str().unwrap_or("");
                let items: Vec<u64> = rep.arg(3).refs().collect();
                if ident.eq_ignore_ascii_case("FootPrint") {
                    for item in items {
                        symbol.extend(self.symbol_items(item, &origin));
                    }
                    continue;
                }
                if !ident.eq_ignore_ascii_case("Body") || !parts.is_empty() {
                    continue;
                }
                for item in items {
                    let mut mesh = EditMesh::default();
                    if !self.g.item(item, &origin, &mut mesh, 0) {
                        continue;
                    }
                    let material = self.part_material(item, &item_style, &style_material);
                    parts.push(ObjectPart {
                        name: String::new(),
                        mesh,
                        material,
                    });
                }
            }
            if parts.is_empty() {
                continue;
            }
            // part names from the type's constituents, when there is one for each
            let names: Vec<String> = self
                .materials_of
                .get(&id)
                .and_then(|m| f.get(*m))
                .filter(|m| m.is("IFCMATERIALCONSTITUENTSET"))
                .map(|cs| {
                    cs.arg(2)
                        .refs()
                        .filter_map(|c| f.get(c))
                        .map(|c| c.arg(0).as_str().unwrap_or("").to_string())
                        .collect()
                })
                .unwrap_or_default();
            if names.len() == parts.len() {
                for (p, n) in parts.iter_mut().zip(names) {
                    p.name = n;
                }
            }
            let props = self.props(id, crate::params::TYPE_PSET);
            let creabim_core::TypeParams::Object(mut t) =
                crate::params::type_from(TypeCategory::Object, false, &props)
            else {
                continue;
            };
            let predefined = e
                .args
                .last()
                .and_then(|v| v.as_enum())
                .unwrap_or("NOTDEFINED");
            t.ifc = IfcKind::new(class, predefined).normalized();
            t.parts = parts;
            t.symbol = symbol;
            if props.is_empty() {
                // from another tool: free-standing unless it says otherwise
                t.mount = objects::Mount::Floor;
            }
            let mut ty = creabim_core::ElementType::new(creabim_core::TypeParams::Object(t));
            ty.description = e.arg(3).as_str().unwrap_or("").trim().to_string();
            let eid = self.new_id(e);
            self.type_elems.push(Element {
                id: eid,
                name: Self::name(e, "Object type"),
                kind: ElementKind::ElementType(ty),
                source: None,
                of_type: None,
            });
            self.types.insert(id, eid);
            self.object_types.insert(id);
        }
    }

    /// The products of the object types read: placed objects.
    pub(super) fn import_objects(&mut self) {
        let mut ids: Vec<(u64, u64)> = self
            .type_of
            .iter()
            .filter(|(_, t)| self.object_types.contains(t))
            .map(|(p, t)| (*p, *t))
            .collect();
        ids.sort_unstable();
        for (id, t) in ids {
            if self.done.contains(&id) {
                continue;
            }
            let Some(e) = self.f.get(id) else { continue };
            if e.arg(5).is_null() {
                continue;
            }
            let Some(&type_id) = self.types.get(&t) else {
                continue;
            };
            self.done.insert(id);
            let world = self.g.placement(e.arg(5));
            let at = world.w_axis.truncate();
            let x = world.x_axis.truncate();
            let rotation = x.y.atan2(x.x).to_degrees().rem_euclid(360.0);
            let (lid, elev) = self.level_for(id, at.z);
            let mut o = objects::Object::new(lid, at.truncate(), rotation);
            o.offset = at.z - elev;
            let eid = self.new_id(e);
            self.other_elems.push(Element {
                id: eid,
                name: Self::name(e, "Object"),
                kind: ElementKind::Object(o),
                source: None,
                of_type: Some(TypeRef::new(type_id)),
            });
        }
    }

    /// A footprint item's lines in the type's axes: closed polylines as
    /// outlines, open ones as their segments.
    fn symbol_items(&mut self, item: u64, origin: &DMat4) -> Vec<SymbolItem> {
        let f = self.f;
        let Some(e) = f.get(item) else {
            return vec![];
        };
        let curves: Vec<crate::step_read::Value> =
            if e.is("IFCGEOMETRICCURVESET") || e.is("IFCGEOMETRICSET") {
                e.arg(0).as_list().to_vec()
            } else {
                vec![crate::step_read::Value::Ref(item)]
            };
        let mut out = vec![];
        for c in &curves {
            let Some(pts) = self.g.curve(c) else { continue };
            let pts: Vec<DVec2> = pts
                .iter()
                .map(|p| origin.transform_point3(*p).truncate())
                .collect();
            if pts.len() < 2 {
                continue;
            }
            if pts.len() > 3 && pts[0].distance(pts[pts.len() - 1]) < 1e-9 {
                out.push(SymbolItem::Loop(pts[..pts.len() - 1].to_vec()));
            } else {
                out.extend(pts.windows(2).map(|w| SymbolItem::Line(w[0], w[1])));
            }
        }
        out
    }

    /// The material of a part: the one whose look its style is, else one
    /// made from the style itself.
    fn part_material(
        &mut self,
        item: u64,
        item_style: &HashMap<u64, u64>,
        style_material: &HashMap<u64, u64>,
    ) -> Option<creabim_core::ElementId> {
        let style = *item_style.get(&item)?;
        if let Some(m) = style_material.get(&style) {
            return self.material_entity(*m);
        }
        let surface = self.styles.get(&item).copied()?;
        let name = self
            .f
            .get(style)
            .and_then(|s| s.arg(0).as_str())
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| {
                let [r, g, b] = surface.color.map(|c| (c * 255.0) as u8);
                format!("Colour #{r:02x}{g:02x}{b:02x}")
            });
        if let Some(&m) = self.materials.get(&name) {
            return Some(m);
        }
        let material = material_from(self.f, &name, None, Some(surface));
        let el = Element::new(name.clone(), ElementKind::Material(material));
        let id = el.id;
        self.material_elems.push(el);
        self.materials.insert(name, id);
        Some(id)
    }
}
