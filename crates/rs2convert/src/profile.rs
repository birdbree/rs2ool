//! Class profiles: what the structures of each class are like across the
//! map, for generating new ones by the same rules. A profile gathers the
//! structures the learned trees ([`crate::learn`]) put in one class: their
//! size, planes and rooms, which room uses they have and how those rooms
//! connect, what they're made of, which furnishings go in which rooms, and
//! what they stand with on a site.
//!
//! Counts are plain counts over the structures read, and sizes are spreads
//! (least, quartiles, most), so a generator can pick within what's usual
//! and know what's rare. `docs/conversion/structure-profiles.md` has the
//! fields and the figures for one cache.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::furnish::Category;
use crate::material::{Dressing, Material};
use crate::purpose::{Class, Use};
use crate::structure::{self, Structure};

/// How a number spreads over some structures or rooms: the least, the
/// quartiles and the most, each one of the values (by nearest rank).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spread {
    /// The least.
    pub min: u32,
    /// A quarter are at or below it.
    pub low: u32,
    /// Half are at or below it.
    pub median: u32,
    /// Three quarters are at or below it.
    pub high: u32,
    /// The most.
    pub max: u32,
}

impl Spread {
    /// The spread of `values`, or `None` when there are none.
    pub fn of(mut values: Vec<u32>) -> Option<Self> {
        values.sort_unstable();
        let n = values.len();
        let rank = |quarters: usize| values[(quarters * n).div_ceil(4).max(1) - 1];
        Some(Self {
            min: *values.first()?,
            low: rank(1),
            median: rank(2),
            high: rank(3),
            max: rank(4),
        })
    }
}

/// How often something comes up, commonest first in every list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Count<T> {
    /// The thing.
    pub what: T,
    /// How many: structures, rooms, pieces or tiles, as the list says.
    pub count: usize,
}

/// Which way two rooms connect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Via {
    /// A closed door between them, on one plane.
    Door,
    /// The second is on the plane above the first, over some of its
    /// tiles, which is how the floors of a building stack up.
    Above,
}

/// Rooms of one use connected to rooms of another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    /// One room's use: the lower one for [`Via::Above`]; for a door, the
    /// lesser of the two in [`Use`]'s order.
    pub from: Use,
    /// The other's.
    pub to: Use,
    /// How they connect.
    pub via: Via,
    /// How many pairs of rooms connect this way.
    pub count: usize,
}

/// One kind of furnishing in rooms of one use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Furnishing {
    /// Its category.
    pub category: Category,
    /// How many rooms of the use have at least one.
    pub rooms: usize,
    /// How many there are in all those rooms.
    pub count: usize,
}

/// The rooms of one use in a class.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoomProfile {
    /// The use.
    #[serde(rename = "use")]
    pub use_: Use,
    /// How many rooms have it.
    pub rooms: usize,
    /// How many of the class's structures have at least one.
    pub structures: usize,
    /// How many tiles each room has.
    pub tiles: Spread,
    /// How many are on their structure's lowest plane.
    pub ground: usize,
    /// How many are roofed (the rest are yards).
    pub roofed: usize,
    /// How many lead outside, through a closed door or an opening.
    pub entrances: usize,
    /// What's in them, by category, commonest first.
    pub furnishings: Vec<Furnishing>,
}

/// What the structures of one class are like.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// The class.
    pub class: Class,
    /// How many structures are in it.
    pub structures: usize,
    /// Their width (east–west) in tiles.
    pub width: Spread,
    /// Their depth (north–south) in tiles.
    pub depth: Spread,
    /// How many planes they have rooms on.
    pub planes: Spread,
    /// How many rooms they have, yards included.
    pub rooms: Spread,
    /// How many of those are yards.
    pub yards: Spread,
    /// How many ways lead outside: closed doors, and runs of open edges
    /// (an open doorway, an archway, a stall's open front), each run
    /// counted once.
    pub entrances: Spread,
    /// Their rooms, by use, commonest first.
    pub uses: Vec<RoomProfile>,
    /// How rooms of each use connect, commonest first.
    pub links: Vec<Link>,
    /// What their rooms' floors are made of, by tiles.
    pub floors: Vec<Count<Material>>,
    /// What their walls are made of, by pieces.
    pub walls: Vec<Count<Material>>,
    /// What their roofs are made of, by pieces.
    pub roofs: Vec<Count<Material>>,
    /// What hangs on their outside walls, by category.
    pub outside: Vec<Count<Category>>,
    /// How many share a site with other structures.
    pub on_sites: usize,
    /// The classes of the structures they share a site with, counted once
    /// per pair.
    pub site_classes: Vec<Count<Class>>,
    /// How far, in tiles, each structure on a site is from the nearest
    /// other structure on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site_gap: Option<Spread>,
}

