//! Structures into a `structures` pack: a blueprint per structure, a site
//! per group of structures that belong together, and a profile per class
//! ([`contentpack::records::structure`]).
//!
//! [`read`] finds and dresses the structures in the map; [`pack`] classes
//! them with the learned trees and writes the records. Blueprints keep
//! every piece's object id beside its category, and every wall, roof and
//! floor's material, so the client can place one as found or swap pieces
//! for others of the same category or material. Positions are turned into
//! pack axes on the way: x east and z south from each box's north-west
//! corner. `docs/pack/structures.md` has the layout.

use std::collections::{BTreeMap, BTreeSet};

use contentpack::records::structure::{
    self as rec, Blueprint, Count as RecCount, Dressing as RecDressing, Furnishing, Link, Member,
    NONE, NONE8, NONE16, Piece, Site, Spread as RecSpread, UseProfile,
};
use contentpack::{Kind, PackWriter};
use rs2cache::Records;
use rs2cache::types::config::{FloDef, LocDef};
use rs2cache::types::map::Region;
use rs2cache::types::record::{Record, RecordType};

use crate::ConvertError;
use crate::furnish::Category;
use crate::learn::Trees;
use crate::material::{self, Dressing, Material};
use crate::profile::{self, Count, Profile, Spread, Via};
use crate::purpose::{Class, Use};
use crate::structure::{self, Placed, Structure};

/// What [`read`] found.
pub struct Found {
    /// The structures, dressed in their materials.
    pub structures: Vec<Structure>,
    /// Regions that wouldn't load, with why.
    pub failed: Vec<String>,
}

/// Finds and dresses the structures in the regions `wanted` keeps, by
/// region `(x, y)`.
pub fn read(records: &Records, wanted: &dyn Fn(u8, u8) -> bool) -> Result<Found, ConvertError> {
    let cache = records.cache();
    let index = cache.map_index()?;
    let mut regions: Vec<Region> = Vec::new();
    let mut failed = Vec::new();
    for e in index.iter().filter(|e| wanted(e.x(), e.y())) {
        match cache.region_for(e) {
            Ok(r) => regions.push(r),
            Err(err) => failed.push(err.to_string()),
        }
    }
    let ids: BTreeSet<u16> = regions
        .iter()
        .flat_map(|r| r.locations.iter().map(|l| l.id))
        .collect();
    let defs: BTreeMap<u16, LocDef> = ids
        .into_iter()
        .filter_map(|id| match records.decode(RecordType::Loc, id.into()) {
            Ok(Record::Loc(d)) => Some((id, d)),
            _ => None,
        })
        .collect();
    let floors: BTreeMap<u16, FloDef> = records
        .ids(RecordType::Flo)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|id| match records.decode(RecordType::Flo, id) {
            Ok(Record::Flo(d)) => Some((u16::try_from(id).ok()?, d)),
            _ => None,
        })
        .collect();
    let mut structures = structure::find(&regions, &defs);
    material::dress(&mut structures, records, &defs, &floors);
    Ok(Found { structures, failed })
}

/// The names of each code a `structures` pack uses, for its manifest.
pub fn names() -> serde_json::Value {
    fn named<T: serde::Serialize>(all: &[T]) -> Vec<serde_json::Value> {
        all.iter()
            .map(|v| serde_json::to_value(v).unwrap_or_default())
            .collect()
    }
    serde_json::json!({
        "classes": named(&Class::ALL),
        "uses": named(&Use::ALL),
        "categories": named(&Category::ALL),
        "materials": named(&Material::ALL),
        "vias": named(&[Via::Door, Via::Above]),
    })
}

