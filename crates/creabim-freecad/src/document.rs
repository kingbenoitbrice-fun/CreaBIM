//! Reading `.FCStd`: the zip container, `Document.xml` and `GuiDocument.xml`.

use crate::brep;
use crate::math::{v3, Trsf};
use crate::{meshkernel, FcDocument, FcError, FcMesh, FcObject};
use roxmltree::{Document, Node};
use std::collections::HashMap;
use std::io::{Cursor, Read};

/// Upper bound for one decompressed archive member (zip-bomb guard).
const MAX_ENTRY: u64 = 1 << 30;

/// Properties that are mapped to dedicated fields (or are pure noise) and
/// therefore not repeated in `FcObject::properties`.
const SKIPPED: &[&str] = &[
    "Label",
    "Label2",
    "Placement",
    "Shape",
    "Proxy",
    "Visibility",
    "Group",
    "IfcType",
    "Mesh",
    "ExpressionEngine",
    "AttacherType",
    "AttacherEngine",
    "MapMode",
    "MapPathParameter",
    "MapReversed",
    "AttachmentOffset",
    "AttachmentSupport",
    "Support",
    "ShapeMaterial",
    "IfcData",
    "IfcProperties",
    "SavedInventor",
];

struct Archive {
    zip: zip::ZipArchive<Cursor<Vec<u8>>>,
}

impl Archive {
    fn file(&mut self, name: &str) -> Option<Vec<u8>> {
        let f = self.zip.by_name(name).ok()?;
        let mut buf = Vec::new();
        f.take(MAX_ENTRY).read_to_end(&mut buf).ok()?;
        Some(buf)
    }
}

fn xml_err(file: &str, e: impl std::fmt::Display) -> FcError {
    FcError::Xml {
        file: file.into(),
        message: e.to_string(),
    }
}

/// Raw per-object data gathered from Document.xml before geometry loading.
struct RawObject {
    obj: FcObject,
    shape_file: Option<String>,
    mesh_file: Option<String>,
    group: Vec<String>,
}