/// One profile for each class the guesses give, in [`Class`]'s order.
/// `guesses` holds each structure's class and its rooms' uses (from
/// [`crate::learn::Trees::classify`]), and `sites` each structure's site
/// (from [`crate::learn::Trees::sites`]), both in the order of
/// `structures`.
pub fn profiles(
    structures: &[Structure],
    guesses: &[(Class, Vec<Use>)],
    sites: &[usize],
) -> Vec<Profile> {
    let mut by_class: BTreeMap<Class, Vec<usize>> = BTreeMap::new();
    for (i, (class, _)) in guesses.iter().enumerate() {
        by_class.entry(*class).or_default().push(i);
    }
    let mut members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, &site) in sites.iter().enumerate() {
        members.entry(site).or_default().push(i);
    }
    by_class
        .into_iter()
        .map(|(class, ids)| {
            profile(class, &ids, structures, guesses, &|i| {
                members[&sites[i]]
                    .iter()
                    .copied()
                    .filter(|&j| j != i)
                    .collect()
            })
        })
        .collect()
}

fn profile(
    class: Class,
    ids: &[usize],
    structures: &[Structure],
    guesses: &[(Class, Vec<Use>)],
    others: &dyn Fn(usize) -> Vec<usize>,
) -> Profile {
    let spread = |f: &dyn Fn(&Structure) -> usize| {
        let values = ids
            .iter()
            .map(|&i| u32::try_from(f(&structures[i])).unwrap_or(u32::MAX))
            .collect();
        Spread::of(values).expect("a class has structures")
    };
    let mut rooms: BTreeMap<Use, Rooms> = BTreeMap::new();
    let mut links: BTreeMap<(Use, Use, Via), usize> = BTreeMap::new();
    let (mut floors, mut walls, mut roofs) = (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    let mut outside: BTreeMap<Category, usize> = BTreeMap::new();
    let (mut on_sites, mut site_classes, mut site_gaps) = (0, BTreeMap::new(), Vec::new());
    for &i in ids {
        let s = &structures[i];
        let uses = &guesses[i].1;
        let lowest = s.planes.first().copied().unwrap_or(0);
        let leads_out = entrances(s);
        for (r, room) in s.rooms.iter().enumerate() {
            let entry = rooms.entry(uses[r]).or_default();
            entry.structures.insert(i);
            entry
                .tiles
                .push(u32::try_from(room.tiles.len()).unwrap_or(u32::MAX));
            entry.ground += usize::from(room.plane == lowest);
            entry.roofed += usize::from(room.roofed);
            entry.entrances += usize::from(leads_out.contains(&r));
            let mut here: BTreeMap<Category, usize> = BTreeMap::new();
            for o in s.objects.iter().filter(|o| o.room == Some(r)) {
                if let Some(c) = o.category {
                    *here.entry(c).or_default() += 1;
                }
            }
            for (c, n) in here {
                let f = entry.furnishings.entry(c).or_default();
                f.0 += 1;
                f.1 += n;
            }
            if let Some(m) = room.floor.map(|d| d.material) {
                *floors.entry(m).or_default() += room.tiles.len();
            }
        }
        for (a, b, via) in connections(s) {
            let (from, to) = match via {
                Via::Door => (uses[a].min(uses[b]), uses[a].max(uses[b])),
                Via::Above => (uses[a], uses[b]),
            };
            *links.entry((from, to, via)).or_default() += 1;
        }
        tally(&mut walls, s.walls.iter().filter_map(|w| w.material));
        tally(&mut roofs, s.roofs.iter().filter_map(|r| r.material));
        for o in s.objects.iter().filter(|o| o.room.is_none()) {
            if let Some(c) = o.category {
                *outside.entry(c).or_default() += 1;
            }
        }
        let others = others(i);
        if !others.is_empty() {
            on_sites += 1;
            for &j in &others {
                *site_classes.entry(guesses[j].0).or_default() += 1;
            }
            site_gaps.extend(
                others
                    .iter()
                    .map(|&j| structure::gap(s, &structures[j]))
                    .min(),
            );
        }
    }
    let mut uses: Vec<RoomProfile> = rooms
        .into_iter()
        .map(|(use_, r)| RoomProfile {
            use_,
            rooms: r.tiles.len(),
            structures: r.structures.len(),
            tiles: Spread::of(r.tiles).expect("a use has rooms"),
            ground: r.ground,
            roofed: r.roofed,
            entrances: r.entrances,
            furnishings: commonest(
                r.furnishings
                    .into_iter()
                    .map(|(category, (rooms, count))| Furnishing {
                        category,
                        rooms,
                        count,
                    })
                    .collect(),
                |f| (f.rooms, f.count),
            ),
        })
        .collect();
    uses.sort_by_key(|u| std::cmp::Reverse(u.rooms));
    let links = commonest(
        links
            .into_iter()
            .map(|((from, to, via), count)| Link {
                from,
                to,
                via,
                count,
            })
            .collect(),
        |l| (l.count, 0),
    );
    Profile {
        class,
        structures: ids.len(),
        width: spread(&|s| usize::from(s.size.0)),
        depth: spread(&|s| usize::from(s.size.1)),
        planes: spread(&|s| s.planes.len()),
        rooms: spread(&|s| s.rooms.len()),
        yards: spread(&|s| s.rooms.iter().filter(|r| !r.roofed).count()),
        entrances: spread(&ways_out),
        uses,
        links,
        floors: counts(floors),
        walls: counts(walls),
        roofs: counts(roofs),
        outside: counts(outside),
        on_sites,
        site_classes: counts(site_classes),
        site_gap: Spread::of(site_gaps),
    }
}

/// What's gathered for the rooms of one use.
#[derive(Default)]
struct Rooms {
    structures: BTreeSet<usize>,
    tiles: Vec<u32>,
    ground: usize,
    roofed: usize,
    entrances: usize,
    /// By category: rooms with one, and how many in all.
    furnishings: BTreeMap<Category, (usize, usize)>,
}

/// The rooms of `s` that lead outside, through a closed door or an opening.
fn entrances(s: &Structure) -> BTreeSet<usize> {
    let doors = s
        .doors
        .iter()
        .filter(|d| !d.open && d.rooms.contains(&None))
        .flat_map(|d| d.rooms.iter().flatten().copied());
    doors.chain(s.openings.iter().map(|o| o.room)).collect()
}

/// How many ways lead out of `s`: closed doors to the outside, and runs of
/// openings along one side of a row or column of tiles.
fn ways_out(s: &Structure) -> usize {
    let doors = s
        .doors
        .iter()
        .filter(|d| !d.open && d.rooms.contains(&None))
        .count();
    // An opening's place along its edge's line, and the line itself: on a
    // west or east edge the line is a column and the place its y.
    let edges: BTreeSet<((u8, u8, u16), u16)> = s
        .openings
        .iter()
        .map(|o| match o.side {
            0 | 2 => ((o.plane, o.side, o.x), o.y),
            _ => ((o.plane, o.side, o.y), o.x),
        })
        .collect();
    let runs = edges
        .iter()
        .filter(|&&(line, at)| at == 0 || !edges.contains(&(line, at - 1)))
        .count();
    doors + runs
}

/// Each pair of rooms in `s` that connect, once: through a closed door
/// (either way round), or the second over the first on the plane above.
fn connections(s: &Structure) -> BTreeSet<(usize, usize, Via)> {
    let mut out = BTreeSet::new();
    for d in s.doors.iter().filter(|d| !d.open) {
        if let [Some(a), Some(b)] = d.rooms
            && a != b
        {
            out.insert((a.min(b), a.max(b), Via::Door));
        }
    }
    let tiles: Vec<BTreeSet<(u16, u16)>> = s
        .rooms
        .iter()
        .map(|r| r.tiles.iter().map(|t| (t.x, t.y)).collect())
        .collect();
    for (a, lower) in s.rooms.iter().enumerate() {
        for (b, upper) in s.rooms.iter().enumerate() {
            if upper.plane == lower.plane + 1 && !tiles[a].is_disjoint(&tiles[b]) {
                out.insert((a, b, Via::Above));
            }
        }
    }
    out
}

fn tally(into: &mut BTreeMap<Material, usize>, dressings: impl Iterator<Item = Dressing>) {
    for d in dressings {
        *into.entry(d.material).or_default() += 1;
    }
}

/// `map` as counts, commonest first, ties in the map's order.
fn counts<T: Copy>(map: BTreeMap<T, usize>) -> Vec<Count<T>> {
    commonest(
        map.into_iter()
            .map(|(what, count)| Count { what, count })
            .collect(),
        |c| (c.count, 0),
    )
}

/// `items` sorted by `key`, largest first; the sort is stable, so ties keep
/// their order.
fn commonest<T>(mut items: Vec<T>, key: impl Fn(&T) -> (usize, usize)) -> Vec<T> {
    items.sort_by_key(|t| std::cmp::Reverse(key(t)));
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::furnish;
    use crate::structure::{Door, Opening, Placed, Room, RoomTile};

    fn room(plane: u8, roofed: bool, x0: u16, w: u16, d: u16) -> Room {
        Room {
            plane,
            roofed,
            tiles: (x0..x0 + w)
                .flat_map(|x| (0..d).map(move |y| (x, y)))
                .map(|(x, y)| RoomTile {
                    x,
                    y,
                    overlay: 0,
                    underlay: 1,
                    height: 0,
                })
                .collect(),
            floor: None,
        }
    }

    fn placed(name: &str, kind: u8, room: Option<usize>) -> Placed {
        Placed {
            id: 1,
            name: Some(name.into()),
            category: furnish::category(name),
            icon: None,
            material: None,
            kind,
            rotation: 0,
            plane: 0,
            x: 1,
            y: 1,
            room,
        }
    }

    /// Two rooms side by side, a closed door between them and one to the
    /// outside, an archway out of the second, and a room above the first;
    /// two beds in the second room.
    fn house(x: u16) -> Structure {
        Structure {
            origin: (x, 100),
            size: (8, 4),
            planes: vec![0, 1],
            rooms: vec![
                room(0, true, 0, 4, 4),
                room(0, true, 4, 4, 4),
                room(1, true, 0, 4, 4),
            ],
            doors: vec![
                Door {
                    placed: placed("Door", 0, None),
                    open: false,
                    rooms: [Some(0), Some(1)],
                },
                Door {
                    placed: placed("Door", 0, None),
                    open: false,
                    rooms: [None, Some(0)],
                },
            ],
            // A two-tile archway out of the second room.
            openings: [0, 1]
                .map(|y| Opening {
                    room: 1,
                    plane: 0,
                    x: 7,
                    y,
                    side: 2,
                })
                .to_vec(),
            walls: Vec::new(),
            roofs: Vec::new(),
            objects: vec![placed("Bed", 10, Some(1)), placed("Bed", 10, Some(1))],
            clipped: false,
        }
    }

    #[test]
    fn spreads_take_nearest_ranks() {
        assert_eq!(Spread::of(Vec::new()), None);
        let s = Spread::of(vec![5, 1, 4, 2, 3]).unwrap();
        assert_eq!((s.min, s.low, s.median, s.high, s.max), (1, 2, 3, 4, 5));
        let one = Spread::of(vec![7]).unwrap();
        assert_eq!((one.min, one.median, one.max), (7, 7, 7));
    }

    #[test]
    fn a_profile_counts_rooms_links_and_sites() {
        let structures = vec![house(100), house(110), house(300)];
        let uses = vec![Use::Kitchen, Use::Bedroom, Use::Bedroom];
        let guesses = vec![(Class::House, uses.clone()); 3];
        // The first two share a site.
        let p = profiles(&structures, &guesses, &[0, 0, 2]);
        assert_eq!(p.len(), 1);
        let p = &p[0];
        assert_eq!((p.class, p.structures, p.on_sites), (Class::House, 3, 2));
        assert_eq!(p.rooms.median, 3);
        assert_eq!(p.entrances.max, 2);
        assert_eq!(p.uses[0].use_, Use::Bedroom);
        assert_eq!(p.uses[0].rooms, 6);
        let kitchen = p.uses.iter().find(|u| u.use_ == Use::Kitchen).unwrap();
        assert_eq!((kitchen.entrances, kitchen.ground), (3, 3));
        let bed = &p.uses[0].furnishings[0];
        assert_eq!((bed.category, bed.rooms, bed.count), (Category::Bed, 3, 6));
        let link = |via| p.links.iter().find(|l| l.via == via).unwrap();
        assert_eq!(
            (
                link(Via::Door).from,
                link(Via::Door).to,
                link(Via::Door).count
            ),
            (Use::Bedroom, Use::Kitchen, 3)
        );
        assert_eq!(
            (link(Via::Above).from, link(Via::Above).to),
            (Use::Kitchen, Use::Bedroom)
        );
        assert_eq!(
            p.site_classes,
            vec![Count {
                what: Class::House,
                count: 2
            }]
        );
        assert_eq!(p.site_gap.unwrap().max, 2);
    }
}