/// A `structures` pack for `structures`, classed and grouped by `trees`.
pub fn pack(structures: &[Structure], trees: &Trees, crc: u32) -> Result<PackWriter, ConvertError> {
    let guesses: Vec<(Class, Vec<Use>)> = structures.iter().map(|s| trees.classify(s)).collect();
    let roots = trees.sites(structures);
    let ids = structures
        .iter()
        .map(|s| {
            let plane = s.planes.first().copied().unwrap_or(0);
            rec::blueprint_id(plane, s.origin.0, s.origin.1).ok_or_else(|| {
                ConvertError::Invalid(format!(
                    "structure {} has no blueprint id",
                    structure::id(s)
                ))
            })
        })
        .collect::<Result<Vec<u32>, _>>()?;
    if ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err(ConvertError::Invalid(
            "two structures share an origin and lowest plane".into(),
        ));
    }
    // Each site's members, named by their lowest blueprint id.
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, &root) in roots.iter().enumerate() {
        groups.entry(root).or_default().push(i);
    }
    let mut site_of: Vec<u32> = vec![NONE; structures.len()];
    let mut w = PackWriter::new(Kind::Structures, crc);
    for members in groups.values().filter(|m| m.len() > 1) {
        let first = members.iter().map(|&i| ids[i]).min().expect("members");
        let id = rec::site_id(first);
        for &i in members {
            site_of[i] = id;
        }
        w.insert(id, site(structures, &ids, members)?.encode()?);
    }
    for (i, s) in structures.iter().enumerate() {
        let b = blueprint(s, guesses[i].0, &guesses[i].1, site_of[i])?;
        w.insert(ids[i], b.encode()?);
    }
    for p in profile::profiles(structures, &guesses, &roots) {
        w.insert(
            rec::profile_id(p.class as u8),
            profile_record(&p)?.encode()?,
        );
    }
    w.set_extra("names", names());
    Ok(w)
}

fn out_of_range(what: &str) -> ConvertError {
    ConvertError::Invalid(format!("{what} out of range for a structures record"))
}

fn small<T: TryFrom<usize>>(n: usize, what: &str) -> Result<T, ConvertError> {
    T::try_from(n).map_err(|_| out_of_range(what))
}

fn dressing(d: Option<Dressing>) -> RecDressing {
    d.map_or(RecDressing::NONE, |d| RecDressing {
        material: d.material as u8,
        material_ref: d.texture.map_or(NONE, |t| {
            u32::from(crate::material_id::TEXTURE_BASE) + u32::from(t)
        }),
        colour: d.colour.unwrap_or(NONE),
    })
}

/// `p` placed in a box `depth` tiles deep, z running south from its north
/// edge.
fn piece(p: &Placed, depth: u16) -> Result<Piece, ConvertError> {
    let z = i32::from(depth) - 1 - p.y;
    Ok(Piece {
        object: u32::from(p.id),
        category: p.category.map_or(NONE8, |c| c as u8),
        icon: p.icon.unwrap_or(NONE16),
        kind: p.kind,
        rotation: p.rotation,
        plane: p.plane,
        x: i16::try_from(p.x).map_err(|_| out_of_range("a piece's x"))?,
        z: i16::try_from(z).map_err(|_| out_of_range("a piece's z"))?,
        room: p.room.map_or(Ok(NONE16), |r| small(r, "a room index"))?,
        dressing: dressing(p.material),
    })
}

fn pieces(list: &[Placed], depth: u16) -> Result<Vec<Piece>, ConvertError> {
    list.iter().map(|p| piece(p, depth)).collect()
}

