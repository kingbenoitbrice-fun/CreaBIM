//! Writing `.FCStd`: plain `Part::Feature` objects with polyhedral BREP
//! shapes, `App::DocumentObjectGroup`s, and a GUI document with colours.
//!
//! No Python proxies are used, so FreeCAD opens the file without any
//! workbench. Colours are written both as `ShapeColor` (read by 0.x) and as
//! a `ShapeAppearance` material list (read by 1.x).

use crate::brep::write::write_brep;
use crate::math::{Trsf, V3};
use crate::{FcError, FcExport};
use std::collections::HashSet;
use std::fmt::Write as _;
use std::io::Write as _;

/// Writes an `.FCStd` archive.
pub fn write(doc: &FcExport) -> Result<Vec<u8>, FcError> {
    let mut used = HashSet::new();
    let group_names: Vec<String> = doc
        .groups
        .iter()
        .map(|g| unique_name(&g.label, "Group", &mut used))
        .collect();
    let obj_names: Vec<String> = doc
        .objects
        .iter()
        .map(|o| unique_name(&o.label, "Shape", &mut used))
        .collect();
    let check = |i: Option<usize>, what: &str| -> Result<Option<usize>, FcError> {
        match i {
            Some(i) if i >= doc.groups.len() => Err(FcError::Write(format!(
                "{what} refers to group {i}, but there are only {}",
                doc.groups.len()
            ))),
            other => Ok(other),
        }
    };
    let mut members: Vec<Vec<&str>> = vec![Vec::new(); doc.groups.len()];
    for (gi, g) in doc.groups.iter().enumerate() {
        if let Some(p) = check(g.parent, &g.label)? {
            if p == gi || creates_cycle(doc, gi) {
                return Err(FcError::Write(format!(
                    "group '{}' nests in itself",
                    g.label
                )));
            }
            members[p].push(&group_names[gi]);
        }
    }
    for (oi, o) in doc.objects.iter().enumerate() {
        if let Some(g) = check(o.group, &o.label)? {
            members[g].push(&obj_names[oi]);
        }
    }

    // FreeCAD reads archive members in the order it registers them: the
    // document, its shape files, then the GUI document and its files.
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut gui_files: Vec<(String, Vec<u8>)> = Vec::new();
    let mut objects_xml = String::new();
    let mut data_xml = String::new();
    let mut gui_xml = String::new();
    let total = doc.groups.len() + doc.objects.len();
    let _ = writeln!(
        objects_xml,
        "    <Objects Count=\"{total}\" Dependencies=\"1\">"
    );
    for (gi, name) in group_names.iter().enumerate() {
        let _ = writeln!(
            objects_xml,
            "        <ObjectDeps Name=\"{}\" Count=\"{}\">",
            esc(name),
            members[gi].len()
        );
        for m in &members[gi] {
            let _ = writeln!(objects_xml, "            <Dep Name=\"{}\"/>", esc(m));
        }
        objects_xml.push_str("        </ObjectDeps>\n");
    }
    for name in &obj_names {
        let _ = writeln!(
            objects_xml,
            "        <ObjectDeps Name=\"{}\" Count=\"0\"/>",
            esc(name)
        );
    }
    let mut id = 1;
    for name in &group_names {
        let _ = writeln!(
            objects_xml,
            "        <Object type=\"App::DocumentObjectGroup\" name=\"{}\" id=\"{id}\" />",
            esc(name)
        );
        id += 1;
    }
    for name in &obj_names {
        let _ = writeln!(
            objects_xml,
            "        <Object type=\"Part::Feature\" name=\"{}\" id=\"{id}\" />",
            esc(name)
        );
        id += 1;
    }
    objects_xml.push_str("    </Objects>\n");

    let _ = writeln!(data_xml, "    <ObjectData Count=\"{total}\">");
    for (gi, g) in doc.groups.iter().enumerate() {
        let name = &group_names[gi];
        let _ = writeln!(
            data_xml,
            "        <Object name=\"{}\" Extensions=\"True\">",
            esc(name)
        );
        data_xml.push_str(
            "            <Extensions Count=\"1\">\n                <Extension type=\"App::GroupExtension\" name=\"GroupExtension\">\n                </Extension>\n            </Extensions>\n",
        );
        data_xml.push_str("            <Properties Count=\"3\" TransientCount=\"0\">\n");
        let mut links = String::new();
        let _ = writeln!(links, "<LinkList count=\"{}\">", members[gi].len());
        for m in &members[gi] {
            let _ = writeln!(
                links,
                "                        <Link value=\"{}\"/>",
                esc(m)
            );
        }
        links.push_str("                    </LinkList>");
        prop(&mut data_xml, "Group", "App::PropertyLinkList", &links, "");
        prop_string(&mut data_xml, "Label", &g.label, "");
        prop(
            &mut data_xml,
            "Visibility",
            "App::PropertyBool",
            "<Bool value=\"true\"/>",
            "",
        );
        data_xml.push_str("            </Properties>\n        </Object>\n");
        view_provider(&mut gui_xml, name, None, true, &mut gui_files);
    }

    for (oi, o) in doc.objects.iter().enumerate() {
        let name = &obj_names[oi];
        let mut verts: Vec<V3> = o.mesh.vertices.iter().map(|v| V3::from_array(*v)).collect();
        let mut placement = Trsf::from_cols(&o.placement);
        if !placement.is_finite() {
            placement = Trsf::IDENTITY;
        }
        let rigid = placement.to_quat();
        let (pos, quat) = match rigid {
            Some(pq) => pq,
            None => {
                // FreeCAD placements are rigid: bake anything else in.
                for v in verts.iter_mut() {
                    *v = placement.point(*v);
                }
                placement = Trsf::IDENTITY;
                (V3::default(), [0.0, 0.0, 0.0, 1.0])
            }
        };
        let mut faces = o.mesh.faces.clone();
        if rigid.is_none() && Trsf::from_cols(&o.placement).is_mirror() {
            for f in faces.iter_mut() {
                f.reverse();
            }
        }
        let root = (placement != Trsf::IDENTITY).then_some(&placement);
        let brep = write_brep(&verts, &faces, root);
        let shape_file = if brep.faces > 0 {
            let f = format!("{name}.Shape.brp");
            files.push((f.clone(), brep.text.into_bytes()));
            f
        } else {
            String::new()
        };

        let extra: Vec<(&str, &str)> = o
            .ifc_type
            .iter()
            .map(|t| ("IfcType", t.as_str()))
            .chain(
                o.properties
                    .iter()
                    .filter(|(k, _)| is_identifier(k) && !RESERVED.contains(&k.as_str()))
                    .map(|(k, v)| (k.as_str(), v.as_str())),
            )
            .collect();
        let mut seen = HashSet::new();
        let extra: Vec<_> = extra.into_iter().filter(|(k, _)| seen.insert(*k)).collect();
        let _ = writeln!(data_xml, "        <Object name=\"{}\">", esc(name));
        let _ = writeln!(
            data_xml,
            "            <Properties Count=\"{}\" TransientCount=\"0\">",
            4 + extra.len()
        );
        prop_string(&mut data_xml, "Label", &o.label, "");
        prop(
            &mut data_xml,
            "Placement",
            "App::PropertyPlacement",
            &placement_xml(pos, quat),
            "",
        );
        prop(
            &mut data_xml,
            "Shape",
            "Part::PropertyPartShape",
            &format!("<Part file=\"{}\"/>", esc(&shape_file)),
            "",
        );
        prop(
            &mut data_xml,
            "Visibility",
            "App::PropertyBool",
            &format!("<Bool value=\"{}\"/>", o.visible),
            "",
        );
        for (k, v) in extra {
            let group = if k == "IfcType" { "IFC" } else { "CreaBIM" };
            let dynamic = format!(" group=\"{group}\" doc=\"\" attr=\"0\" ro=\"0\" hide=\"0\"");
            prop_string(&mut data_xml, k, v, &dynamic);
        }
        data_xml.push_str("            </Properties>\n        </Object>\n");
        view_provider(&mut gui_xml, name, o.color, o.visible, &mut gui_files);
    }
    data_xml.push_str("    </ObjectData>\n");

    let mut xml = String::new();
    xml.push_str(HEADER);
    xml.push_str(
        "<Document SchemaVersion=\"4\" ProgramVersion=\"0.21R33771 (Git)\" FileVersion=\"1\">\n",
    );
    xml.push_str("    <Properties Count=\"1\" TransientCount=\"0\">\n");
    prop_string(&mut xml, "Label", &doc.label, "");
    xml.push_str("    </Properties>\n");
    xml.push_str(&objects_xml);
    xml.push_str(&data_xml);
    xml.push_str("</Document>\n");

    let mut gui = String::new();
    gui.push_str(HEADER);
    gui.push_str("<Document SchemaVersion=\"1\">\n");
    let _ = writeln!(gui, "    <ViewProviderData Count=\"{total}\">");
    gui.push_str(&gui_xml);
    gui.push_str("    </ViewProviderData>\n</Document>\n");

    let mut zw = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let werr = |e: &dyn std::fmt::Display| FcError::Write(e.to_string());
    let mut put = |name: &str, data: &[u8]| -> Result<(), FcError> {
        zw.start_file(name, opts).map_err(|e| werr(&e))?;
        zw.write_all(data).map_err(|e| werr(&e))
    };
    put("Document.xml", xml.as_bytes())?;
    for (n, d) in &files {
        put(n, d)?;
    }
    put("GuiDocument.xml", gui.as_bytes())?;
    for (n, d) in &gui_files {
        put(n, d)?;
    }
    let cursor = zw.finish().map_err(|e| werr(&e))?;
    Ok(cursor.into_inner())
}

