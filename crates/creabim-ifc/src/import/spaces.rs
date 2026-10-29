//! Rooms, room separators and ceilings: an IfcSpace becomes a room (its
//! number from Name, its name from LongName, its finishes and occupancy
//! from the standard property sets), found again from the walls round its
//! point when CreaBIM wrote it so, else with the outline of its body; an
//! IfcVirtualElement with an axis becomes a room separator; an IfcCovering
//! of type CEILING with a flat body becomes a ceiling, over the room it
//! covers when CreaBIM wrote which.

use super::Importer;
use crate::params::{ceiling_from, room_from, CEILING_PSET, ROOM_PSET};
use creabim_core::{Ceiling, Element, ElementId, ElementKind, Room, Separator};
use glam::DVec2;

/// A point well inside a polygon: its middle when that is inside, else the
/// middle of its first ear.
fn inside_point(poly: &[DVec2]) -> DVec2 {
    let n = poly.len().max(1) as f64;
    let middle = poly.iter().copied().sum::<DVec2>() / n;
    if creabim_core::polygon::contains(poly, middle) {
        return middle;
    }
    creabim_core::polygon::triangulate(poly)
        .first()
        .map_or(middle, |t| (poly[t[0]] + poly[t[1]] + poly[t[2]]) / 3.0)
}

impl Importer<'_> {
    pub(super) fn import_spaces(&mut self) {
        let f = self.f;
        let mut spaces: Vec<u64> = self.ids("IFCSPACE").to_vec();
        spaces.sort_unstable();
        let mut skipped = 0;
        for id in spaces {
            let Some(e) = f.get(id) else { continue };
            let world = self.g.placement(e.arg(5));
            let Some(p) = self.prism(e, &world) else {
                skipped += 1;
                continue;
            };
            let (level, elevation) = self.level_for(id, p.z0);
            let own = self.props(id, ROOM_PSET);
            let (at, drawn, height, department) = room_from(&own);
            let mut r = Room::new(level, at.unwrap_or_else(|| inside_point(&p.footprint)));
            // CreaBIM's own rooms are found from their walls again
            if at.is_none() || drawn {
                r.boundary = p.footprint.clone();
            }
            let tall = p.z1 - elevation;
            r.height = height.or((at.is_none() && tall > 0.1).then_some(tall));
            r.department = department;
            let text = |set: &str, key: &str| {
                self.props(id, set)
                    .get(key)
                    .and_then(|v| v.1.clone())
                    .unwrap_or_default()
            };
            r.floor_finish = text("Pset_SpaceCoveringRequirements", "FloorCovering");
            r.wall_finish = text("Pset_SpaceCoveringRequirements", "WallCovering");
            r.ceiling_finish = text("Pset_SpaceCoveringRequirements", "CeilingCovering");
            r.occupancy = text("Pset_SpaceOccupancyRequirements", "OccupancyType");
            let number = e.arg(2).as_str().unwrap_or("").trim().to_string();
            let long = e.arg(7).as_str().unwrap_or("").trim().to_string();
            let name = if long.is_empty() {
                if number.is_empty() {
                    "Room".to_string()
                } else {
                    number.clone()
                }
            } else {
                r.number = number;
                long
            };
            self.done.insert(id);
            let eid = self.new_id(e);
            self.other_elems.push(Element {
                mark: None,
                source: None,
                of_type: None,
                id: eid,
                name,
                kind: ElementKind::Room(r),
            });
        }
        if skipped > 0 {
            self.g.warnings.add(format!(
                "{skipped} spaces have no flat outline CreaBIM can read and were left out"
            ));
        }
        let mut virtuals: Vec<u64> = self.ids("IFCVIRTUALELEMENT").to_vec();
        virtuals.sort_unstable();
        for id in virtuals {
            let Some(e) = f.get(id) else { continue };
            let world = self.g.placement(e.arg(5));
            let Some((a, b)) = self.axis(e, &world) else {
                continue;
            };
            let (level, _) = self.level_for(id, world.w_axis.z);
            self.done.insert(id);
            let eid = self.new_id(e);
            self.other_elems.push(Element {
                mark: None,
                source: None,
                of_type: None,
                id: eid,
                name: Self::name(e, "Room separator"),
                kind: ElementKind::RoomSeparator(Separator { level, a, b }),
            });
        }
        let mut coverings: Vec<u64> = self.ids("IFCCOVERING").to_vec();
        coverings.sort_unstable();
        for id in coverings {
            let Some(e) = f.get(id) else { continue };
            let ceiling = e
                .arg(8)
                .as_enum()
                .is_some_and(|t| t.eq_ignore_ascii_case("CEILING"));
            if !ceiling {
                continue;
            }
            let world = self.g.placement(e.arg(5));
            let Some(p) = self.prism(e, &world).filter(|p| p.holes.is_empty()) else {
                continue;
            };
            let (level, elevation) = self.level_for(id, p.z0);
            let (grid, room) = ceiling_from(&self.props(id, CEILING_PSET));
            let mut c = Ceiling::drawn(level, p.footprint, p.z0 - elevation);
            c.thickness = (p.z1 - p.z0).max(0.001);
            c.grid = grid;
            // over its room again, when that room came along
            if let Some(room) = room.as_deref().and_then(ElementId::from_ifc_guid) {
                if self.other_elems.iter().any(|x| x.id == room) {
                    c.room = Some(room);
                    c.boundary.clear();
                }
            }
            c.material = self.material_for(id);
            self.done.insert(id);
            let eid = self.new_id(e);
            self.other_elems.push(Element {
                mark: None,
                source: None,
                of_type: None,
                id: eid,
                name: Self::name(e, "Ceiling"),
                kind: ElementKind::Ceiling(c),
            });
        }
    }
}
