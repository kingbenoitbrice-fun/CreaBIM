//! Turns the generic block/SDNA view into the neutral [`BlendScene`].

use std::collections::{HashMap, HashSet};

use crate::container::BlendFile;
use crate::math::{self, Mat4};
use crate::view::{c_string, View};
use crate::{BlendMaterial, BlendMesh, BlendObject, BlendScene, ObjectKind};

/// `eCustomDataType` values of the legacy mesh arrays. These numbers are
/// part of the file format and never change.
const CD_MVERT: i64 = 0;
const CD_MFACE: i64 = 4;
const CD_MPOLY: i64 = 25;
const CD_MLOOP: i64 = 26;

/// `Object.restrictflag`: `OB_HIDE_VIEWPORT` (the monitor icon).
const OB_HIDE_VIEWPORT: i64 = 1 << 0;
/// `Base.flag`: `BASE_HIDDEN` (the eye icon, stored per view layer).
const BASE_HIDDEN: i64 = 1 << 8;
/// `Object.partype` values whose transform is plain object parenting.
const PAROBJECT: i64 = 0;
const PARSKEL: i64 = 4;

/// Guard against absurd depths in parent chains and ID property groups
/// of a malformed file.
const MAX_DEPTH: usize = 64;

pub(crate) fn extract(file: &BlendFile) -> BlendScene {
    let ids = |code: &[u8; 4]| -> Vec<(u64, View)> {
        file.blocks
            .iter()
            .filter(|b| &b.code == code)
            .filter_map(|b| Some((b.old, file.block_views(b).into_iter().next()?)))
            .collect()
    };

    let mut materials = Vec::new();
    let mut material_index = HashMap::new();
    for (addr, ma) in ids(b"MA\0\0") {
        material_index.insert(addr, materials.len());
        let c = |f: &str, d: f64| ma.float(f).unwrap_or(d) as f32;
        materials.push(BlendMaterial {
            name: id_name(&ma),
            color: [c("r", 0.8), c("g", 0.8), c("b", 0.8), c("a", 1.0)],
        });
    }

    let obs = ids(b"OB\0\0");
    let object_index: HashMap<u64, usize> =
        obs.iter().enumerate().map(|(i, (a, _))| (*a, i)).collect();

    let mut collections = vec![Vec::new(); obs.len()];
    for (_, coll) in ids(b"GR\0\0") {
        let name = id_name(&coll);
        for link in list(&coll, "gobject", "CollectionObject") {
            if let Some(&i) = link.ptr("ob").and_then(|a| object_index.get(&a)) {
                if !collections[i].contains(&name) {
                    collections[i].push(name.clone());
                }
            }
        }
    }

    let scene = current_scene(file);
    let unit_scale = scene
        .and_then(|s| s.sub("unit"))
        .and_then(|u| u.float("scale_length"))
        .filter(|s| s.is_finite() && *s > 0.0)
        .unwrap_or(1.0);
    let base_hidden = scene.map(|s| hidden_bases(&s)).unwrap_or_default();

    let parents: Vec<Option<usize>> = obs
        .iter()
        .map(|(_, ob)| ob.ptr("parent").and_then(|p| object_index.get(&p).copied()))
        .collect();
    let worlds = world_matrices(&obs, &parents);

    let mut objects = Vec::with_capacity(obs.len());
    for (i, (addr, ob)) in obs.iter().enumerate() {
        let kind = object_kind(ob.int("type").unwrap_or(-1));
        let data = ob.deref("data", "");
        let mesh = match (&kind, data) {
            (ObjectKind::Mesh, Some(me)) if me.type_name() == "Mesh" => Some(read_mesh(&me)),
            _ => None,
        };
        let restricted = ob.int("restrictflag").unwrap_or(0) & OB_HIDE_VIEWPORT != 0;
        objects.push(BlendObject {
            name: id_name(ob),
            data_name: data.filter(|d| d.has("id")).map(|d| id_name(&d)),
            kind,
            parent: parents[i],
            collections: std::mem::take(&mut collections[i]),
            world: worlds[i],
            mesh,
            material_slots: material_slots(ob, data.as_ref(), &material_index),
            custom_props: ob.sub("id").map(|id| custom_props(&id)).unwrap_or_default(),
            hidden: restricted || base_hidden.contains(addr),
        });
    }

    BlendScene {
        objects,
        materials,
        version: format!("{}.{}", file.version / 100, file.version % 100),
        unit_scale,
    }
}