const HEADER: &str = "<?xml version='1.0' encoding='utf-8'?>\n<!--\n FreeCAD Document, see https://www.freecad.org for more information...\n-->\n";

/// Properties every `Part::Feature` already has; user properties must not
/// clash with them.
const RESERVED: &[&str] = &[
    "Label",
    "Label2",
    "Placement",
    "Shape",
    "Visibility",
    "ExpressionEngine",
    "ShapeMaterial",
    "IfcType",
];

fn creates_cycle(doc: &FcExport, start: usize) -> bool {
    let mut p = doc.groups[start].parent;
    let mut steps = 0;
    while let Some(i) = p {
        if i == start || steps > doc.groups.len() {
            return true;
        }
        steps += 1;
        p = doc.groups.get(i).and_then(|g| g.parent);
    }
    false
}

fn is_identifier(s: &str) -> bool {
    let mut c = s.chars();
    c.next()
        .is_some_and(|f| f.is_ascii_alphabetic() || f == '_')
        && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

/// FreeCAD internal names: ASCII identifiers, unique, `Name001` style.
fn unique_name(label: &str, fallback: &str, used: &mut HashSet<String>) -> String {
    let mut base: String = label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    base = base.trim_matches('_').to_owned();
    if base.is_empty() {
        base = fallback.to_owned();
    }
    if base.starts_with(|c: char| c.is_ascii_digit()) {
        base.insert(0, '_');
    }
    base.truncate(48);
    let mut name = base.clone();
    let mut n = 1;
    while used.contains(&name) {
        name = format!("{base}{n:03}");
        n += 1;
    }
    used.insert(name.clone());
    name
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            '\n' => o.push_str("&#10;"),
            '\r' => o.push_str("&#13;"),
            '\t' => o.push_str("&#9;"),
            c if (c as u32) < 0x20 => {}
            c => o.push(c),
        }
    }
    o
}

