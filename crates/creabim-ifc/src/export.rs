use crate::step::{list, point2, point3, real, string, Ref, StepWriter};
use creabim_core::{Element, ElementId, ElementKind, Model, OpeningKind};
use glam::{DVec2, DVec3};
use std::collections::HashMap;

pub struct ExportOptions {
    pub project_name: String,
    pub site_name: String,
    pub building_name: String,
    /// ISO 8601 timestamp for the file header.
    pub timestamp: String,
}

impl Default for ExportOptions {
    fn default() -> Self {
        Self {
            project_name: "CreaBIM Project".into(),
            site_name: "Site".into(),
            building_name: "Building".into(),
            timestamp: "2026-01-01T00:00:00".into(),
        }
    }
}

struct Ctx {
    w: StepWriter,
    body: Ref,
    z_axis: Ref,
    x_axis: Ref,
}

impl Ctx {
    fn guid(&self) -> String {
        string(&ElementId::new().ifc_guid())
    }

    fn dir(&mut self, d: DVec3) -> Ref {
        if d == DVec3::Z {
            return self.z_axis;
        }
        if d == DVec3::X {
            return self.x_axis;
        }
        self.w.add(
            "IFCDIRECTION",
            format!("({},{},{})", real(d.x), real(d.y), real(d.z)),
        )
    }

    fn placement3d(&mut self, at: DVec3, axis: Option<DVec3>, ref_dir: Option<DVec3>) -> Ref {
        let p = self.w.add("IFCCARTESIANPOINT", point3(at.x, at.y, at.z));
        let a = axis.map(|d| self.dir(d).to_string()).unwrap_or("$".into());
        let r = ref_dir
            .map(|d| self.dir(d).to_string())
            .unwrap_or("$".into());
        self.w.add("IFCAXIS2PLACEMENT3D", format!("{p},{a},{r}"))
    }

    fn local_placement(&mut self, rel: Option<Ref>, at: DVec3, ref_dir: Option<DVec3>) -> Ref {
        let axis = ref_dir.map(|_| DVec3::Z);
        let p = self.placement3d(at, axis, ref_dir);
        let rel = rel.map(|r| r.to_string()).unwrap_or("$".into());
        self.w.add("IFCLOCALPLACEMENT", format!("{rel},{p}"))
    }

    fn shape(&mut self, kind: &str, items: &[Ref]) -> Ref {
        let rep = self.w.add(
            "IFCSHAPEREPRESENTATION",
            format!("{},'Body',{},{}", self.body, string(kind), list(items)),
        );
        self.w
            .add("IFCPRODUCTDEFINITIONSHAPE", format!("$,$,({rep})"))
    }

    /// A rectangle `xdim` × `ydim` centred at `center` (profile plane),
    /// extruded `depth` along the solid's local Z.
    fn rect_extrusion(
        &mut self,
        center: DVec2,
        xdim: f64,
        ydim: f64,
        position: Ref,
        depth: f64,
    ) -> Ref {
        let c = self.w.add("IFCCARTESIANPOINT", point2(center.x, center.y));
        let p2 = self.w.add("IFCAXIS2PLACEMENT2D", format!("{c},$"));
        let prof = self.w.add(
            "IFCRECTANGLEPROFILEDEF",
            format!(".AREA.,$,{p2},{},{}", real(xdim), real(ydim)),
        );
        self.w.add(
            "IFCEXTRUDEDAREASOLID",
            format!("{prof},{position},{},{}", self.z_axis, real(depth)),
        )
    }
}