/// The ID name without its two-letter type prefix (`OBCube` -> `Cube`).
fn id_name(v: &View) -> String {
    let name = v
        .sub("id")
        .and_then(|id| id.string("name"))
        .unwrap_or_default();
    name.get(2..).unwrap_or("").to_string()
}

/// Follows a `ListBase` (`first` / `next` pointers), stopping on cycles.
fn list<'f, 'a>(v: &View<'f, 'a>, field: &str, ty: &str) -> Vec<View<'f, 'a>> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let Some(head) = v.sub(field) else {
        return out;
    };
    let mut next = head.ptr("first");
    let mut from = head;
    while let Some(addr) = next {
        if !seen.insert(addr) {
            break;
        }
        let Some(item) = from.view_at(addr, ty) else {
            break;
        };
        next = item.ptr("next");
        from = item;
        out.push(item);
    }
    out
}

/// The scene the file was saved with (`FileGlobal.curscene`), else the first.
fn current_scene<'f, 'a>(file: &'f BlendFile<'a>) -> Option<View<'f, 'a>> {
    let glob = file
        .blocks
        .iter()
        .find(|b| &b.code == b"GLOB")
        .and_then(|b| file.block_views(b).into_iter().next());
    glob.and_then(|g| g.deref("curscene", "Scene"))
        .filter(|s| s.type_name() == "Scene")
        .or_else(|| {
            let b = file.blocks.iter().find(|b| &b.code == b"SC\0\0")?;
            file.block_views(b).into_iter().next()
        })
}

/// Objects hidden with the eye icon in the scene's first view layer.
fn hidden_bases(scene: &View) -> HashSet<u64> {
    let Some(layer) = list(scene, "view_layers", "ViewLayer").into_iter().next() else {
        return HashSet::new();
    };
    list(&layer, "object_bases", "Base")
        .iter()
        .filter(|b| b.int("flag").unwrap_or(0) & BASE_HIDDEN != 0)
        .filter_map(|b| b.ptr("object"))
        .collect()
}

fn object_kind(t: i64) -> ObjectKind {
    let other = match t {
        0 => return ObjectKind::Empty,
        1 => return ObjectKind::Mesh,
        2 => return ObjectKind::Curve,
        10 => return ObjectKind::Light,
        11 => return ObjectKind::Camera,
        3 => "Surface",
        4 => "Text",
        5 => "Metaball",
        12 => "Speaker",
        13 => "LightProbe",
        22 => "Lattice",
        25 => "Armature",
        26 | 30 => "GreasePencil",
        27 => "Curves",
        28 => "PointCloud",
        29 => "Volume",
        _ => "Unknown",
    };
    ObjectKind::Other(other.to_string())
}

/// World matrices for all objects.
///
/// Blender up to 4.0 saves the evaluated matrix (`obmat`), but it is stale
/// for objects hidden from the depsgraph, and later versions do not save
/// it at all. So the matrix is rebuilt from loc/rot/scale and the parent
/// chain, which is exact for object parenting. Only where that cannot be
/// right (constraints, bone or vertex parents) is the saved one used.
fn world_matrices(obs: &[(u64, View)], parents: &[Option<usize>]) -> Vec<Mat4> {
    let mut done: Vec<Option<Mat4>> = vec![None; obs.len()];
    for i in 0..obs.len() {
        world(obs, parents, &mut done, i, 0);
    }
    done.into_iter()
        .map(|m| m.unwrap_or(math::IDENTITY))
        .collect()
}