fn prop(out: &mut String, name: &str, ty: &str, body: &str, extra_attrs: &str) {
    let _ = write!(
        out,
        "                <Property name=\"{}\" type=\"{ty}\"{extra_attrs}>\n                    {body}\n                </Property>\n",
        esc(name)
    );
}

fn prop_string(out: &mut String, name: &str, value: &str, extra_attrs: &str) {
    prop(
        out,
        name,
        "App::PropertyString",
        &format!("<String value=\"{}\"/>", esc(value)),
        extra_attrs,
    );
}

fn placement_xml(p: V3, q: [f64; 4]) -> String {
    let [x, y, z, w] = q;
    let s = (1.0 - w * w).max(0.0).sqrt();
    let angle = 2.0 * w.clamp(-1.0, 1.0).acos();
    let axis = if s > 1e-12 {
        V3 {
            x: x / s,
            y: y / s,
            z: z / s,
        }
    } else {
        V3 {
            x: 0.0,
            y: 0.0,
            z: 1.0,
        }
    };
    format!(
        "<PropertyPlacement Px=\"{}\" Py=\"{}\" Pz=\"{}\" Q0=\"{}\" Q1=\"{}\" Q2=\"{}\" Q3=\"{}\" A=\"{}\" Ox=\"{}\" Oy=\"{}\" Oz=\"{}\"/>",
        f(p.x), f(p.y), f(p.z), f(x), f(y), f(z), f(w), f(angle), f(axis.x), f(axis.y), f(axis.z)
    )
}