fn blueprint(
    s: &Structure,
    class: Class,
    uses: &[Use],
    site: u32,
) -> Result<Blueprint, ConvertError> {
    let depth = s.size.1;
    // A tile's z, south from the box's north edge.
    let south = |y: u16| depth.saturating_sub(1).saturating_sub(y);
    let room_index = |r: Option<usize>| r.map_or(Ok(NONE16), |r| small(r, "a room index"));
    let rooms = s
        .rooms
        .iter()
        .zip(uses)
        .map(|(r, &u)| {
            Ok(rec::Room {
                plane: r.plane,
                roofed: r.roofed,
                use_: u as u8,
                floor: dressing(r.floor),
                tiles: r
                    .tiles
                    .iter()
                    .map(|t| {
                        Ok(rec::Tile {
                            x: t.x,
                            z: south(t.y),
                            overlay: t.overlay,
                            underlay: t.underlay,
                            height: i16::try_from(t.height)
                                .map_err(|_| out_of_range("a tile's height"))?,
                        })
                    })
                    .collect::<Result<_, ConvertError>>()?,
            })
        })
        .collect::<Result<_, ConvertError>>()?;
    let doors = s
        .doors
        .iter()
        .map(|d| {
            Ok(rec::Door {
                piece: piece(&d.placed, depth)?,
                open: d.open,
                rooms: [room_index(d.rooms[0])?, room_index(d.rooms[1])?],
            })
        })
        .collect::<Result<_, ConvertError>>()?;
    let openings = s
        .openings
        .iter()
        .map(|o| {
            Ok(rec::Opening {
                room: small(o.room, "a room index")?,
                plane: o.plane,
                x: o.x,
                z: south(o.y),
                side: o.side,
            })
        })
        .collect::<Result<_, ConvertError>>()?;
    Ok(Blueprint {
        width: s.size.0,
        depth,
        planes: s.planes.clone(),
        class: class as u8,
        site,
        clipped: s.clipped,
        rooms,
        doors,
        openings,
        walls: pieces(&s.walls, depth)?,
        roofs: pieces(&s.roofs, depth)?,
        objects: pieces(&s.objects, depth)?,
    })
}

/// The site record for `members`, placed from the north-west corner of the
/// box round them.
fn site(structures: &[Structure], ids: &[u32], members: &[usize]) -> Result<Site, ConvertError> {
    let west = members
        .iter()
        .map(|&i| structures[i].origin.0)
        .min()
        .unwrap_or(0);
    let south = members
        .iter()
        .map(|&i| structures[i].origin.1)
        .min()
        .unwrap_or(0);
    let east = members
        .iter()
        .map(|&i| structures[i].origin.0 + structures[i].size.0)
        .max()
        .unwrap_or(0);
    let north = members
        .iter()
        .map(|&i| structures[i].origin.1 + structures[i].size.1)
        .max()
        .unwrap_or(0);
    let mut list: Vec<Member> = members
        .iter()
        .map(|&i| {
            let s = &structures[i];
            Member {
                blueprint: ids[i],
                x: s.origin.0 - west,
                z: north - (s.origin.1 + s.size.1),
            }
        })
        .collect();
    list.sort_by_key(|m| m.blueprint);
    Ok(Site {
        width: east - west,
        depth: north - south,
        members: list,
    })
}

fn spread(s: &Spread) -> Result<RecSpread, ConvertError> {
    let v = |x: u32| u16::try_from(x).map_err(|_| out_of_range("a spread"));
    Ok(RecSpread([
        v(s.min)?,
        v(s.low)?,
        v(s.median)?,
        v(s.high)?,
        v(s.max)?,
    ]))
}

fn counts<T: Copy>(
    list: &[Count<T>],
    code: impl Fn(T) -> u8,
) -> Result<Vec<RecCount>, ConvertError> {
    list.iter()
        .map(|c| {
            Ok(RecCount {
                code: code(c.what),
                count: small(c.count, "a count")?,
            })
        })
        .collect()
}