fn world(
    obs: &[(u64, View)],
    parents: &[Option<usize>],
    done: &mut [Option<Mat4>],
    i: usize,
    depth: usize,
) -> Mat4 {
    if let Some(m) = done[i] {
        return m;
    }
    let ob = &obs[i].1;
    let partype = ob.int("partype").unwrap_or(PAROBJECT) & 0xF;
    let has_constraints = ob.sub("constraints").and_then(|l| l.ptr("first")).is_some();
    let parent = parents[i].filter(|&p| p != i && depth < MAX_DEPTH);
    let special =
        has_constraints || (parent.is_some() && partype != PAROBJECT && partype != PARSKEL);
    let saved = special.then(|| stored_world(ob)).flatten();
    let m = saved.unwrap_or_else(|| {
        let local = local_matrix(ob);
        match parent {
            Some(p) => {
                let pw = world(obs, parents, done, p, depth + 1);
                let inv = ob
                    .floats("parentinv")
                    .as_deref()
                    .and_then(math::from_floats)
                    .unwrap_or(math::IDENTITY);
                math::mul(&math::mul(&pw, &inv), &local)
            }
            None => local,
        }
    });
    done[i] = Some(m);
    m
}

/// The matrix saved with the object (`obmat`, called `object_to_world` in
/// Blender 4.x source but kept under its old name in files).
fn stored_world(ob: &View) -> Option<Mat4> {
    ["obmat", "object_to_world"]
        .iter()
        .find_map(|f| ob.floats(f).as_deref().and_then(math::from_floats))
        .filter(|m| m[3][3] != 0.0)
}

fn vec3(v: &View, field: &str, default: f64) -> [f64; 3] {
    match v.floats(field).as_deref() {
        Some([a, b, c, ..]) => [*a, *b, *c],
        _ => [default; 3],
    }
}

/// The transform relative to the parent, from loc/rot/scale plus their
/// delta counterparts, as Blender's `BKE_object_to_mat4` does.
fn local_matrix(ob: &View) -> Mat4 {
    let loc = vec3(ob, "loc", 0.0);
    let dloc = vec3(ob, "dloc", 0.0);
    let size = vec3(ob, "size", 1.0);
    let dscale = vec3(ob, "dscale", 1.0);
    let rot = match ob.int("rotmode").unwrap_or(1) {
        0 => {
            let q = |f: &str| match ob.floats(f).as_deref() {
                Some([w, x, y, z]) => [*w, *x, *y, *z],
                _ => [1.0, 0.0, 0.0, 0.0],
            };
            math::rot_mul(&math::quat(q("dquat")), &math::quat(q("quat")))
        }
        -1 => {
            let angle = |f: &str| ob.float(f).unwrap_or(0.0);
            let r = math::axis_angle(vec3(ob, "rotAxis", 0.0), angle("rotAngle"));
            let d = math::axis_angle(vec3(ob, "drotAxis", 0.0), angle("drotAngle"));
            math::rot_mul(&d, &r)
        }
        mode => math::rot_mul(
            &math::euler(vec3(ob, "drot", 0.0), mode),
            &math::euler(vec3(ob, "rot", 0.0), mode),
        ),
    };
    let loc = [loc[0] + dloc[0], loc[1] + dloc[1], loc[2] + dloc[2]];
    let scale = [
        size[0] * dscale[0],
        size[1] * dscale[1],
        size[2] * dscale[2],
    ];
    math::compose(loc, &rot, scale)
}

/// Per-slot materials: the object's own slot when its `matbits` entry is
/// set (slot linked to "Object"), otherwise the data's.
fn material_slots(
    ob: &View,
    data: Option<&View>,
    index: &HashMap<u64, usize>,
) -> Vec<Option<usize>> {
    let n = |v: &View| v.int("totcol").unwrap_or(0).clamp(0, 1024) as usize;
    let mats = |v: &View, n: usize| v.ptr("mat").map(|a| v.ptr_array(a, n)).unwrap_or_default();
    let ob_n = n(ob);
    let ob_mats = mats(ob, ob_n);
    let bits = ob
        .ptr("matbits")
        .and_then(|a| ob.bytes_at(a))
        .unwrap_or(&[]);
    let (data_n, data_mats) = match data {
        Some(d) if d.has("mat") && d.has("totcol") => (n(d), mats(d, n(d))),
        _ => (0, Vec::new()),
    };
    (0..ob_n.max(data_n))
        .map(|i| {
            let addr = if bits.get(i).is_some_and(|&b| b != 0) {
                ob_mats.get(i)
            } else {
                data_mats.get(i)
            };
            addr.and_then(|a| index.get(a).copied())
        })
        .collect()
}