/// Export the model as an IFC4 file.
pub fn export_ifc4(model: &Model, opts: &ExportOptions) -> String {
    let mut w = StepWriter::default();
    let z_axis = w.add("IFCDIRECTION", "(0.,0.,1.)");
    let x_axis = w.add("IFCDIRECTION", "(1.,0.,0.)");
    let origin = w.add("IFCCARTESIANPOINT", "(0.,0.,0.)");
    let wcs = w.add("IFCAXIS2PLACEMENT3D", format!("{origin},$,$"));
    let model_ctx = w.add(
        "IFCGEOMETRICREPRESENTATIONCONTEXT",
        format!("$,'Model',3,1.E-05,{wcs},$"),
    );
    let body = w.add(
        "IFCGEOMETRICREPRESENTATIONSUBCONTEXT",
        format!("'Body','Model',*,*,*,*,{model_ctx},$,.MODEL_VIEW.,$"),
    );
    let mut c = Ctx {
        w,
        body,
        z_axis,
        x_axis,
    };

    let units = [
        c.w.add("IFCSIUNIT", "*,.LENGTHUNIT.,$,.METRE."),
        c.w.add("IFCSIUNIT", "*,.AREAUNIT.,$,.SQUARE_METRE."),
        c.w.add("IFCSIUNIT", "*,.VOLUMEUNIT.,$,.CUBIC_METRE."),
        c.w.add("IFCSIUNIT", "*,.PLANEANGLEUNIT.,$,.RADIAN."),
    ];
    let unit_assignment = c.w.add("IFCUNITASSIGNMENT", list(units));
    let project = c.w.add(
        "IFCPROJECT",
        format!(
            "{},$,{},$,$,$,$,({model_ctx}),{unit_assignment}",
            c.guid(),
            string(&opts.project_name)
        ),
    );

    let site_pl = c.local_placement(None, DVec3::ZERO, None);
    let site = c.w.add(
        "IFCSITE",
        format!(
            "{},$,{},$,$,{site_pl},$,$,.ELEMENT.,$,$,$,$,$",
            c.guid(),
            string(&opts.site_name)
        ),
    );
    let bldg_pl = c.local_placement(Some(site_pl), DVec3::ZERO, None);
    let building = c.w.add(
        "IFCBUILDING",
        format!(
            "{},$,{},$,$,{bldg_pl},$,$,.ELEMENT.,$,$,$",
            c.guid(),
            string(&opts.building_name)
        ),
    );

    // Storeys, one per level.
    let mut storeys: HashMap<ElementId, (Ref, Ref)> = HashMap::new();
    let mut storey_order = Vec::new();
    for (e, l) in model.levels() {
        let pl = c.local_placement(Some(bldg_pl), DVec3::new(0.0, 0.0, l.elevation), None);
        let s = c.w.add(
            "IFCBUILDINGSTOREY",
            format!(
                "{},$,{},$,$,{pl},$,$,.ELEMENT.,{}",
                string(&e.id.ifc_guid()),
                string(&e.name),
                real(l.elevation)
            ),
        );
        storeys.insert(e.id, (s, pl));
        storey_order.push(s);
    }

    let mut contained: HashMap<Option<ElementId>, Vec<Ref>> = HashMap::new();
    let mut wall_refs: HashMap<ElementId, (Ref, Ref)> = HashMap::new(); // entity, placement
    let joins = creabim_core::wall_joins(model);
    let mut by_material: HashMap<ElementId, Vec<Ref>> = HashMap::new();

    for e in model.iter() {
        match &e.kind {
            ElementKind::Wall(wall) => {
                let Some(&(_, spl)) = storeys.get(&wall.level) else {
                    continue;
                };
                let d = wall.direction();
                let pl = c.local_placement(
                    Some(spl),
                    DVec3::new(wall.start.x, wall.start.y, wall.base_offset),
                    Some(DVec3::new(d.x, d.y, 0.0)),
                );
                // footprint in wall-local coordinates, following its joins
                let ends = joins.get(&e.id).copied().unwrap_or_default();
                let t = wall.thickness / 2.0;
                let local = [
                    DVec2::new(ends.start_u(1.0), t),
                    DVec2::new(ends.end_u(wall, 1.0), t),
                    DVec2::new(ends.end_u(wall, -1.0), -t),
                    DVec2::new(ends.start_u(-1.0), -t),
                ];
                let mut pts: Vec<Ref> = local
                    .iter()
                    .map(|p| c.w.add("IFCCARTESIANPOINT", point2(p.x, p.y)))
                    .collect();
                pts.push(pts[0]);
                let poly = c.w.add("IFCPOLYLINE", list(&pts));
                let prof =
                    c.w.add("IFCARBITRARYCLOSEDPROFILEDEF", format!(".AREA.,$,{poly}"));
                let pos = c.placement3d(DVec3::ZERO, None, None);
                let solid = c.w.add(
                    "IFCEXTRUDEDAREASOLID",
                    format!("{prof},{pos},{},{}", c.z_axis, real(wall.height)),
                );
                let shape = c.shape("SweptSolid", &[solid]);
                let r = product(&mut c, "IFCWALL", e, pl, shape, ",$,.STANDARD.");
                wall_refs.insert(e.id, (r, pl));
                contained.entry(Some(wall.level)).or_default().push(r);
                if let Some((m, _)) = creabim_core::material_of(model, e) {
                    by_material.entry(m.id).or_default().push(r);
                }
            }
            ElementKind::Slab(slab) => {
                let Some(&(_, spl)) = storeys.get(&slab.level) else {
                    continue;
                };
                let pl = c.local_placement(Some(spl), DVec3::ZERO, None);
                let mut pts: Vec<Ref> = slab
                    .boundary
                    .iter()
                    .map(|p| c.w.add("IFCCARTESIANPOINT", point2(p.x, p.y)))
                    .collect();
                pts.push(pts[0]);
                let poly = c.w.add("IFCPOLYLINE", list(&pts));
                let prof =
                    c.w.add("IFCARBITRARYCLOSEDPROFILEDEF", format!(".AREA.,$,{poly}"));
                let pos = c.placement3d(DVec3::new(0.0, 0.0, -slab.thickness), None, None);
                let solid = c.w.add(
                    "IFCEXTRUDEDAREASOLID",
                    format!("{prof},{pos},{},{}", c.z_axis, real(slab.thickness)),
                );
                let shape = c.shape("SweptSolid", &[solid]);
                let r = product(&mut c, "IFCSLAB", e, pl, shape, ",$,.FLOOR.");
                contained.entry(Some(slab.level)).or_default().push(r);
                if let Some((m, _)) = creabim_core::material_of(model, e) {
                    by_material.entry(m.id).or_default().push(r);
                }
            }
            ElementKind::FreeMesh(f) => {
                let spl = f
                    .level
                    .and_then(|l| storeys.get(&l))
                    .map(|s| s.1)
                    .unwrap_or(bldg_pl);
                let pl = c.local_placement(Some(spl), f.origin, None);
                // the shape as shown, with its modifiers applied
                let mesh = f.evaluated().mesh;
                let coords: Vec<String> = mesh
                    .vertices
                    .iter()
                    .map(|v| point3(v.x, v.y, v.z))
                    .collect();
                let pts =
                    c.w.add("IFCCARTESIANPOINTLIST3D", format!("({})", coords.join(",")));
                let mut tris = Vec::new();
                for face in 0..mesh.faces.len() {
                    for [a, b, d] in mesh.face_triangles(face) {
                        tris.push(format!("({},{},{})", a + 1, b + 1, d + 1));
                    }
                }
                let fs = c.w.add(
                    "IFCTRIANGULATEDFACESET",
                    format!("{pts},$,.T.,({}),$", tris.join(",")),
                );
                let shape = c.shape("Tessellation", &[fs]);
                let class = free_mesh_entity(&f.ifc_class);
                let tail = ",$,.NOTDEFINED.";
                let r = product(&mut c, class, e, pl, shape, tail);
                contained.entry(f.level).or_default().push(r);
                if let Some((m, _)) = creabim_core::material_of(model, e) {
                    by_material.entry(m.id).or_default().push(r);
                }
            }
            _ => {}
        }
    }

    // Openings need their host wall exported first, so they get a second pass.
    for e in model.iter() {
        let ElementKind::Opening(o) = &e.kind else {
            continue;
        };
        let Some(&(wall_r, wall_pl)) = wall_refs.get(&o.host) else {
            continue;
        };
        let Some(ElementKind::Wall(wall)) = model.get(o.host).map(|h| &h.kind) else {
            continue;
        };
        let eps = 0.05;
        let pl = c.local_placement(Some(wall_pl), DVec3::new(o.offset, 0.0, o.sill), None);
        let pos = c.placement3d(
            DVec3::new(o.width / 2.0, wall.thickness / 2.0 + eps, o.height / 2.0),
            Some(-DVec3::Y),
            Some(DVec3::X),
        );
        let solid = c.rect_extrusion(
            DVec2::ZERO,
            o.width,
            o.height,
            pos,
            wall.thickness + 2.0 * eps,
        );
        let shape = c.shape("SweptSolid", &[solid]);
        let guid = c.guid();
        let opening = c.w.add(
            "IFCOPENINGELEMENT",
            format!(
                "{guid},$,{},$,$,{pl},{shape},$,.OPENING.",
                string(&format!("{} opening", e.name))
            ),
        );
        let g = c.guid();
        c.w.add(
            "IFCRELVOIDSELEMENT",
            format!("{g},$,$,$,{wall_r},{opening}"),
        );

        let fill_pl = c.local_placement(Some(pl), DVec3::ZERO, None);
        let panel_pos = c.placement3d(
            DVec3::new(o.width / 2.0, 0.025, o.height / 2.0),
            Some(-DVec3::Y),
            Some(DVec3::X),
        );
        let panel = c.rect_extrusion(DVec2::ZERO, o.width, o.height, panel_pos, 0.05);
        let fill_shape = c.shape("SweptSolid", &[panel]);
        let (class, tail) = match o.kind {
            OpeningKind::Door => (
                "IFCDOOR",
                format!(
                    ",$,{},{},.DOOR.,.SINGLE_SWING_LEFT.,$",
                    real(o.height),
                    real(o.width)
                ),
            ),
            OpeningKind::Window => (
                "IFCWINDOW",
                format!(
                    ",$,{},{},.WINDOW.,.SINGLE_PANEL.,$",
                    real(o.height),
                    real(o.width)
                ),
            ),
        };
        let fill = product(&mut c, class, e, fill_pl, fill_shape, &tail);
        let g = c.guid();
        c.w.add("IFCRELFILLSELEMENT", format!("{g},$,$,$,{opening},{fill}"));
        let level = match model.get(o.host).map(|h| &h.kind) {
            Some(ElementKind::Wall(w)) => w.level,
            _ => continue,
        };
        contained.entry(Some(level)).or_default().push(fill);
    }

    // Materials, each associated with the elements made of it.
    for e in model.iter() {
        let Some(users) = by_material.get(&e.id) else {
            continue;
        };
        let m = c.w.add("IFCMATERIAL", format!("{},$,$", string(&e.name)));
        let g = c.guid();
        c.w.add(
            "IFCRELASSOCIATESMATERIAL",
            format!("{g},$,$,$,{},{m}", list(users)),
        );
    }

    // Spatial structure.
    let g = c.guid();
    c.w.add("IFCRELAGGREGATES", format!("{g},$,$,$,{project},({site})"));
    let g = c.guid();
    c.w.add("IFCRELAGGREGATES", format!("{g},$,$,$,{site},({building})"));
    if !storey_order.is_empty() {
        let g = c.guid();
        c.w.add(
            "IFCRELAGGREGATES",
            format!("{g},$,$,$,{building},{}", list(&storey_order)),
        );
    }
    let mut keys: Vec<_> = contained.keys().copied().collect();
    keys.sort_by_key(|k| {
        k.map(|id| storeys.get(&id).map(|s| s.0 .0))
            .unwrap_or(Some(0))
    });
    for k in keys {
        let items = &contained[&k];
        let container = k
            .and_then(|id| storeys.get(&id))
            .map(|s| s.0)
            .unwrap_or(building);
        let g = c.guid();
        c.w.add(
            "IFCRELCONTAINEDINSPATIALSTRUCTURE",
            format!("{g},$,$,$,{},{container}", list(items)),
        );
    }

    let header = format!(
        "FILE_DESCRIPTION(('ViewDefinition [DesignTransferView]'),'2;1');\n\
         FILE_NAME({},{},(''),(''),{},{},'');\n\
         FILE_SCHEMA(('IFC4'));\n",
        string(&format!("{}.ifc", opts.project_name)),
        string(&opts.timestamp),
        string(concat!("CreaBIM ", env!("CARGO_PKG_VERSION"))),
        string("CreaBIM"),
    );
    c.w.finish(&header)
}

fn product(c: &mut Ctx, class: &str, e: &Element, pl: Ref, shape: Ref, tail: &str) -> Ref {
    c.w.add(
        class,
        format!(
            "{},$,{},$,$,{pl},{shape}{tail}",
            string(&e.id.ifc_guid()),
            string(&e.name)
        ),
    )
}

fn free_mesh_entity(class: &str) -> &'static str {
    match class {
        "IfcWall" => "IFCWALL",
        "IfcSlab" => "IFCSLAB",
        "IfcRoof" => "IFCROOF",
        "IfcColumn" => "IFCCOLUMN",
        "IfcBeam" => "IFCBEAM",
        "IfcCovering" => "IFCCOVERING",
        "IfcFurniture" => "IFCFURNITURE",
        _ => "IFCBUILDINGELEMENTPROXY",
    }
}