fn profile_record(p: &Profile) -> Result<rec::Profile, ConvertError> {
    let uses = p
        .uses
        .iter()
        .map(|u| {
            Ok(UseProfile {
                use_: u.use_ as u8,
                rooms: small(u.rooms, "a room count")?,
                structures: small(u.structures, "a structure count")?,
                tiles: spread(&u.tiles)?,
                ground: small(u.ground, "a room count")?,
                roofed: small(u.roofed, "a room count")?,
                entrances: small(u.entrances, "a room count")?,
                furnishings: u
                    .furnishings
                    .iter()
                    .map(|f| {
                        Ok(Furnishing {
                            category: f.category as u8,
                            rooms: small(f.rooms, "a room count")?,
                            count: small(f.count, "a furnishing count")?,
                        })
                    })
                    .collect::<Result<_, ConvertError>>()?,
            })
        })
        .collect::<Result<_, ConvertError>>()?;
    let links = p
        .links
        .iter()
        .map(|l| {
            Ok(Link {
                from: l.from as u8,
                to: l.to as u8,
                via: l.via as u8,
                count: small(l.count, "a link count")?,
            })
        })
        .collect::<Result<_, ConvertError>>()?;
    Ok(rec::Profile {
        class: p.class as u8,
        structures: small(p.structures, "a structure count")?,
        sizes: [
            spread(&p.width)?,
            spread(&p.depth)?,
            spread(&p.planes)?,
            spread(&p.rooms)?,
            spread(&p.yards)?,
            spread(&p.entrances)?,
        ],
        uses,
        links,
        floors: counts(&p.floors, |m| m as u8)?,
        walls: counts(&p.walls, |m| m as u8)?,
        roofs: counts(&p.roofs, |m| m as u8)?,
        outside: counts(&p.outside, |c| c as u8)?,
        on_sites: small(p.on_sites, "a structure count")?,
        site_classes: counts(&p.site_classes, |c| c as u8)?,
        site_gap: p.site_gap.as_ref().map(spread).transpose()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_declaration_order() {
        assert!(Class::ALL.iter().enumerate().all(|(i, &c)| c as usize == i));
        assert!(Use::ALL.iter().enumerate().all(|(i, &u)| u as usize == i));
        assert!(
            Category::ALL
                .iter()
                .enumerate()
                .all(|(i, &c)| c as usize == i)
        );
        assert!(
            Material::ALL
                .iter()
                .enumerate()
                .all(|(i, &m)| m as usize == i)
        );
        assert_eq!(Via::Door as u8, 0);
        assert_eq!(Via::Above as u8, 1);
        let n = names();
        assert_eq!(n["classes"][0], "house");
        assert_eq!(n["uses"].as_array().unwrap().len(), Use::ALL.len());
    }

    #[test]
    fn positions_run_south_from_the_north_west_corner() {
        use crate::structure::{Room, RoomTile};
        let placed = |y: i32| Placed {
            id: 7,
            name: None,
            category: Some(Category::Bed),
            icon: None,
            material: None,
            kind: 10,
            rotation: 1,
            plane: 0,
            x: 1,
            y,
            room: Some(0),
        };
        let tile = |x: u16, y: u16| RoomTile {
            x,
            y,
            overlay: 0,
            underlay: 1,
            height: 240,
        };
        let s = Structure {
            origin: (3200, 3200),
            size: (2, 3),
            planes: vec![0],
            rooms: vec![Room {
                plane: 0,
                roofed: true,
                tiles: vec![tile(0, 0), tile(1, 2)],
                floor: None,
            }],
            doors: Vec::new(),
            openings: Vec::new(),
            walls: Vec::new(),
            roofs: vec![placed(3)],
            objects: vec![placed(0)],
            clipped: false,
        };
        let b = blueprint(&s, Class::House, &[Use::Bedroom], NONE).unwrap();
        // The south-west tile is the south row; the north-east one is z 0.
        let z: Vec<u16> = b.rooms[0].tiles.iter().map(|t| t.z).collect();
        assert_eq!(z, vec![2, 0]);
        assert_eq!(b.rooms[0].tiles[0].height, 240);
        // A roof piece just past the north edge is at z -1.
        assert_eq!((b.objects[0].z, b.roofs[0].z), (2, -1));
        assert_eq!(b.objects[0].category, Category::Bed as u8);
        assert_eq!(
            (b.class, b.rooms[0].use_),
            (Class::House as u8, Use::Bedroom as u8)
        );
    }
}