/// ID properties as `(name, text)` pairs; groups are flattened with dots.
fn custom_props(id: &View) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(group) = id.deref("properties", "IDProperty") {
        props_into(&group, "", &mut out, 0);
    }
    out
}

/// `eIDPropertyType` values; also part of the file format.
const IDP_STRING: i64 = 0;
const IDP_INT: i64 = 1;
const IDP_FLOAT: i64 = 2;
const IDP_ARRAY: i64 = 5;
const IDP_GROUP: i64 = 6;
const IDP_DOUBLE: i64 = 8;
const IDP_BOOLEAN: i64 = 10;

fn props_into(group: &View, prefix: &str, out: &mut Vec<(String, String)>, depth: usize) {
    let Some(data) = group.sub("data") else {
        return;
    };
    if depth > MAX_DEPTH {
        return;
    }
    for prop in list(&data, "group", "IDProperty") {
        let name = prop.string("name").unwrap_or_default();
        if name == "_RNA_UI" {
            continue;
        }
        let full = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}.{name}")
        };
        let Some(d) = prop.sub("data") else {
            continue;
        };
        let len = prop.int("len").unwrap_or(0).clamp(0, 1 << 20) as usize;
        let value = match prop.int("type").unwrap_or(-1) {
            // `len` counts the terminating NUL.
            IDP_STRING => d
                .ptr("pointer")
                .and_then(|a| d.bytes_at(a))
                .map(|b| c_string(&b[..len.min(b.len())])),
            IDP_INT => d.int("val").map(|v| v.to_string()),
            IDP_BOOLEAN => d.int("val").map(|v| (v != 0).to_string()),
            IDP_FLOAT => d.int("val").map(|v| f32::from_bits(v as u32).to_string()),
            IDP_DOUBLE => double_at(&d, "val").map(|v| v.to_string()),
            IDP_ARRAY => array_prop(&d, prop.int("subtype").unwrap_or(-1), len),
            IDP_GROUP => {
                props_into(&prop, &full, out, depth + 1);
                None
            }
            _ => None,
        };
        if let Some(v) = value {
            out.push((full, v));
        }
    }
}

/// IDP_DOUBLE keeps its 8 bytes in `val` and `val2`, in file byte order.
fn double_at(d: &View, field: &str) -> Option<f64> {
    let off = d.def.field(field)?.offset;
    let b: [u8; 8] = d.bytes.get(off..off + 8)?.try_into().ok()?;
    Some(f64_from(d, b))
}

fn f64_from(v: &View, b: [u8; 8]) -> f64 {
    if v.file.little_endian {
        f64::from_le_bytes(b)
    } else {
        f64::from_be_bytes(b)
    }
}

fn array_prop(d: &View, subtype: i64, len: usize) -> Option<String> {
    let addr = d.ptr("pointer")?;
    let items: Vec<String> = match subtype {
        IDP_INT => d.i32s_at(addr, len)?.iter().map(i32::to_string).collect(),
        IDP_FLOAT => d.f32s_at(addr, len)?.iter().map(f32::to_string).collect(),
        IDP_DOUBLE => d
            .bytes_at(addr)?
            .get(..len.checked_mul(8)?)?
            .chunks_exact(8)
            .map(|c| f64_from(d, c.try_into().unwrap_or_default()).to_string())
            .collect(),
        IDP_BOOLEAN => d
            .bytes_at(addr)?
            .get(..len)?
            .iter()
            .map(|b| (*b != 0).to_string())
            .collect(),
        _ => return None,
    };
    Some(format!("[{}]", items.join(", ")))
}