fn f(x: f64) -> String {
    format!("{:.16}", if x.is_finite() { x } else { 0.0 })
}

fn pack(c: [f32; 4]) -> u32 {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    (b(c[0]) << 24) | (b(c[1]) << 16) | (b(c[2]) << 8)
}

/// Writes a view provider; for shapes with a colour also the 1.x material
/// list file it points to.
fn view_provider(
    out: &mut String,
    name: &str,
    color: Option<[f32; 4]>,
    visible: bool,
    files: &mut Vec<(String, Vec<u8>)>,
) {
    let mut props = String::new();
    let mut count = 1;
    if let Some(c) = color {
        let transparency = ((1.0 - c[3].clamp(0.0, 1.0)) * 100.0).round() as i32;
        let file = format!("{name}.ShapeAppearance");
        files.push((file.clone(), material_list(c)));
        prop(
            &mut props,
            "ShapeAppearance",
            "App::PropertyMaterialList",
            &format!("<MaterialList file=\"{}\" version=\"3\"/>", esc(&file)),
            "",
        );
        prop(
            &mut props,
            "ShapeColor",
            "App::PropertyColor",
            &format!("<PropertyColor value=\"{}\"/>", pack(c)),
            "",
        );
        prop(
            &mut props,
            "Transparency",
            "App::PropertyPercent",
            &format!("<Integer value=\"{transparency}\"/>"),
            "",
        );
        count += 3;
    }
    prop(
        &mut props,
        "Visibility",
        "App::PropertyBool",
        &format!("<Bool value=\"{visible}\"/>"),
        "",
    );
    let _ = write!(
        out,
        "        <ViewProvider name=\"{}\" expanded=\"0\">\n            <Properties Count=\"{count}\" TransientCount=\"0\">\n{props}            </Properties>\n        </ViewProvider>\n",
        esc(name)
    );
}

/// One-entry `App::PropertyMaterialList` file (format version 3), mirroring
/// what FreeCAD 1.0 writes for a plain coloured shape.
fn material_list(c: [f32; 4]) -> Vec<u8> {
    let mut d = Vec::with_capacity(48);
    let push = |d: &mut Vec<u8>, v: u32| d.extend_from_slice(&v.to_le_bytes());
    push(&mut d, 1);
    push(&mut d, 0x3333_3300); // ambient 0.2 grey
    push(&mut d, pack(c)); // diffuse
    push(&mut d, 0); // specular
    push(&mut d, 0); // emissive
    push(&mut d, 0.9f32.to_bits()); // shininess
    push(&mut d, (1.0 - c[3].clamp(0.0, 1.0)).to_bits()); // transparency
    push(&mut d, 0); // image
    push(&mut d, 0); // image path
    push(&mut d, 0); // uuid
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_identifiers() {
        let mut used = HashSet::new();
        assert_eq!(unique_name("Wall A", "Shape", &mut used), "Wall_A");
        assert_eq!(unique_name("Wall A", "Shape", &mut used), "Wall_A001");
        assert_eq!(unique_name("", "Shape", &mut used), "Shape");
        assert_eq!(unique_name("1st", "Shape", &mut used), "_1st");
        assert_eq!(unique_name("Mur Sud é", "Shape", &mut used), "Mur_Sud");
    }

    #[test]
    fn bad_group_index_is_an_error() {
        let doc = FcExport {
            objects: vec![crate::FcExportObject {
                group: Some(3),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert!(write(&doc).is_err());
    }
}