pub fn read(bytes: &[u8]) -> Result<FcDocument, FcError> {
    let zip = zip::ZipArchive::new(Cursor::new(bytes.to_vec()))
        .map_err(|e| FcError::Zip(e.to_string()))?;
    let mut ar = Archive { zip };
    let doc_xml = ar.file("Document.xml").ok_or(FcError::MissingDocument)?;
    let doc_text = String::from_utf8_lossy(&doc_xml);
    let xml = Document::parse(&doc_text).map_err(|e| xml_err("Document.xml", e))?;
    let root = xml.root_element();
    let mut out = FcDocument {
        program_version: root.attribute("ProgramVersion").unwrap_or("").to_owned(),
        ..Default::default()
    };
    if let Some(props) = child(root, "Properties") {
        out.label = props_of(props)
            .get("Label")
            .and_then(|p| value_of(*p))
            .unwrap_or_default();
    }

    // <Objects> gives types in document order, <ObjectData> the properties.
    let mut raws: Vec<RawObject> = Vec::new();
    let mut by_name: HashMap<String, usize> = HashMap::new();
    if let Some(objs) = child(root, "Objects") {
        for o in objs.children().filter(|n| n.has_tag_name("Object")) {
            let name = o.attribute("name").unwrap_or("").to_owned();
            if name.is_empty() || by_name.contains_key(&name) {
                continue;
            }
            by_name.insert(name.clone(), raws.len());
            raws.push(RawObject {
                obj: FcObject {
                    label: name.clone(),
                    name,
                    type_id: o.attribute("type").unwrap_or("").to_owned(),
                    proxy_type: None,
                    ifc_type: None,
                    placement: crate::IDENTITY,
                    mesh: None,
                    properties: Vec::new(),
                    parent: None,
                    color: None,
                    visible: true,
                },
                shape_file: None,
                mesh_file: None,
                group: Vec::new(),
            });
        }
    }
    if let Some(data) = child(root, "ObjectData") {
        for o in data.children().filter(|n| n.has_tag_name("Object")) {
            let Some(&i) = o.attribute("name").and_then(|n| by_name.get(n)) else {
                continue;
            };
            if let Some(props) = child(o, "Properties") {
                fill_object(&mut raws[i], props);
            }
        }
    }

    // Link values hold internal names; show the target's label instead.
    let labels: HashMap<String, String> = raws
        .iter()
        .map(|r| (r.obj.name.clone(), r.obj.label.clone()))
        .collect();
    for r in raws.iter_mut() {
        for (_, v) in r.obj.properties.iter_mut() {
            if let Some(name) = v.strip_prefix("\u{0}link:") {
                *v = labels.get(name).cloned().unwrap_or_else(|| name.to_owned());
            }
        }
    }
    let parents: Vec<(usize, usize)> = raws
        .iter()
        .enumerate()
        .flat_map(|(gi, r)| {
            let by_name = &by_name;
            r.group
                .iter()
                .filter_map(move |c| by_name.get(c).map(|&ci| (ci, gi)))
        })
        .collect();
    for (ci, gi) in parents {
        if ci != gi && raws[ci].obj.parent.is_none() {
            raws[ci].obj.parent = Some(gi);
        }
    }

    read_gui(&mut ar, &mut raws, &by_name, &mut out.warnings);

    for r in raws.iter_mut() {
        let label = r.obj.label.clone();
        if r.obj.proxy_type.as_deref() == Some("BuildingPart") {
            continue; // Its shape is a compound of its children.
        }
        if let Some(f) = r.shape_file.clone() {
            match ar.file(&f) {
                None => out
                    .warnings
                    .push(format!("{label}: shape file '{f}' missing from archive")),
                Some(data) if data.is_empty() => {}
                Some(data) if brep::is_binary(&data) => out.warnings.push(format!(
                    "{label}: '{f}' is a binary BREP (FreeCAD 'SaveBinaryBrep' option), \
                     which is not supported yet; re-save with that option off"
                )),
                Some(data) => {
                    let text = String::from_utf8_lossy(&data);
                    match brep::parse::parse(&text) {
                        Ok(model) => {
                            let t = brep::tess::tessellate(&model);
                            for w in t.warnings {
                                out.warnings.push(format!("{label}: {w}"));
                            }
                            if !t.triangles.is_empty() {
                                r.obj.mesh = Some(FcMesh {
                                    vertices: t.vertices.iter().map(|p| p.to_array()).collect(),
                                    faces: t.triangles.iter().map(|t| t.to_vec()).collect(),
                                });
                            }
                        }
                        Err(e) => out.warnings.push(format!("{label}: {e}")),
                    }
                }
            }
        } else if let Some(f) = r.mesh_file.clone() {
            match ar.file(&f).map(|d| meshkernel::read(&d)) {
                None => out
                    .warnings
                    .push(format!("{label}: mesh file '{f}' missing from archive")),
                Some(Err(e)) => out.warnings.push(format!("{label}: {e}")),
                Some(Ok(mesh)) if mesh.faces.is_empty() => {}
                // Mesh kernels are stored in local coordinates.
                Some(Ok(mesh)) => r.obj.mesh = Some(mesh.transformed(&r.obj.placement)),
            }
        }
    }

    // Children of an App::Part live in its coordinate system.
    let n = raws.len();
    for i in 0..n {
        let mut t = Trsf::IDENTITY;
        let mut p = raws[i].obj.parent;
        let mut guard = 0;
        while let Some(pi) = p {
            if guard > n {
                break;
            }
            guard += 1;
            if matches!(raws[pi].obj.type_id.as_str(), "App::Part") {
                t = Trsf::from_cols(&raws[pi].obj.placement).then_after(&t);
            }
            p = raws[pi].obj.parent;
        }
        if t != Trsf::IDENTITY {
            if let Some(m) = raws[i].obj.mesh.take() {
                raws[i].obj.mesh = Some(m.transformed(&t.to_cols()));
            }
        }
    }

    out.objects = raws.into_iter().map(|r| r.obj).collect();
    Ok(out)
}

fn child<'a, 'i>(n: Node<'a, 'i>, tag: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.has_tag_name(tag))
}

