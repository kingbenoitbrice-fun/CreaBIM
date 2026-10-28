use crate::params::{curtain_props, type_props, Prop, CURTAIN_PSET, TYPE_PSET};
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

/// Marks the hatch in an exported material's description, so CreaBIM
/// gets it back on import.
pub(crate) const HATCH_PREFIX: &str = "CreaBIM hatch: ";

struct Ctx {
    w: StepWriter,
    body: Ref,
    /// Plan symbols of furniture and fixtures.
    footprint: Ref,
    axis: Ref,
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
        let rep = self.body_rep(kind, items);
        self.w
            .add("IFCPRODUCTDEFINITIONSHAPE", format!("$,$,({rep})"))
    }

    fn body_rep(&mut self, kind: &str, items: &[Ref]) -> Ref {
        self.w.add(
            "IFCSHAPEREPRESENTATION",
            format!("{},'Body',{},{}", self.body, string(kind), list(items)),
        )
    }

    /// A property set of single values.
    fn pset(&mut self, name: &str, props: &[(&'static str, Prop)]) -> Ref {
        let refs: Vec<Ref> = props
            .iter()
            .map(|(k, v)| {
                self.w.add(
                    "IFCPROPERTYSINGLEVALUE",
                    format!("{},$,{},$", string(k), v.step()),
                )
            })
            .collect();
        let g = self.guid();
        self.w.add(
            "IFCPROPERTYSET",
            format!("{g},$,{},$,{}", string(name), list(&refs)),
        )
    }

    /// A property set, given to `objects`.
    fn pset_on(&mut self, objects: &[Ref], name: &str, props: &[(&'static str, Prop)]) {
        let set = self.pset(name, props);
        let g = self.guid();
        self.w.add(
            "IFCRELDEFINESBYPROPERTIES",
            format!("{g},$,$,$,{},{set}", list(objects)),
        );
    }

    /// A polygon in plan extruded `depth` up from height `z`.
    fn plan_extrusion(&mut self, pts: &[DVec2], z: f64, depth: f64) -> Ref {
        let mut refs: Vec<Ref> = pts
            .iter()
            .map(|p| self.w.add("IFCCARTESIANPOINT", point2(p.x, p.y)))
            .collect();
        refs.push(refs[0]);
        let poly = self.w.add("IFCPOLYLINE", list(&refs));
        let prof = self
            .w
            .add("IFCARBITRARYCLOSEDPROFILEDEF", format!(".AREA.,$,{poly}"));
        let pos = self.placement3d(DVec3::new(0.0, 0.0, z), None, None);
        self.w.add(
            "IFCEXTRUDEDAREASOLID",
            format!("{prof},{pos},{},{}", self.z_axis, real(depth)),
        )
    }

    /// An IfcOpeningElement of shape `solid` (in `host_pl`'s frame) voiding `host`.
    fn void(&mut self, host: Ref, host_pl: Ref, solid: Ref, name: &str, guid: Option<String>) {
        let pl = self.local_placement(Some(host_pl), DVec3::ZERO, None);
        let shape = self.shape("SweptSolid", &[solid]);
        let guid = guid.unwrap_or_else(|| self.guid());
        let opening = self.w.add(
            "IFCOPENINGELEMENT",
            format!("{guid},$,{},$,$,{pl},{shape},$,.OPENING.", string(name)),
        );
        let g = self.guid();
        self.w
            .add("IFCRELVOIDSELEMENT", format!("{g},$,$,$,{host},{opening}"));
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
    let axis = w.add(
        "IFCGEOMETRICREPRESENTATIONSUBCONTEXT",
        format!("'Axis','Model',*,*,*,*,{model_ctx},$,.GRAPH_VIEW.,$"),
    );
    let footprint = w.add(
        "IFCGEOMETRICREPRESENTATIONSUBCONTEXT",
        format!("'FootPrint','Model',*,*,*,*,{model_ctx},$,.PLAN_VIEW.,$"),
    );
    let mut c = Ctx {
        w,
        body,
        footprint,
        axis,
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
                                                                        // how far each wall's axis (its location line) is from its centreline
    let mut wall_axes: HashMap<ElementId, f64> = HashMap::new();
    let joins = creabim_core::wall_joins(model);
    let mut by_material: HashMap<ElementId, Vec<Ref>> = HashMap::new();
    // layered elements, associated with their assembly's layer set instead
    let mut layered: Vec<(Ref, ElementId, LayerUse)> = Vec::new();
    // boolean cuts: cutters are construction aids and stay out; what they
    // cut is written as its final shape, tessellated
    let cuts = creabim_geom::scene::cuts(model);
    let cutters: std::collections::HashSet<ElementId> = cuts.iter().map(|c| c.1).collect();
    let cut_meshes: HashMap<ElementId, creabim_geom::mesh::TriMesh> = if cuts.is_empty() {
        HashMap::new()
    } else {
        let targets: std::collections::HashSet<ElementId> = cuts.iter().map(|c| c.0).collect();
        let mut all = creabim_geom::scene::element_meshes(&creabim_geom::scene::build_scene(model));
        all.retain(|id, _| targets.contains(id));
        all
    };
    let elevation =
        |l: Option<ElementId>| l.and_then(|l| model.level(l)).map_or(0.0, |l| l.elevation);
    // the products of each element type
    let mut typed: HashMap<ElementId, Vec<Ref>> = HashMap::new();
    let mut of_type = |e: &Element, r: Ref| {
        if let Some(t) = &e.of_type {
            typed.entry(t.id).or_default().push(r);
        }
    };

    for e in model.iter() {
        if cutters.contains(&e.id) {
            continue;
        }
        let cut = cut_meshes.get(&e.id);
        match &e.kind {
            ElementKind::Wall(
                wall @ creabim_core::Wall {
                    curtain: Some(grid),
                    ..
                },
            ) => {
                // a curtain wall: no body of its own, aggregating its
                // mullions (IfcMember) and panels (IfcPlate)
                let Some(&(_, spl)) = storeys.get(&wall.level) else {
                    continue;
                };
                let d = wall.direction();
                let pl = c.local_placement(
                    Some(spl),
                    DVec3::new(wall.start.x, wall.start.y, wall.base_offset),
                    Some(DVec3::new(d.x, d.y, 0.0)),
                );
                let r = c.w.add(
                    "IFCCURTAINWALL",
                    format!(
                        "{},$,{},$,$,{pl},$,$,.NOTDEFINED.",
                        string(&e.id.ifc_guid()),
                        string(&e.name)
                    ),
                );
                wall_refs.insert(e.id, (r, pl));
                wall_axes.insert(e.id, 0.0);
                contained.entry(Some(wall.level)).or_default().push(r);
                of_type(e, r);
                let openings: Vec<&creabim_core::Opening> = model
                    .dependents(e.id)
                    .into_iter()
                    .filter_map(|o| match &model.get(o)?.kind {
                        ElementKind::Opening(o) => Some(o),
                        _ => None,
                    })
                    .collect();
                let mut parts = vec![];
                let mut solid_parts = vec![];
                for b in creabim_geom::curtain::curtain_boxes(wall, grid, &openings) {
                    use creabim_geom::curtain::CurtainPart;
                    let size = b.hi - b.lo;
                    let ppl = c.local_placement(Some(pl), b.lo, None);
                    let pos = c.placement3d(DVec3::ZERO, None, None);
                    let solid =
                        c.rect_extrusion(size.truncate() / 2.0, size.x, size.y, pos, size.z);
                    let shape = c.shape("SweptSolid", &[solid]);
                    let (class, tail, what) = match b.part {
                        CurtainPart::Mullion => ("IFCMEMBER", ",$,.MULLION.", "mullion"),
                        CurtainPart::Glass => ("IFCPLATE", ",$,.CURTAIN_PANEL.", "glass"),
                        CurtainPart::Panel => ("IFCPLATE", ",$,.CURTAIN_PANEL.", "panel"),
                    };
                    let g = c.guid();
                    let p = c.w.add(
                        class,
                        format!(
                            "{g},$,{},$,$,{ppl},{shape}{tail}",
                            string(&format!("{} {what}", e.name))
                        ),
                    );
                    parts.push(p);
                    if b.part != CurtainPart::Glass {
                        solid_parts.push(p);
                    }
                }
                if !parts.is_empty() {
                    let g = c.guid();
                    c.w.add(
                        "IFCRELAGGREGATES",
                        format!("{g},$,$,$,{r},{}", list(&parts)),
                    );
                }
                if let Some((m, _)) = creabim_core::material_of(model, e) {
                    by_material.entry(m.id).or_default().extend(solid_parts);
                }
                let mut props = curtain_props(grid);
                props.push(("Length", Prop::Length(wall.length())));
                props.push(("Height", Prop::Length(wall.height)));
                c.pset_on(&[r], CURTAIN_PSET, &props);
            }
            ElementKind::Wall(wall) => {
                let Some(&(_, spl)) = storeys.get(&wall.level) else {
                    continue;
                };
                let d = wall.direction();
                let n = d.perp();
                // the wall's axis is its location line: its placement sits
                // on it and the footprint is measured from it
                let loc = creabim_core::walls::location_offset(model, wall);
                let origin = wall.start + n * loc;
                let pl = c.local_placement(
                    Some(spl),
                    DVec3::new(origin.x, origin.y, wall.base_offset),
                    Some(DVec3::new(d.x, d.y, 0.0)),
                );
                wall_axes.insert(e.id, loc);
                // footprint in wall-local coordinates, following its joins,
                // one piece between each pair of walls crossing it
                let ends = joins.get(&e.id).cloned().unwrap_or_default();
                let t = wall.thickness / 2.0;
                let asm = creabim_core::assembly_of(model, e);
                // a layered wall's core may run on past its faces at a T
                let core = asm.map_or((-t, t), |(_, a)| creabim_core::assembly::wall_core(a, wall));
                // an attached top is cut by the undersides over it
                let top = creabim_core::walls::top_profile(model, wall);
                let z0 = elevation(Some(wall.level)) + wall.base_offset;
                let depth = top.as_ref().map_or(wall.height, |tp| {
                    let steps = 16;
                    (0..=steps)
                        .flat_map(|i| {
                            let p = wall.start.lerp(wall.end, i as f64 / steps as f64);
                            [p + n * t, p - n * t]
                        })
                        .filter_map(|p| tp.z_at(p))
                        .map(|z| z - z0)
                        .fold(wall.height, f64::max)
                });
                let mut solids = vec![];
                for local in ends.local_outlines(wall, core) {
                    let mut pts: Vec<Ref> = local
                        .iter()
                        .map(|p| c.w.add("IFCCARTESIANPOINT", point2(p.x, p.y - loc)))
                        .collect();
                    pts.push(pts[0]);
                    let poly = c.w.add("IFCPOLYLINE", list(&pts));
                    let prof =
                        c.w.add("IFCARBITRARYCLOSEDPROFILEDEF", format!(".AREA.,$,{poly}"));
                    let pos = c.placement3d(DVec3::ZERO, None, None);
                    let mut solid = c.w.add(
                        "IFCEXTRUDEDAREASOLID",
                        format!("{prof},{pos},{},{}", c.z_axis, real(depth)),
                    );
                    if let Some(tp) = &top {
                        // wall-local: x along the wall from its axis start
                        let to_local = |p: DVec2| {
                            let q = p - origin;
                            DVec2::new(q.dot(d), q.dot(n))
                        };
                        for f in &tp.faces {
                            solid = clip_under(&mut c, solid, f, to_local, d, n, origin, z0);
                        }
                    }
                    solids.push(solid);
                }
                let clipped = top.is_some();
                // the axis lets other tools (and our importer) recover the
                // location line whatever the joins did to the ends
                let a0 = c.w.add("IFCCARTESIANPOINT", point2(0.0, 0.0));
                let a1 = c.w.add("IFCCARTESIANPOINT", point2(wall.length(), 0.0));
                let axis_line = c.w.add("IFCPOLYLINE", format!("({a0},{a1})"));
                let axis_rep = c.w.add(
                    "IFCSHAPEREPRESENTATION",
                    format!("{},'Axis','Curve2D',({axis_line})", c.axis),
                );
                let body_rep = match cut {
                    Some(m) => {
                        let fs = tessellate(&mut c, m, |p| {
                            let q = p.truncate() - origin;
                            DVec3::new(q.dot(d), q.dot(n), p.z - z0)
                        });
                        c.body_rep("Tessellation", &[fs])
                    }
                    None if clipped => c.body_rep("Clipping", &solids),
                    None => c.body_rep("SweptSolid", &solids),
                };
                let shape = c.w.add(
                    "IFCPRODUCTDEFINITIONSHAPE",
                    format!("$,$,({body_rep},{axis_rep})"),
                );
                let r = product(&mut c, "IFCWALL", e, pl, shape, ",$,.STANDARD.");
                of_type(e, r);
                wall_refs.insert(e.id, (r, pl));
                contained.entry(Some(wall.level)).or_default().push(r);
                if let Some((a, _)) = asm {
                    // layers run across the wall from its exterior face,
                    // measured from the axis
                    let (sense, offset) = if wall.flip {
                        (false, t - loc)
                    } else {
                        (true, -t - loc)
                    };
                    layered.push((r, a.id, LayerUse::Usage("AXIS2", sense, offset)));
                } else if let Some((m, _)) = creabim_core::material_of(model, e) {
                    by_material.entry(m.id).or_default().push(r);
                }
            }
            ElementKind::Slab(slab) => {
                let Some(&(_, spl)) = storeys.get(&slab.level) else {
                    continue;
                };
                let pl = c.local_placement(Some(spl), DVec3::ZERO, None);
                // arcs go out in short straight pieces
                let mut pts: Vec<Ref> = slab
                    .outline()
                    .iter()
                    .map(|p| c.w.add("IFCCARTESIANPOINT", point2(p.x, p.y)))
                    .collect();
                pts.push(pts[0]);
                let poly = c.w.add("IFCPOLYLINE", list(&pts));
                let prof =
                    c.w.add("IFCARBITRARYCLOSEDPROFILEDEF", format!(".AREA.,$,{poly}"));
                let pos = c.placement3d(
                    DVec3::new(0.0, 0.0, slab.top_offset - slab.thickness),
                    None,
                    None,
                );
                let solid = c.w.add(
                    "IFCEXTRUDEDAREASOLID",
                    format!("{prof},{pos},{},{}", c.z_axis, real(slab.thickness)),
                );
                let shape = match cut {
                    Some(m) => {
                        let z0 = elevation(Some(slab.level));
                        let fs = tessellate(&mut c, m, |p| p - DVec3::Z * z0);
                        c.shape("Tessellation", &[fs])
                    }
                    None => c.shape("SweptSolid", &[solid]),
                };
                let r = product(&mut c, "IFCSLAB", e, pl, shape, ",$,.FLOOR.");
                of_type(e, r);
                contained.entry(Some(slab.level)).or_default().push(r);
                // holes, and shafts passing through, as openings voiding it
                for (i, h) in creabim_core::slab::holes_of(model, e).iter().enumerate() {
                    let z = slab.top_offset - slab.thickness - 0.05;
                    let solid = c.plan_extrusion(h, z, slab.thickness + 0.1);
                    c.void(r, pl, solid, &format!("{} hole {}", e.name, i + 1), None);
                }
                if let Some((a, _)) = creabim_core::assembly_of(model, e) {
                    // layers run down from the top
                    layered.push((r, a.id, LayerUse::Usage("AXIS3", false, slab.top_offset)));
                } else if let Some((m, _)) = creabim_core::material_of(model, e) {
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
                let fs = match cut {
                    Some(m) => {
                        let at = f.origin + DVec3::Z * elevation(f.level);
                        tessellate(&mut c, m, |p| p - at)
                    }
                    None => c.w.add(
                        "IFCTRIANGULATEDFACESET",
                        format!("{pts},$,.T.,({}),$", tris.join(",")),
                    ),
                };
                let shape = c.shape("Tessellation", &[fs]);
                let (class, tail) = free_mesh_entity(&f.ifc_class);
                let r = product(&mut c, &class, e, pl, shape, &tail);
                contained.entry(f.level).or_default().push(r);
                if let Some((m, _)) = creabim_core::material_of(model, e) {
                    by_material.entry(m.id).or_default().push(r);
                }
            }
            k if creabim_core::members::is_member(k) => {
                let level = creabim_core::members::level_of(k);
                let Some(&(_, spl)) = level.and_then(|l| storeys.get(&l)) else {
                    continue;
                };
                // the shape as built, tessellated in the storey's frame
                let mesh = match cut {
                    Some(m) => Some(m.clone()),
                    None => creabim_geom::members::member_mesh(model, e).map(|m| m.0),
                };
                let Some(mesh) = mesh else { continue };
                let pl = c.local_placement(Some(spl), DVec3::ZERO, None);
                let z0 = elevation(level);
                let fs = tessellate(&mut c, &mesh, |p| p - DVec3::Z * z0);
                let shape = c.shape("Tessellation", &[fs]);
                let (class, tail) = member_entity(k);
                let r = product(&mut c, class, e, pl, shape, tail);
                of_type(e, r);
                contained.entry(level).or_default().push(r);
                if let ElementKind::Roof(roof) = k {
                    // the mesh has its holes already; say so for other tools
                    let vt = roof.vertical_thickness();
                    let z = roof.base_offset - vt - 0.1;
                    for (i, h) in creabim_core::slab::holes_of(model, e).iter().enumerate() {
                        let solid = c.plan_extrusion(h, z, roof.peak() + vt + 0.2);
                        c.void(r, pl, solid, &format!("{} hole {}", e.name, i + 1), None);
                    }
                }
                if let Some((a, _)) = creabim_core::assembly_of(model, e) {
                    // a roof's shape isn't a straight extrusion: the set itself
                    layered.push((r, a.id, LayerUse::Set));
                } else if let Some((m, _)) = creabim_core::material_of(model, e) {
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
        // on the wall's centreline, wherever its axis is
        let loc = wall_axes.get(&o.host).copied().unwrap_or(0.0);
        let (pl, shape) = opening_body(&mut c, wall_pl, loc, wall, o);
        // a bare hole is the opening itself, under its own id
        let hole = o.kind == OpeningKind::Hole;
        let (guid, name) = if hole {
            (string(&e.id.ifc_guid()), e.name.clone())
        } else {
            (c.guid(), format!("{} opening", e.name))
        };
        let opening = c.w.add(
            "IFCOPENINGELEMENT",
            format!("{guid},$,{},$,$,{pl},{shape},$,.OPENING.", string(&name)),
        );
        let g = c.guid();
        c.w.add(
            "IFCRELVOIDSELEMENT",
            format!("{g},$,$,$,{wall_r},{opening}"),
        );
        if hole {
            continue;
        }

        // the door or window itself: its frame, glass, leaf and handle,
        // each part a box, placed where the opening starts in the wall. A
        // door's own y axis points the way it swings, as IFC has it, so a
        // door opening to the wall's right is placed turned round.
        let cut = creabim_geom::wall::Cut::of(o, wall);
        let start = cut.map_or(DVec3::ZERO, |cut| {
            DVec3::new(cut.u0 - o.offset, 0.0, cut.v0 - o.sill)
        });
        let width = cut.map_or(o.width, |cut| cut.u1 - cut.u0);
        let turned = o.kind == OpeningKind::Door && !o.faces_left(wall);
        let fill_pl = if turned {
            c.local_placement(Some(pl), start + DVec3::X * width, Some(-DVec3::X))
        } else {
            c.local_placement(Some(pl), start, None)
        };
        let mut parts = Vec::new();
        for b in creabim_geom::opening::opening_boxes(o, wall) {
            let size = b.hi - b.lo;
            let lo = if turned {
                DVec3::new(width - b.hi.x, -b.hi.y, b.lo.z)
            } else {
                b.lo
            };
            let pos = c.placement3d(lo, None, None);
            parts.push(c.rect_extrusion(size.truncate() / 2.0, size.x, size.y, pos, size.z));
        }
        if parts.is_empty() {
            continue;
        }
        let fill_shape = c.shape("SweptSolid", &parts);
        // hinged on the left seen along its y axis (the way it swings)
        let hinge_left = !o.flip_hand != turned;
        let (class, tail) = match o.kind {
            OpeningKind::Door => (
                "IFCDOOR",
                format!(
                    ",$,{},{},.DOOR.,{},$",
                    real(o.height),
                    real(o.width),
                    if hinge_left {
                        ".SINGLE_SWING_LEFT."
                    } else {
                        ".SINGLE_SWING_RIGHT."
                    }
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
            OpeningKind::Hole => continue,
        };
        let fill = product(&mut c, class, e, fill_pl, fill_shape, &tail);
        of_type(e, fill);
        let g = c.guid();
        c.w.add("IFCRELFILLSELEMENT", format!("{g},$,$,$,{opening},{fill}"));
        if let Some((m, _)) = creabim_core::material_of(model, e) {
            by_material.entry(m.id).or_default().push(fill);
        }
        let level = match model.get(o.host).map(|h| &h.kind) {
            Some(ElementKind::Wall(w)) => w.level,
            _ => continue,
        };
        contained.entry(Some(level)).or_default().push(fill);
    }

    // A curtain wall set in a wall cuts its own opening there and fills it,
    // as a door does.
    for e in model.iter() {
        let ElementKind::Wall(wall) = &e.kind else {
            continue;
        };
        for (cw, o) in creabim_core::curtain::voids_in(model, e.id) {
            let (Some(&(wall_r, wall_pl)), Some(&(cw_r, _))) =
                (wall_refs.get(&e.id), wall_refs.get(&cw))
            else {
                continue;
            };
            let loc = wall_axes.get(&e.id).copied().unwrap_or(0.0);
            let (pl, shape) = opening_body(&mut c, wall_pl, loc, wall, &o);
            let name = model.get(cw).map_or("Curtain wall", |x| x.name.as_str());
            let opening = c.w.add(
                "IFCOPENINGELEMENT",
                format!(
                    "{},$,{},$,$,{pl},{shape},$,.OPENING.",
                    c.guid(),
                    string(&format!("{name} opening"))
                ),
            );
            let g = c.guid();
            c.w.add(
                "IFCRELVOIDSELEMENT",
                format!("{g},$,$,$,{wall_r},{opening}"),
            );
            let g = c.guid();
            c.w.add("IFCRELFILLSELEMENT", format!("{g},$,$,$,{opening},{cw_r}"));
        }
    }

    // Materials, each associated with the elements made of it. Unused ones
    // are written too, so the project's material library goes along.
    let mut material_refs: HashMap<ElementId, Ref> = HashMap::new();
    // each material's surface style, for styling the parts of objects
    let mut material_styles: HashMap<ElementId, Ref> = HashMap::new();
    for e in model.iter() {
        let users = by_material.get(&e.id);
        let ElementKind::Material(mat) = &e.kind else {
            continue;
        };
        let mut description = format!("{HATCH_PREFIX}{}", mat.hatch.label());
        if !mat.description.trim().is_empty() {
            description = format!("{description}. {}", mat.description.trim());
        }
        let category = match mat.category.trim() {
            "" => "$".to_string(),
            c => string(c),
        };
        let m = c.w.add(
            "IFCMATERIAL",
            format!("{},{},{category}", string(&e.name), string(&description)),
        );
        material_refs.insert(e.id, m);
        // the look, as a surface style on the material: colour,
        // transparency, roughness and whether it is a metal
        let [r, g, b] = mat.color.map(|v| real(v as f64));
        let rgb = c.w.add("IFCCOLOURRGB", format!("$,{r},{g},{b}"));
        let method = if mat.metallic >= 0.5 {
            ".METAL."
        } else if mat.category == "glass" {
            ".GLASS."
        } else {
            ".NOTDEFINED."
        };
        let rendering = c.w.add(
            "IFCSURFACESTYLERENDERING",
            format!(
                "{rgb},{},$,$,$,$,$,IFCSPECULARROUGHNESS({}),{method}",
                real((1.0 - mat.opacity as f64).clamp(0.0, 1.0)),
                real(mat.roughness as f64)
            ),
        );
        let style = c.w.add(
            "IFCSURFACESTYLE",
            format!("{},.BOTH.,({rendering})", string(&e.name)),
        );
        material_styles.insert(e.id, style);
        let styled = c.w.add("IFCSTYLEDITEM", format!("$,({style}),$"));
        let styled_rep = c.w.add(
            "IFCSTYLEDREPRESENTATION",
            format!("{},'Style','Material',({styled})", c.body),
        );
        c.w.add(
            "IFCMATERIALDEFINITIONREPRESENTATION",
            format!("$,$,({styled_rep}),{m}"),
        );
        if let Some(users) = users.filter(|u| !u.is_empty()) {
            let g = c.guid();
            c.w.add(
                "IFCRELASSOCIATESMATERIAL",
                format!("{g},$,$,$,{},{m}", list(users)),
            );
        }
    }

    // Assemblies as layer sets, unused ones too; each layered element gets
    // a usage saying how the set lies in it (walls across, slabs down).
    let mut layer_sets: HashMap<ElementId, Ref> = HashMap::new();
    for e in model.iter() {
        let ElementKind::Assembly(a) = &e.kind else {
            continue;
        };
        if a.layers.is_empty() {
            continue;
        }
        let layers: Vec<Ref> = a
            .layers
            .iter()
            .map(|l| {
                let m = l
                    .material
                    .and_then(|m| material_refs.get(&m))
                    .map_or("$".to_string(), |r| r.to_string());
                let air = l.function == creabim_core::LayerFunction::AirGap;
                c.w.add(
                    "IFCMATERIALLAYER",
                    format!(
                        "{m},{},{},{},$,{},$",
                        real(l.thickness),
                        if air { ".T." } else { ".F." },
                        string(l.function.label()),
                        string(l.function.ifc_category())
                    ),
                )
            })
            .collect();
        let description = match a.description.trim() {
            "" => "$".to_string(),
            d => string(d),
        };
        let set = c.w.add(
            "IFCMATERIALLAYERSET",
            format!("{},{},{description}", list(&layers), string(&e.name)),
        );
        layer_sets.insert(e.id, set);
    }
    for (product, assembly, how) in &layered {
        let Some(&set) = layer_sets.get(assembly) else {
            continue;
        };
        let relating = match how {
            LayerUse::Set => set,
            LayerUse::Usage(axis, positive, offset) => c.w.add(
                "IFCMATERIALLAYERSETUSAGE",
                format!(
                    "{set},.{axis}.,{},{},$",
                    if *positive {
                        ".POSITIVE."
                    } else {
                        ".NEGATIVE."
                    },
                    real(*offset)
                ),
            ),
        };
        let g = c.guid();
        c.w.add(
            "IFCRELASSOCIATESMATERIAL",
            format!("{g},$,$,$,({product}),{relating}"),
        );
    }

    // Element types, each with its shared parameters, its material or
    // layer set, and the elements of the type.
    for e in model.iter() {
        let ElementKind::ElementType(t) = &e.kind else {
            continue;
        };
        if let creabim_core::TypeParams::Object(o) = &t.params {
            let looks = Looks {
                materials: &material_refs,
                styles: &material_styles,
            };
            for (level, r) in object_type(&mut c, model, e, t, o, &looks, &storeys) {
                contained.entry(Some(level)).or_default().push(r);
            }
            continue;
        }
        let set = c.pset(TYPE_PSET, &type_props(t));
        let description = match t.description.trim() {
            "" => "$".to_string(),
            d => string(d),
        };
        let head = format!(
            "{},$,{},{description},$,({set}),$,$,$",
            string(&e.id.ifc_guid()),
            string(&e.name)
        );
        use creabim_core::TypeParams as P;
        let (class, tail) = match &t.params {
            P::Wall(w) if w.curtain.is_some() => ("IFCCURTAINWALLTYPE", ",.NOTDEFINED."),
            P::Wall(_) => ("IFCWALLTYPE", ",.STANDARD."),
            P::Door(_) => ("IFCDOORTYPE", ",.DOOR.,.SINGLE_SWING_LEFT.,.F.,$"),
            P::Window(_) => ("IFCWINDOWTYPE", ",.WINDOW.,.SINGLE_PANEL.,.F.,$"),
            P::Slab(_) => ("IFCSLABTYPE", ",.FLOOR."),
            P::Roof(_) => ("IFCROOFTYPE", ",.NOTDEFINED."),
            P::Column(_) => ("IFCCOLUMNTYPE", ",.COLUMN."),
            P::Beam(_) => ("IFCBEAMTYPE", ",.BEAM."),
            P::Object(_) => unreachable!("object types are written above"),
        };
        let tr = c.w.add(class, format!("{head}{tail}"));
        let relating = t
            .assembly()
            .and_then(|a| layer_sets.get(&a))
            .or_else(|| t.material().and_then(|m| material_refs.get(&m)))
            .copied();
        if let Some(m) = relating {
            let g = c.guid();
            c.w.add("IFCRELASSOCIATESMATERIAL", format!("{g},$,$,$,({tr}),{m}"));
        }
        if let Some(users) = typed.get(&e.id).filter(|u| !u.is_empty()) {
            let g = c.guid();
            c.w.add(
                "IFCRELDEFINESBYTYPE",
                format!("{g},$,$,$,{},{tr}", list(users)),
            );
        }
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

/// The materials written, and their surface styles.
struct Looks<'a> {
    materials: &'a HashMap<ElementId, Ref>,
    styles: &'a HashMap<ElementId, Ref>,
}

/// An object type: its parts as one representation map, each part's face
/// set styled with its material's look, its plan symbol as another, its
/// materials as a constituent set; then every object of the type, each
/// mapping both maps where it stands. Returns the objects written, with
/// their levels.
fn object_type(
    c: &mut Ctx,
    model: &Model,
    e: &Element,
    t: &creabim_core::ElementType,
    o: &creabim_core::ObjectType,
    looks: &Looks,
    storeys: &HashMap<ElementId, (Ref, Ref)>,
) -> Vec<(ElementId, Ref)> {
    let kind = o.ifc.normalized();
    // the parts, in the type's own axes
    let mut items = vec![];
    for p in &o.parts {
        let coords: Vec<String> = p
            .mesh
            .vertices
            .iter()
            .map(|v| point3(v.x, v.y, v.z))
            .collect();
        let pts =
            c.w.add("IFCCARTESIANPOINTLIST3D", format!("({})", coords.join(",")));
        let mut tris = vec![];
        for f in 0..p.mesh.faces.len() {
            for [a, b, d] in p.mesh.face_triangles(f) {
                tris.push(format!("({},{},{})", a + 1, b + 1, d + 1));
            }
        }
        let fs = c.w.add(
            "IFCTRIANGULATEDFACESET",
            format!("{pts},$,.T.,({}),$", tris.join(",")),
        );
        if let Some(style) = p.material.and_then(|m| looks.styles.get(&m)) {
            c.w.add("IFCSTYLEDITEM", format!("{fs},({style}),$"));
        }
        items.push(fs);
    }
    let body_rep = c.body_rep("Tessellation", &items);
    let origin = c.placement3d(DVec3::ZERO, None, None);
    let body_map =
        c.w.add("IFCREPRESENTATIONMAP", format!("{origin},{body_rep}"));
    // the plan symbol, as polylines on the floor
    let mut curves = vec![];
    for s in o.plan_symbol() {
        let pts: Vec<DVec2> = match s {
            creabim_core::objects::SymbolItem::Loop(mut p) => {
                if let Some(first) = p.first().copied() {
                    p.push(first);
                }
                p
            }
            creabim_core::objects::SymbolItem::Line(a, b) => vec![a, b],
        };
        if pts.len() < 2 {
            continue;
        }
        let refs: Vec<Ref> = pts
            .iter()
            .map(|p| c.w.add("IFCCARTESIANPOINT", point3(p.x, p.y, 0.0)))
            .collect();
        curves.push(c.w.add("IFCPOLYLINE", list(&refs)));
    }
    let foot_map = (!curves.is_empty()).then(|| {
        let set = c.w.add("IFCGEOMETRICCURVESET", list(&curves));
        let rep = c.w.add(
            "IFCSHAPEREPRESENTATION",
            format!("{},'FootPrint','GeometricCurveSet',({set})", c.footprint),
        );
        let origin = c.placement3d(DVec3::ZERO, None, None);
        c.w.add("IFCREPRESENTATIONMAP", format!("{origin},{rep}"))
    });
    let maps: Vec<Ref> = std::iter::once(body_map).chain(foot_map).collect();
    let set = c.pset(TYPE_PSET, &type_props(t));
    let description = match t.description.trim() {
        "" => "$".to_string(),
        d => string(d),
    };
    let type_class = kind.type_class().to_ascii_uppercase();
    // AssemblyPlace comes before PredefinedType on furniture types
    let place = if type_class == "IFCFURNITURETYPE" {
        ".NOTDEFINED.,"
    } else {
        ""
    };
    let tr = c.w.add(
        &type_class,
        format!(
            "{},$,{},{description},$,({set}),{},$,{},{place}.{}.",
            string(&e.id.ifc_guid()),
            string(&e.name),
            list(&maps),
            string(&e.name),
            kind.predefined
        ),
    );
    // the materials of the parts
    let mut constituents = vec![];
    for p in &o.parts {
        let Some(m) = p.material.and_then(|m| looks.materials.get(&m)) else {
            continue;
        };
        let name = if p.name.is_empty() {
            "$".to_string()
        } else {
            string(&p.name)
        };
        constituents.push(c.w.add("IFCMATERIALCONSTITUENT", format!("{name},$,{m},$,$")));
    }
    if !constituents.is_empty() {
        let cs = c.w.add(
            "IFCMATERIALCONSTITUENTSET",
            format!("{},$,{}", string(&e.name), list(&constituents)),
        );
        let g = c.guid();
        c.w.add("IFCRELASSOCIATESMATERIAL", format!("{g},$,$,$,({tr}),{cs}"));
    }
    // the objects of the type
    let mut out = vec![];
    for id in creabim_core::types::instances(model, e.id) {
        let Some(oe) = model.get(id) else { continue };
        let ElementKind::Object(ob) = &oe.kind else {
            continue;
        };
        let Some(&(_, spl)) = storeys.get(&ob.level) else {
            continue;
        };
        let (s, co) = ob.rotation.to_radians().sin_cos();
        // hung from the ceiling or not, where it stands above its storey
        let floor = model.level(ob.level).map_or(0.0, |l| l.elevation);
        let z = creabim_core::objects::base_z_of(model, ob, o) - floor;
        let pl = c.local_placement(
            Some(spl),
            ob.position.extend(z),
            Some(DVec3::new(co, s, 0.0)),
        );
        let mut reps = vec![];
        for (i, map) in maps.iter().enumerate() {
            let at = c.w.add("IFCCARTESIANPOINT", "(0.,0.,0.)");
            let op = c.w.add(
                "IFCCARTESIANTRANSFORMATIONOPERATOR3D",
                format!("$,$,{at},$,$"),
            );
            let item = c.w.add("IFCMAPPEDITEM", format!("{map},{op}"));
            let (ctx, ident) = if i == 0 {
                (c.body, "Body")
            } else {
                (c.footprint, "FootPrint")
            };
            reps.push(c.w.add(
                "IFCSHAPEREPRESENTATION",
                format!("{ctx},'{ident}','MappedRepresentation',({item})"),
            ));
        }
        let shape =
            c.w.add("IFCPRODUCTDEFINITIONSHAPE", format!("$,$,{}", list(&reps)));
        let object_type = if kind.predefined == "USERDEFINED" {
            string(&e.name)
        } else {
            "$".into()
        };
        let r = c.w.add(
            &kind.class.to_ascii_uppercase(),
            format!(
                "{},$,{},$,{object_type},{pl},{shape},$,.{}.",
                string(&oe.id.ifc_guid()),
                string(&oe.name),
                kind.predefined
            ),
        );
        out.push((ob.level, r));
    }
    if !out.is_empty() {
        let g = c.guid();
        let users: Vec<Ref> = out.iter().map(|o| o.1).collect();
        c.w.add(
            "IFCRELDEFINESBYTYPE",
            format!("{g},$,$,$,{},{tr}", list(&users)),
        );
    }
    out
}

/// How a layered element takes its assembly's layer set.
enum LayerUse {
    /// Through an `IfcMaterialLayerSetUsage`: the axis the layers run
    /// along, whether they run its positive way, and where the first
    /// layer starts from the element's reference line.
    Usage(&'static str, bool, f64),
    /// The set itself, for shapes that aren't straight extrusions.
    Set,
}

/// A triangle mesh as an `IfcTriangulatedFaceSet`, its corners taken into
/// the product's own coordinates by `to_local`.
/// `solid`, in a wall's own axes (`x` along `d` and `y` along `n` from
/// `origin`, `z` up from `z0`), with what lies above the underside `f` cut
/// away: a half-space over the face's plane, bounded by its outline in
/// plan, as Revit writes walls attached to roofs.
#[allow(clippy::too_many_arguments)]
fn clip_under(
    c: &mut Ctx,
    solid: Ref,
    f: &creabim_core::walls::TopFace,
    to_local: impl Fn(DVec2) -> DVec2,
    d: DVec2,
    n: DVec2,
    origin: DVec2,
    z0: f64,
) -> Ref {
    // the plane z = h + a x + b y in the wall's axes
    let (a, b) = (f.g.dot(d), f.g.dot(n));
    let h = f.c + f.g.dot(origin) - z0;
    let normal = DVec3::new(-a, -b, 1.0).normalize();
    let along = DVec3::new(1.0, 0.0, a).normalize();
    let pos = c.placement3d(DVec3::new(0.0, 0.0, h), Some(normal), Some(along));
    let plane = c.w.add("IFCPLANE", pos.to_string());
    let mut pts: Vec<Ref> = f
        .outline
        .iter()
        .map(|p| {
            let q = to_local(*p);
            c.w.add("IFCCARTESIANPOINT", point2(q.x, q.y))
        })
        .collect();
    pts.push(pts[0]);
    let boundary = c.w.add("IFCPOLYLINE", list(&pts));
    let at = c.placement3d(DVec3::ZERO, None, None);
    // the material is above the plane, on the side its normal points to
    let half = c.w.add(
        "IFCPOLYGONALBOUNDEDHALFSPACE",
        format!("{plane},.F.,{at},{boundary}"),
    );
    c.w.add(
        "IFCBOOLEANCLIPPINGRESULT",
        format!(".DIFFERENCE.,{solid},{half}"),
    )
}

fn tessellate(
    c: &mut Ctx,
    m: &creabim_geom::mesh::TriMesh,
    to_local: impl Fn(DVec3) -> DVec3,
) -> Ref {
    let coords: Vec<String> = m
        .positions
        .iter()
        .map(|p| {
            let q = to_local(glam::Vec3::from(*p).as_dvec3());
            point3(q.x, q.y, q.z)
        })
        .collect();
    let pts =
        c.w.add("IFCCARTESIANPOINTLIST3D", format!("({})", coords.join(",")));
    let tris: Vec<String> = m
        .indices
        .chunks(3)
        .map(|t| format!("({},{},{})", t[0] + 1, t[1] + 1, t[2] + 1))
        .collect();
    c.w.add(
        "IFCTRIANGULATEDFACESET",
        format!("{pts},$,.T.,({}),$", tris.join(",")),
    )
}

/// The placement and body of opening `o` in `wall` (placed at `wall_pl`,
/// its axis `loc` off its centreline): a box through the wall, a little
/// proud of both faces.
fn opening_body(
    c: &mut Ctx,
    wall_pl: Ref,
    loc: f64,
    wall: &creabim_core::Wall,
    o: &creabim_core::Opening,
) -> (Ref, Ref) {
    let eps = 0.05;
    let pl = c.local_placement(Some(wall_pl), DVec3::new(o.offset, -loc, o.sill), None);
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
    (pl, c.shape("SweptSolid", &[solid]))
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

/// The STEP entity and record tail (Tag, PredefinedType) of a member.
fn member_entity(k: &ElementKind) -> (&'static str, &'static str) {
    match k {
        ElementKind::Column(_) => ("IFCCOLUMN", ",$,.COLUMN."),
        ElementKind::Beam(_) => ("IFCBEAM", ",$,.BEAM."),
        ElementKind::Roof(r) => (
            "IFCROOF",
            match r.effective_shape() {
                creabim_core::RoofShape::Flat => ",$,.FLAT_ROOF.",
                creabim_core::RoofShape::Shed => ",$,.SHED_ROOF.",
                creabim_core::RoofShape::Gable => ",$,.GABLE_ROOF.",
                creabim_core::RoofShape::Hip => ",$,.HIP_ROOF.",
            },
        ),
        ElementKind::Stair(s) if s.landing.is_some() => ("IFCSTAIR", ",$,.QUARTER_TURN_STAIR."),
        ElementKind::Stair(_) => ("IFCSTAIR", ",$,.STRAIGHT_RUN_STAIR."),
        _ => ("IFCRAILING", ",$,.GUARDRAIL."),
    }
}

/// The STEP entity and record tail for a free mesh's IFC class; classes the
/// exporter cannot write become proxies.
fn free_mesh_entity(class: &str) -> (String, String) {
    match crate::classes::ifc4_element_tail(class) {
        Some(tail) => (class.to_ascii_uppercase(), format!(",${tail}")),
        None => ("IFCBUILDINGELEMENTPROXY".into(), ",$,.NOTDEFINED.".into()),
    }
}