/// Generic attribute arrays of a mesh by name, from `AttributeStorage`
/// (Blender 5.x) and the CustomData layers (Blender 3.x/4.x).
struct Attributes<'f, 'a> {
    mesh: View<'f, 'a>,
    by_name: HashMap<String, Attr>,
}

#[derive(Clone, Copy)]
enum Attr {
    Array(u64),
    /// One value shared by every element (`AttributeSingle`).
    Single(u64),
}

impl<'f, 'a> Attributes<'f, 'a> {
    fn collect(me: &View<'f, 'a>) -> Self {
        let mut by_name = HashMap::new();
        if let Some(store) = me.sub("attribute_storage") {
            let n = store
                .int("dna_attributes_num")
                .unwrap_or(0)
                .clamp(0, 1 << 16) as usize;
            let attrs = store
                .ptr("dna_attributes")
                .and_then(|a| store.array_at(a, "Attribute", n))
                .unwrap_or_default();
            for attr in attrs {
                let name = attr
                    .ptr("name")
                    .and_then(|a| attr.bytes_at(a))
                    .map(c_string);
                let data = attr.ptr("data").and_then(|a| attr.view_at(a, ""));
                let (Some(name), Some(data)) = (name, data) else {
                    continue;
                };
                let Some(ptr) = data.ptr("data") else {
                    continue;
                };
                let kind = if data.type_name() == "AttributeSingle" {
                    Attr::Single(ptr)
                } else {
                    Attr::Array(ptr)
                };
                by_name.insert(name, kind);
            }
        }
        for cd in ["vdata", "edata", "pdata", "ldata"] {
            for layer in layers(me, cd) {
                let name = layer.string("name").unwrap_or_default();
                if let (false, Some(ptr)) = (name.is_empty(), layer.ptr("data")) {
                    by_name.entry(name).or_insert(Attr::Array(ptr));
                }
            }
        }
        Attributes { mesh: *me, by_name }
    }

    fn f32s(&self, name: &str, n: usize) -> Option<Vec<f32>> {
        match *self.by_name.get(name)? {
            Attr::Array(a) => self.mesh.f32s_at(a, n),
            Attr::Single(_) => None,
        }
    }

    fn i32s(&self, name: &str, n: usize) -> Option<Vec<i32>> {
        match *self.by_name.get(name)? {
            Attr::Array(a) => self.mesh.i32s_at(a, n),
            Attr::Single(a) => Some(vec![*self.mesh.i32s_at(a, 1)?.first()?; n]),
        }
    }
}

fn layers<'f, 'a>(me: &View<'f, 'a>, cd: &str) -> Vec<View<'f, 'a>> {
    let Some(cd) = me.sub(cd) else {
        return Vec::new();
    };
    let n = cd.int("totlayer").unwrap_or(0).clamp(0, 1 << 16) as usize;
    cd.ptr("layers")
        .and_then(|a| cd.array_at(a, "CustomDataLayer", n))
        .unwrap_or_default()
}

/// A legacy struct array (`MVert`, `MPoly`...), via the Mesh pointer field
/// first and the CustomData layer of that type second.
fn legacy_array<'f, 'a>(
    me: &View<'f, 'a>,
    field: &str,
    cd: &str,
    cd_type: i64,
    ty: &str,
    n: usize,
) -> Option<Vec<View<'f, 'a>>> {
    let ptr = me.ptr(field).or_else(|| {
        layers(me, cd)
            .iter()
            .find(|l| l.int("type") == Some(cd_type))
            .and_then(|l| l.ptr("data"))
    })?;
    me.array_at(ptr, ty, n)
}

fn count(me: &View, names: &[&str]) -> usize {
    names
        .iter()
        .find_map(|n| me.int(n))
        .unwrap_or(0)
        .clamp(0, i32::MAX as i64) as usize
}

fn clamp_u16(v: i64) -> u16 {
    v.clamp(0, u16::MAX as i64) as u16
}