fn props_of<'a, 'i>(props: Node<'a, 'i>) -> HashMap<&'a str, Node<'a, 'i>> {
    props
        .children()
        .filter(|n| n.has_tag_name("Property"))
        .filter_map(|n| Some((n.attribute("name")?, n)))
        .collect()
}

/// First element child of a `<Property>`.
fn payload<'a, 'i>(p: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    p.children().find(|c| c.is_element())
}

/// Text value of a simple property; enumerations resolve to their name.
fn value_of(p: Node) -> Option<String> {
    let e = payload(p)?;
    match e.tag_name().name() {
        "String" | "Float" | "Bool" | "Uuid" => e.attribute("value").map(str::to_owned),
        "Integer" => {
            let v = e.attribute("value")?;
            // Enumerations carry their item list when it is custom.
            let list = p.children().find(|c| c.has_tag_name("CustomEnumList"));
            if let Some(list) = list {
                let idx: usize = v.parse().ok()?;
                let items: Vec<&str> = list
                    .children()
                    .filter(|c| c.has_tag_name("Enum"))
                    .filter_map(|c| c.attribute("value"))
                    .collect();
                return items.get(idx).map(|s| (*s).to_owned());
            }
            Some(v.to_owned())
        }
        "Link" => e
            .attribute("value")
            .filter(|v| !v.is_empty())
            .map(|v| format!("\u{0}link:{v}")),
        _ => None,
    }
}

fn fill_object(raw: &mut RawObject, props: Node) {
    let obj = &mut raw.obj;
    for (name, p) in props_of(props) {
        let ty = p.attribute("type").unwrap_or("");
        match name {
            "Label" => {
                if let Some(v) = value_of(p) {
                    obj.label = v;
                }
            }
            "Placement" => {
                if let Some(e) = child(p, "PropertyPlacement") {
                    obj.placement = placement(e).to_cols();
                }
            }
            "Visibility" => {
                if let Some(v) = value_of(p) {
                    obj.visible = v == "true";
                }
            }
            "Proxy" => {
                if let Some(py) = child(p, "Python") {
                    obj.proxy_type = proxy_type(py);
                }
            }
            "IfcType" => obj.ifc_type = value_of(p).filter(|s| !s.is_empty()),
            "Group" => {
                if let Some(list) = child(p, "LinkList") {
                    raw.group = list
                        .children()
                        .filter(|c| c.has_tag_name("Link"))
                        .filter_map(|c| c.attribute("value"))
                        .map(str::to_owned)
                        .collect();
                }
            }
            _ => {}
        }
        if ty == "Part::PropertyPartShape" && name == "Shape" {
            raw.shape_file = child(p, "Part")
                .and_then(|e| e.attribute("file"))
                .filter(|f| !f.is_empty())
                .map(str::to_owned);
        } else if ty == "Mesh::PropertyMeshKernel" {
            raw.mesh_file = child(p, "Mesh")
                .and_then(|e| e.attribute("file"))
                .filter(|f| !f.is_empty())
                .map(str::to_owned);
        } else if !SKIPPED.contains(&name) {
            if let Some(v) = value_of(p).filter(|v| !v.is_empty()) {
                obj.properties.push((name.to_owned(), v));
            }
        }
    }
    obj.properties.sort();
}

fn attr_f64(e: Node, name: &str, default: f64) -> f64 {
    e.attribute(name)
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(default)
}

fn placement(e: Node) -> Trsf {
    let t = v3(
        attr_f64(e, "Px", 0.0),
        attr_f64(e, "Py", 0.0),
        attr_f64(e, "Pz", 0.0),
    );
    let q = [
        attr_f64(e, "Q0", 0.0),
        attr_f64(e, "Q1", 0.0),
        attr_f64(e, "Q2", 0.0),
        attr_f64(e, "Q3", 1.0),
    ];
    Trsf::from_quat(t, q)
}

/// Arch/BIM proxies pickle their `Type` (e.g. `"Wall"`) as base64 JSON;
/// fall back to the class name without its leading underscore.
fn proxy_type(py: Node) -> Option<String> {
    let encoded = py.attribute("encoded") == Some("yes");
    let value = py.attribute("value").unwrap_or("");
    let state = if encoded {
        base64_decode(value).map(|b| String::from_utf8_lossy(&b).into_owned())
    } else {
        Some(value.to_owned())
    };
    if let Some(s) = state {
        let s = s.trim();
        if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
            let inner = &s[1..s.len() - 1];
            if !inner.is_empty() && !inner.contains('"') {
                return Some(inner.to_owned());
            }
        }
    }
    py.attribute("class")
        .map(|c| c.trim_start_matches('_').to_owned())
        .filter(|c| !c.is_empty())
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut acc = 0u32;
    let mut bits = 0;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' | b'\n' | b'\r' | b' ' => continue,
            _ => return None,
        } as u32;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

/// Colours and visibility from GuiDocument.xml (absent in files saved by
/// FreeCADCmd). FreeCAD 1.x stores colours in a binary `ShapeAppearance`
/// material list, 0.x in a packed `ShapeColor`.
fn read_gui(
    ar: &mut Archive,
    raws: &mut [RawObject],
    by_name: &HashMap<String, usize>,
    warnings: &mut Vec<String>,
) {
    let Some(data) = ar.file("GuiDocument.xml") else {
        return;
    };
    let text = String::from_utf8_lossy(&data);
    let xml = match Document::parse(&text) {
        Ok(x) => x,
        Err(e) => {
            warnings.push(format!("GuiDocument.xml ignored: {e}"));
            return;
        }
    };
    let Some(vpd) = child(xml.root_element(), "ViewProviderData") else {
        return;
    };
    for vp in vpd.children().filter(|n| n.has_tag_name("ViewProvider")) {
        let Some(&i) = vp.attribute("name").and_then(|n| by_name.get(n)) else {
            continue;
        };
        let Some(props) = child(vp, "Properties") else {
            continue;
        };
        let props = props_of(props);
        if let Some(v) = props.get("Visibility").and_then(|p| value_of(*p)) {
            raws[i].obj.visible = v == "true";
        }
        let transparency = props
            .get("Transparency")
            .and_then(|p| value_of(*p))
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.0)
            / 100.0;
        let appearance = props
            .get("ShapeAppearance")
            .and_then(|p| child(*p, "MaterialList"))
            .and_then(|e| e.attribute("file"))
            .and_then(|f| ar.file(f))
            .and_then(|d| first_material(&d));
        let packed = props
            .get("ShapeColor")
            .and_then(|p| child(*p, "PropertyColor"))
            .and_then(|e| e.attribute("value"))
            .and_then(|v| v.parse::<u32>().ok());
        raws[i].obj.color = match (appearance, packed) {
            (Some(c), _) => Some(c),
            (None, Some(v)) => {
                let [r, g, b] = unpack(v);
                Some([r, g, b, (1.0 - transparency).clamp(0.0, 1.0)])
            }
            _ => None,
        };
    }
}

/// FreeCAD's packed colour: `0xRRGGBBAA`.
fn unpack(v: u32) -> [f32; 3] {
    [
        ((v >> 24) & 255) as f32 / 255.0,
        ((v >> 16) & 255) as f32 / 255.0,
        ((v >> 8) & 255) as f32 / 255.0,
    ]
}

/// Diffuse colour and transparency of the first entry of a material list:
/// `u32 count`, then per material ambient, diffuse, specular, emissive
/// (packed `u32`), shininess and transparency (`f32`).
fn first_material(d: &[u8]) -> Option<[f32; 4]> {
    let u = |o: usize| {
        d.get(o..o + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    };
    if u(0)? == 0 {
        return None;
    }
    let [r, g, b] = unpack(u(8)?);
    let t = f32::from_bits(u(24)?);
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };
    Some([r, g, b, 1.0 - t])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_proxy_state() {
        assert_eq!(base64_decode("IldhbGwi").unwrap(), b"\"Wall\"");
        assert_eq!(base64_decode("IldpbmRvdyI=").unwrap(), b"\"Window\"");
        assert!(base64_decode("@@").is_none());
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(read(b"not a zip").is_err());
        assert!(read(&[]).is_err());
    }
}