/// Reads the base mesh. Newer layouts are tried first: positions as the
/// "position" attribute (3.4+), face offsets (3.6+) and ".corner_vert"
/// (3.6+); then the `MVert`/`MPoly`/`MLoop` structs of older files, and
/// finally `MFace` for files from before 2.63.
fn read_mesh(me: &View) -> BlendMesh {
    let nverts = count(me, &["totvert", "verts_num"]);
    let npolys = count(me, &["totpoly", "faces_num"]);
    let nloops = count(me, &["totloop", "corners_num"]);
    let attrs = Attributes::collect(me);

    let vertices: Vec<[f64; 3]> = match attrs.f32s("position", nverts.saturating_mul(3)) {
        Some(p) => p
            .chunks_exact(3)
            .map(|c| [c[0] as f64, c[1] as f64, c[2] as f64])
            .collect(),
        None => legacy_array(me, "mvert", "vdata", CD_MVERT, "MVert", nverts)
            .unwrap_or_default()
            .iter()
            .map(|v| vec3(v, "co", 0.0))
            .collect(),
    };

    // Faces as (first corner, corner count, material slot).
    let mut polys: Vec<(usize, usize, i64)> = Vec::new();
    // Sized by arrays already read, so a corrupt face count cannot make a
    // broadcast (`AttributeSingle`) material allocate without bound.
    let mats = |n: usize| attrs.i32s("material_index", n).unwrap_or_default();
    let mat = |m: &[i32], i: usize| m.get(i).map(|&m| m as i64);
    let offsets = ["poly_offset_indices", "face_offset_indices"]
        .iter()
        .find_map(|f| me.ptr(f))
        .and_then(|a| me.i32s_at(a, npolys + 1));
    if let Some(off) = offsets {
        let m = mats(off.len() - 1);
        for (i, w) in off.windows(2).enumerate() {
            let (a, b) = (w[0].max(0) as usize, w[1].max(0) as usize);
            polys.push((a, b.saturating_sub(a), mat(&m, i).unwrap_or(0)));
        }
    } else if let Some(mp) = legacy_array(me, "mpoly", "pdata", CD_MPOLY, "MPoly", npolys) {
        let m = mats(mp.len());
        for (i, p) in mp.iter().enumerate() {
            let start = p.int("loopstart").unwrap_or(0).max(0) as usize;
            let len = p.int("totloop").unwrap_or(0).max(0) as usize;
            let slot = mat(&m, i).or_else(|| p.int("mat_nr")).unwrap_or(0);
            polys.push((start, len, slot));
        }
    }

    let mut faces = Vec::new();
    let mut face_material = Vec::new();
    let valid = |f: &[u32]| f.len() >= 3 && f.iter().all(|&v| (v as usize) < vertices.len());
    if !polys.is_empty() {
        let corners: Vec<u32> = attrs
            .i32s(".corner_vert", nloops)
            .map(|c| c.iter().map(|&v| v as u32).collect())
            .or_else(|| {
                let ml = legacy_array(me, "mloop", "ldata", CD_MLOOP, "MLoop", nloops)?;
                Some(ml.iter().map(|l| l.int("v").unwrap_or(-1) as u32).collect())
            })
            .unwrap_or_default();
        for (start, len, m) in polys {
            match corners.get(start..start.saturating_add(len)) {
                Some(face) if valid(face) => {
                    faces.push(face.to_vec());
                    face_material.push(clamp_u16(m));
                }
                _ => {}
            }
        }
    } else {
        let nfaces = count(me, &["totface"]);
        let mf = legacy_array(me, "mface", "fdata", CD_MFACE, "MFace", nfaces).unwrap_or_default();
        for f in mf {
            let v: Vec<u32> = ["v1", "v2", "v3", "v4"]
                .iter()
                .map(|n| f.int(n).unwrap_or(0) as u32)
                .collect();
            // A zero fourth index marks a triangle.
            let face = if v[3] == 0 { v[..3].to_vec() } else { v };
            if valid(&face) {
                faces.push(face);
                face_material.push(clamp_u16(f.int("mat_nr").unwrap_or(0)));
            }
        }
    }

    BlendMesh {
        vertices,
        faces,
        face_material,
    }
}
