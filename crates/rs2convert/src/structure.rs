//! Whole structures found in the map: buildings with their rooms, doors,
//! walls, roofs and everything placed inside them.
//!
//! The map doesn't say where a building is, but it says which tiles are
//! under a roof: tile flag 4, which the original client reads to hide the
//! planes above when the camera is over such a tile. A room is a patch of
//! roofed tiles on one plane that can reach each other without crossing a
//! wall. Walls lie on tile edges: a straight wall (type 0) on one edge, a
//! corner wall (type 2) on two, as the original client blocks movement for
//! them. Corner posts (types 1 and 3) and diagonal walls (type 9) block no
//! edge, so they don't split rooms.
//!
//! Rooms join into a structure through the doors between them (a wall
//! whose definition is a door or gate, or can be opened), through
//! overlapping each other on neighbouring planes, and, for a room with no
//! door and nothing above or below, through the wall it shares most with
//! another room. Everything placed on a structure's tiles belongs to it:
//! its walls, doors and wall decorations by the edges and tiles they're
//! on, its roofs by the tiles under them, and its furnishings and floor
//! decorations by the room they stand in.
//!
//! What a structure is, and what each room is for, isn't decided here.
//! `docs/conversion/structures.md` has the method and what it finds.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rs2cache::types::config::LocDef;
use rs2cache::types::map::{Location, Plane, Region};
use serde::{Deserialize, Serialize};

/// Tiles along a region's side.
pub const REGION_SIZE: u16 = 64;

/// The tile flag for a tile under a roof.
pub const ROOFED: u8 = 4;

/// A tile in world coordinates: x east and y north from the map's
/// south-west corner, so a region's tiles start at `region x · 64`.
pub type Tile = (u16, u16);

/// One structure: its rooms and what's placed in and on it. Positions are
/// relative to [`Structure::origin`], so a blueprint can be put down
/// anywhere.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Structure {
    /// The south-west corner of the box around its rooms, in world tiles.
    pub origin: Tile,
    /// That box's size in tiles, east and north.
    pub size: (u16, u16),
    /// The planes it has rooms on, lowest first.
    pub planes: Vec<u8>,
    /// Its rooms, by plane, then by their south-west-most tile.
    pub rooms: Vec<Room>,
    /// Its doors, with the rooms either side.
    pub doors: Vec<Door>,
    /// The edges where a room meets the outside with no wall: open
    /// doorways, archways, the open front of a stall. Tiles with a corner
    /// post or a diagonal wall have none, since a cut-off corner leaves
    /// their outer edges unwalled.
    pub openings: Vec<Opening>,
    /// Its walls, corner posts and diagonal walls, doors not included.
    pub walls: Vec<Placed>,
    /// Its roof pieces.
    pub roofs: Vec<Placed>,
    /// Everything else in or on it: wall decorations, furnishings and
    /// floor decorations, each with its room (none for a wall decoration
    /// on an outside wall).
    pub objects: Vec<Placed>,
    /// Whether it reaches the edge of the regions it was found in, so it
    /// may go on past them.
    pub clipped: bool,
}

/// One room: roofed tiles on one plane that reach each other without
/// crossing a wall.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Room {
    /// Its plane.
    pub plane: u8,
    /// Its tiles, sorted by x then y.
    pub tiles: Vec<RoomTile>,
}

/// One tile of a room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomTile {
    /// East of the structure's origin.
    pub x: u16,
    /// North of the structure's origin.
    pub y: u16,
    /// Overlay floor: 0 for none, otherwise `flo` id + 1, as the map
    /// stores it.
    pub overlay: u8,
    /// Underlay floor, the same way.
    pub underlay: u8,
    /// The height of the tile's south-west corner above the structure's
    /// lowest one, in client units (240 to a plane), up positive.
    pub height: i32,
}

/// A door: a wall that can be opened, with what's on each side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Door {
    /// The door itself.
    pub placed: Placed,
    /// Whether the map places it open. An open door stands a quarter turn
    /// from its doorway, so it walls no edge, and its doorway shows as an
    /// [`Opening`] (or joins the rooms either side into one).
    pub open: bool,
    /// The rooms on the two sides of the edge it lies on, as indices into
    /// [`Structure::rooms`]: west then east for a door on a north–south
    /// edge, south then north for one on an east–west edge. `None` is
    /// outside, which makes a closed door an entrance.
    pub rooms: [Option<usize>; 2],
}

/// An edge where a room meets the outside with no wall on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Opening {
    /// The room, as an index into [`Structure::rooms`].
    pub room: usize,
    /// Its plane.
    pub plane: u8,
    /// The room's tile on the edge, east of the structure's origin.
    pub x: u16,
    /// North of the origin.
    pub y: u16,
    /// Which edge of that tile: 0 west, 1 north, 2 east, 3 south.
    pub side: u8,
}

/// A placed location, relative to its structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Placed {
    /// The `loc` definition id.
    pub id: u16,
    /// Its name, where the definition has one.
    pub name: Option<String>,
    /// The 317 location type, 0–22.
    pub kind: u8,
    /// Quarter turns, 0–3.
    pub rotation: u8,
    /// Its plane.
    pub plane: u8,
    /// East of the structure's origin; negative just outside it (a roof
    /// overhang, an outside wall's corner post).
    pub x: i32,
    /// North of the structure's origin, the same way.
    pub y: i32,
    /// The room it's in, as an index into [`Structure::rooms`].
    pub room: Option<usize>,
}

/// Which way across a tile edge: an edge is named by the tile south or
/// west of it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Side {
    North,
    East,
}

/// A tile edge on a plane.
type Edge = (u8, u16, u16, Side);

/// A tile on a plane.
type At = (u8, u16, u16);

/// The edge on direction `dir` of tile `(x, y)`: 0 west, 1 north, 2 east,
/// 3 south, the way wall rotations count. `None` off the map's edge.
fn edge(plane: u8, x: u16, y: u16, dir: u8) -> Option<Edge> {
    match dir & 3 {
        0 => x.checked_sub(1).map(|x| (plane, x, y, Side::East)),
        1 => Some((plane, x, y, Side::North)),
        2 => Some((plane, x, y, Side::East)),
        _ => y.checked_sub(1).map(|y| (plane, x, y, Side::North)),
    }
}

/// The two tiles either side of an edge: west then east, or south then
/// north.
fn sides((plane, x, y, side): Edge) -> [At; 2] {
    match side {
        Side::East => [(plane, x, y), (plane, x + 1, y)],
        Side::North => [(plane, x, y), (plane, x, y + 1)],
    }
}

/// The edges a wall of `kind` turned `rotation` quarter turns lies on, as
/// directions (see [`edge`]): one for a straight wall, two for a corner
/// wall, none for posts and diagonals.
fn wall_dirs(kind: u8, rotation: u8) -> Vec<u8> {
    match kind {
        0 => vec![rotation & 3],
        2 => vec![rotation & 3, (rotation + 1) & 3],
        _ => Vec::new(),
    }
}

/// Whether a location type is a wall: types 0–3 and 9.
pub fn is_wall(kind: u8) -> bool {
    matches!(kind, 0..=3 | 9)
}

/// Whether a location type is a roof piece: types 12–21.
pub fn is_roof(kind: u8) -> bool {
    (12..=21).contains(&kind)
}

/// Whether a wall's definition is a door: named a door or gate, or with an
/// option to open or close it.
pub fn is_door(def: &LocDef) -> bool {
    let named = def.name.as_deref().is_some_and(|n| {
        let n = n.to_ascii_lowercase();
        n.contains("door") || n.contains("gate")
    });
    named || has_action(def, "open") || has_action(def, "close")
}

/// Whether a door's definition is the open one: it offers to close and
/// not to open.
pub fn is_open_door(def: &LocDef) -> bool {
    has_action(def, "close") && !has_action(def, "open")
}

fn has_action(def: &LocDef, action: &str) -> bool {
    def.actions
        .iter()
        .flatten()
        .any(|a| a.eq_ignore_ascii_case(action))
}

/// The loaded regions, looked up by world tile.
struct Map<'a> {
    regions: HashMap<(u16, u16), &'a Region>,
}

impl<'a> Map<'a> {
    fn new(regions: &'a [Region]) -> Self {
        Self {
            regions: regions
                .iter()
                .map(|r| ((u16::from(r.x), u16::from(r.y)), r))
                .collect(),
        }
    }

    /// The plane data and index for a tile, if its region is loaded.
    fn plane(&self, plane: u8, x: u16, y: u16) -> Option<(&'a Plane, usize)> {
        let r = self.regions.get(&(x / REGION_SIZE, y / REGION_SIZE))?;
        let p = r.planes.get(usize::from(plane))?;
        Some((
            p,
            usize::from(x % REGION_SIZE) * 64 + usize::from(y % REGION_SIZE),
        ))
    }

    fn loaded(&self, x: u16, y: u16) -> bool {
        self.regions
            .contains_key(&(x / REGION_SIZE, y / REGION_SIZE))
    }

    fn roofed(&self, plane: u8, x: u16, y: u16) -> bool {
        self.plane(plane, x, y)
            .is_some_and(|(p, i)| p.flags.get(i).is_some_and(|f| f & ROOFED != 0))
    }
}

/// A location in world tiles.
#[derive(Debug, Clone, Copy)]
struct World {
    loc: Location,
    x: u16,
    y: u16,
}

/// A small union–find over room indices.
struct Union(Vec<usize>);

impl Union {
    fn find(&mut self, a: usize) -> usize {
        let mut r = a;
        while self.0[r] != r {
            r = self.0[r];
        }
        let mut a = a;
        while self.0[a] != r {
            let next = self.0[a];
            self.0[a] = r;
            a = next;
        }
        r
    }

    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.find(a), self.find(b));
        // The lower index wins, so the result doesn't depend on order.
        if a != b {
            let (lo, hi) = (a.min(b), a.max(b));
            self.0[hi] = lo;
        }
    }
}

/// Finds every structure in `regions`. `defs` holds the definitions of the
/// locations placed there; one that's missing counts as a nameless wall
/// that isn't a door. Structures come sorted by origin (x, then y), then
/// lowest plane.
pub fn find(regions: &[Region], defs: &BTreeMap<u16, LocDef>) -> Vec<Structure> {
    let map = Map::new(regions);
    let mut regions_sorted: Vec<&Region> = regions.iter().collect();
    regions_sorted.sort_by_key(|r| (r.x, r.y));
    let placed: Vec<World> = regions_sorted
        .iter()
        .flat_map(|r| {
            r.locations.iter().map(move |l| World {
                loc: *l,
                x: u16::from(r.x) * REGION_SIZE + u16::from(l.x),
                y: u16::from(r.y) * REGION_SIZE + u16::from(l.y),
            })
        })
        .collect();
    let door = |id: u16| defs.get(&id).is_some_and(is_door);
    let open = |id: u16| defs.get(&id).is_some_and(|d| is_door(d) && is_open_door(d));

    // Every walled edge, and whether a door is on it. An open door walls
    // nothing: it stands beside its doorway.
    let mut edges: BTreeMap<Edge, bool> = BTreeMap::new();
    for w in placed.iter().filter(|w| !open(w.loc.id)) {
        for d in wall_dirs(w.loc.kind, w.loc.rotation) {
            if let Some(e) = edge(w.loc.plane, w.x, w.y, d) {
                *edges.entry(e).or_default() |= door(w.loc.id);
            }
        }
    }

    // Rooms: roofed tiles flood-filled without crossing a walled edge.
    let mut room_of: HashMap<At, usize> = HashMap::new();
    let mut rooms: Vec<(u8, Vec<Tile>)> = Vec::new();
    for r in &regions_sorted {
        for plane in 0..4u8 {
            for lx in 0..REGION_SIZE {
                for ly in 0..REGION_SIZE {
                    let (x, y) = (
                        u16::from(r.x) * REGION_SIZE + lx,
                        u16::from(r.y) * REGION_SIZE + ly,
                    );
                    if !map.roofed(plane, x, y) || room_of.contains_key(&(plane, x, y)) {
                        continue;
                    }
                    let id = rooms.len();
                    let mut tiles = Vec::new();
                    let mut stack = vec![(x, y)];
                    room_of.insert((plane, x, y), id);
                    while let Some((x, y)) = stack.pop() {
                        tiles.push((x, y));
                        for d in 0..4u8 {
                            let Some(e) = edge(plane, x, y, d) else {
                                continue;
                            };
                            if edges.contains_key(&e) {
                                continue;
                            }
                            let [a, b] = sides(e);
                            let (_, nx, ny) = if a == (plane, x, y) { b } else { a };
                            if map.roofed(plane, nx, ny) && !room_of.contains_key(&(plane, nx, ny))
                            {
                                room_of.insert((plane, nx, ny), id);
                                stack.push((nx, ny));
                            }
                        }
                    }
                    tiles.sort_unstable();
                    rooms.push((plane, tiles));
                }
            }
        }
    }

    // Join rooms through doors and across planes.
    let mut union = Union((0..rooms.len()).collect());
    let mut linked = vec![false; rooms.len()];
    for (&e, &is_door) in &edges {
        let [a, b] = sides(e);
        if let (true, Some(&ra), Some(&rb)) = (is_door, room_of.get(&a), room_of.get(&b)) {
            union.join(ra, rb);
        }
        if is_door {
            for t in [a, b] {
                if let Some(&r) = room_of.get(&t) {
                    linked[r] = true;
                }
            }
        }
    }
    for (i, (plane, tiles)) in rooms.iter().enumerate() {
        for &(x, y) in tiles {
            if let Some(&above) = room_of.get(&(plane + 1, x, y)) {
                union.join(i, above);
                linked[i] = true;
                linked[above] = true;
            }
        }
    }
    // A room with no door and nothing above or below joins the room it
    // shares the most wall with.
    for i in 0..rooms.len() {
        if linked[i] {
            continue;
        }
        let mut shared: BTreeMap<usize, usize> = BTreeMap::new();
        let plane = rooms[i].0;
        for &(x, y) in &rooms[i].1 {
            for d in 0..4u8 {
                let Some(e) = edge(plane, x, y, d) else {
                    continue;
                };
                if !edges.contains_key(&e) {
                    continue;
                }
                for t in sides(e) {
                    if let Some(&r) = room_of.get(&t).filter(|&&r| r != i) {
                        *shared.entry(r).or_default() += 1;
                    }
                }
            }
        }
        if let Some((&r, _)) = shared
            .iter()
            .max_by_key(|&(r, n)| (*n, std::cmp::Reverse(*r)))
        {
            union.join(i, r);
        }
    }

    // Group the rooms into structures.
    let mut groups: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..rooms.len() {
        let root = union.find(i);
        groups.entry(root).or_default().push(i);
    }
    let mut structures: Vec<Building> = groups.into_values().map(Building::new).collect();
    let mut structure_of: HashMap<usize, usize> = HashMap::new();
    for (s, b) in structures.iter().enumerate() {
        for &r in &b.members {
            structure_of.insert(r, s);
        }
    }

    // Hand each placed location to the structures it belongs to.
    let in_room = |at: At| room_of.get(&at).copied();
    let structure_at = |at: At| in_room(at).map(|r| structure_of[&r]);
    for w in &placed {
        let (plane, x, y) = (w.loc.plane, w.x, w.y);
        let kind = w.loc.kind;
        if is_wall(kind) {
            let mut owners = BTreeSet::new();
            let dirs = wall_dirs(kind, w.loc.rotation);
            for &d in &dirs {
                if let Some(e) = edge(plane, x, y, d) {
                    owners.extend(sides(e).into_iter().filter_map(structure_at));
                }
            }
            if dirs.is_empty() {
                owners.extend(around(plane, x, y).filter_map(structure_at));
            }
            let is_door = !dirs.is_empty() && door(w.loc.id);
            for s in owners {
                let b = &mut structures[s];
                if is_door {
                    let e = edge(plane, x, y, dirs[0]).expect("a door's edge");
                    let rooms = sides(e).map(|t| in_room(t).filter(|r| structure_of[r] == s));
                    b.doors.push((*w, open(w.loc.id), rooms));
                } else {
                    b.walls.push(*w);
                }
            }
        } else if is_roof(kind) {
            let owners: BTreeSet<usize> = [plane, plane.saturating_sub(1)]
                .into_iter()
                .flat_map(|p| around(p, x, y))
                .filter_map(structure_at)
                .collect();
            for s in owners {
                structures[s].roofs.push(*w);
            }
        } else {
            let (room, owner) = object_room(w, defs, &in_room, &edges);
            if let Some(r) = owner.or(room) {
                let s = structure_of[&r];
                structures[s].objects.push((*w, room));
            }
        }
    }

    // Unwalled edges from a room to the outside. A cut-off corner is a
    // diagonal wall with posts beside it, and the roofed tiles there have
    // no straight wall on their outer edges, so tiles with a post or a
    // diagonal wall open onto nothing.
    let cornered: BTreeSet<At> = placed
        .iter()
        .filter(|w| matches!(w.loc.kind, 1 | 3 | 9))
        .map(|w| (w.loc.plane, w.x, w.y))
        .collect();
    for (r, (plane, tiles)) in rooms.iter().enumerate() {
        for &(x, y) in tiles {
            if cornered.contains(&(*plane, x, y)) {
                continue;
            }
            for d in 0..4u8 {
                let outside = match edge(*plane, x, y, d) {
                    Some(e) if edges.contains_key(&e) => false,
                    Some(e) => {
                        let [a, b] = sides(e);
                        let (_, nx, ny) = if a == (*plane, x, y) { b } else { a };
                        !map.roofed(*plane, nx, ny)
                    }
                    None => true,
                };
                if outside {
                    structures[structure_of[&r]]
                        .openings
                        .push((r, *plane, x, y, d));
                }
            }
        }
    }

    let mut out: Vec<Structure> = structures
        .into_iter()
        .map(|b| b.finish(&rooms, &map, defs))
        .collect();
    out.sort_by(|a, b| {
        (a.origin.0, a.origin.1, a.planes.first()).cmp(&(b.origin.0, b.origin.1, b.planes.first()))
    });
    out
}

/// A tile and its eight neighbours on a plane.
fn around(plane: u8, x: u16, y: u16) -> impl Iterator<Item = At> {
    (-1i32..=1).flat_map(move |dx| {
        (-1i32..=1).filter_map(move |dy| {
            let nx = u16::try_from(i32::from(x) + dx).ok()?;
            let ny = u16::try_from(i32::from(y) + dy).ok()?;
            Some((plane, nx, ny))
        })
    })
}

/// The room an object stands in, and for a wall decoration on an outside
/// wall, the room on the wall's other side that it belongs with (it's in
/// no room itself). Furnishings count as in a room when any tile of their
/// footprint is.
fn object_room(
    w: &World,
    defs: &BTreeMap<u16, LocDef>,
    in_room: &impl Fn(At) -> Option<usize>,
    edges: &BTreeMap<Edge, bool>,
) -> (Option<usize>, Option<usize>) {
    let (plane, x, y) = (w.loc.plane, w.x, w.y);
    if let Some(r) = in_room((plane, x, y)) {
        return (Some(r), None);
    }
    match w.loc.kind {
        // A straight wall decoration hangs on the edge its rotation names;
        // from outside, it belongs with the room across that wall.
        4 | 5 => {
            let across = edge(plane, x, y, w.loc.rotation)
                .filter(|e| edges.contains_key(e))
                .and_then(|e| sides(e).into_iter().find_map(in_room));
            (None, across)
        }
        10 | 11 => {
            let (sx, sy) = defs.get(&w.loc.id).map_or((1, 1), |d| {
                if w.loc.rotation & 1 == 1 {
                    (d.size_y, d.size_x)
                } else {
                    (d.size_x, d.size_y)
                }
            });
            let room = (0..u16::from(sx.max(1)))
                .flat_map(|dx| (0..u16::from(sy.max(1))).map(move |dy| (dx, dy)))
                .find_map(|(dx, dy)| in_room((plane, x + dx, y + dy)));
            (room, None)
        }
        _ => (None, None),
    }
}

/// A structure while it's being put together, in world tiles.
struct Building {
    members: Vec<usize>,
    doors: Vec<(World, bool, [Option<usize>; 2])>,
    openings: Vec<(usize, u8, u16, u16, u8)>,
    walls: Vec<World>,
    roofs: Vec<World>,
    objects: Vec<(World, Option<usize>)>,
}

impl Building {
    fn new(members: Vec<usize>) -> Self {
        Self {
            members,
            doors: Vec::new(),
            openings: Vec::new(),
            walls: Vec::new(),
            roofs: Vec::new(),
            objects: Vec::new(),
        }
    }

    /// Makes positions relative, numbers the rooms in their final order
    /// and sorts everything.
    fn finish(
        self,
        rooms: &[(u8, Vec<Tile>)],
        map: &Map,
        defs: &BTreeMap<u16, LocDef>,
    ) -> Structure {
        let mut members = self.members;
        // By plane, then south-west-most tile.
        members.sort_by_key(|&r| (rooms[r].0, rooms[r].1.first().copied()));
        let index: HashMap<usize, usize> =
            members.iter().enumerate().map(|(i, &r)| (r, i)).collect();
        let tiles = || {
            members
                .iter()
                .flat_map(|&r| rooms[r].1.iter().map(move |t| (rooms[r].0, *t)))
        };
        let (x0, y0) = tiles().fold((u16::MAX, u16::MAX), |(a, b), (_, (x, y))| {
            (a.min(x), b.min(y))
        });
        let (x1, y1) = tiles().fold((0, 0), |(a, b), (_, (x, y))| (a.max(x), b.max(y)));
        let planes: Vec<u8> = tiles()
            .map(|(p, _)| p)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let height = |p: u8, x: u16, y: u16| {
            map.plane(p, x, y)
                .and_then(|(pl, i)| pl.heights.get(i).copied())
                .unwrap_or(0)
        };
        // The lowest corner is the largest height: y is down.
        let base = tiles()
            .map(|(p, (x, y))| height(p, x, y))
            .max()
            .unwrap_or(0);
        let clipped = tiles().any(|(_, (x, y))| {
            [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().any(|&(dx, dy)| {
                match (
                    u16::try_from(i32::from(x) + dx),
                    u16::try_from(i32::from(y) + dy),
                ) {
                    (Ok(nx), Ok(ny)) => !map.loaded(nx, ny),
                    _ => true,
                }
            })
        });
        let room_list = members
            .iter()
            .map(|&r| {
                let (plane, ts) = &rooms[r];
                Room {
                    plane: *plane,
                    tiles: ts
                        .iter()
                        .map(|&(x, y)| {
                            let (overlay, underlay) = map
                                .plane(*plane, x, y)
                                .map_or((0, 0), |(p, i)| (p.overlays[i], p.underlays[i]));
                            RoomTile {
                                x: x - x0,
                                y: y - y0,
                                overlay,
                                underlay,
                                height: base - height(*plane, x, y),
                            }
                        })
                        .collect(),
                }
            })
            .collect();
        let placed = |w: &World, room: Option<usize>| Placed {
            id: w.loc.id,
            name: defs.get(&w.loc.id).and_then(|d| d.name.clone()),
            kind: w.loc.kind,
            rotation: w.loc.rotation,
            plane: w.loc.plane,
            x: i32::from(w.x) - i32::from(x0),
            y: i32::from(w.y) - i32::from(y0),
            room: room.and_then(|r| index.get(&r).copied()),
        };
        let key = |p: &Placed| (p.plane, p.x, p.y, p.kind, p.rotation, p.id);
        let mut doors: Vec<Door> = self
            .doors
            .iter()
            .map(|(w, open, rs)| Door {
                placed: placed(w, None),
                open: *open,
                rooms: rs.map(|r| r.and_then(|r| index.get(&r).copied())),
            })
            .collect();
        doors.sort_by_key(|d| key(&d.placed));
        let mut openings: Vec<Opening> = self
            .openings
            .iter()
            .map(|&(r, plane, x, y, side)| Opening {
                room: index[&r],
                plane,
                x: x - x0,
                y: y - y0,
                side,
            })
            .collect();
        openings.sort_by_key(|o| (o.plane, o.x, o.y, o.side));
        let sorted = |v: Vec<Placed>| {
            let mut v = v;
            v.sort_by_key(key);
            v
        };
        Structure {
            origin: (x0, y0),
            size: (x1 - x0 + 1, y1 - y0 + 1),
            planes,
            rooms: room_list,
            doors,
            openings,
            walls: sorted(self.walls.iter().map(|w| placed(w, None)).collect()),
            roofs: sorted(self.roofs.iter().map(|w| placed(w, None)).collect()),
            objects: sorted(self.objects.iter().map(|(w, r)| placed(w, *r)).collect()),
            clipped,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rs2cache::config::ConfigType;
    use rs2cache::config::loc::Loc;
    use rs2cache::io::Cursor;

    const WALL: u16 = 1;
    const DOOR: u16 = 2;
    const BED: u16 = 3;
    const BANNER: u16 = 4;
    const ROOF: u16 = 5;

    /// A definition with just a name.
    fn named(id: u16, name: &str) -> LocDef {
        let mut bytes = vec![2];
        bytes.extend_from_slice(name.as_bytes());
        bytes.extend_from_slice(b"\n\0");
        Loc::decode(u32::from(id), &mut Cursor::new(&bytes)).unwrap()
    }

    fn defs() -> BTreeMap<u16, LocDef> {
        [
            (WALL, "Wall"),
            (DOOR, "Door"),
            (BED, "Bed"),
            (BANNER, "Banner"),
            (ROOF, "Roof"),
        ]
        .into_iter()
        .map(|(id, n)| (id, named(id, n)))
        .collect()
    }

    /// An empty region 0_0.
    fn region() -> Region {
        let plane = || Plane {
            heights: vec![0; 4096],
            height_derived: vec![false; 4096],
            overlays: vec![0; 4096],
            shapes: vec![0; 4096],
            rotations: vec![0; 4096],
            flags: vec![0; 4096],
            underlays: vec![0; 4096],
        };
        Region {
            id: 0,
            x: 0,
            y: 0,
            members: false,
            planes: (0..4).map(|_| plane()).collect(),
            locations: Vec::new(),
        }
    }

    fn put(r: &mut Region, id: u16, plane: u8, x: u8, y: u8, kind: u8, rotation: u8) {
        r.locations.push(Location {
            id,
            plane,
            x,
            y,
            kind,
            rotation,
        });
    }

    /// Roofs tiles `x0..=x1` × `y0..=y1` on `plane` and walls them in,
    /// with `door` (a tile on the south side) as the door.
    fn house(r: &mut Region, plane: u8, (x0, y0): (u8, u8), (x1, y1): (u8, u8), door: Option<u8>) {
        for x in x0..=x1 {
            for y in y0..=y1 {
                r.planes[usize::from(plane)].flags[usize::from(x) * 64 + usize::from(y)] |= ROOFED;
            }
            put(r, WALL, plane, x, y1, 0, 1);
            let south = if door == Some(x) { DOOR } else { WALL };
            put(r, south, plane, x, y0, 0, 3);
        }
        for y in y0..=y1 {
            put(r, WALL, plane, x0, y, 0, 0);
            put(r, WALL, plane, x1, y, 0, 2);
        }
    }

    #[test]
    fn a_house_with_two_rooms_and_its_contents() {
        let mut r = region();
        house(&mut r, 0, (10, 10), (15, 12), Some(11));
        // An inside wall between x = 12 and 13, with a door in the middle.
        put(&mut r, WALL, 0, 12, 10, 0, 2);
        put(&mut r, DOOR, 0, 12, 11, 0, 2);
        put(&mut r, WALL, 0, 12, 12, 0, 2);
        put(&mut r, BED, 0, 14, 12, 10, 0);
        // A banner on the outside of the west wall, hanging on its east edge.
        put(&mut r, BANNER, 0, 9, 11, 4, 2);
        put(&mut r, ROOF, 1, 12, 11, 12, 0);
        let s = find(&[r], &defs());
        assert_eq!(s.len(), 1);
        let s = &s[0];
        assert_eq!(
            (s.origin, s.size, s.planes.clone()),
            ((10, 10), (6, 3), vec![0])
        );
        assert_eq!(s.rooms.len(), 2);
        assert_eq!(
            s.rooms.iter().map(|r| r.tiles.len()).collect::<Vec<_>>(),
            [9, 9]
        );
        // The entrance faces outside on its south side.
        let entrance = s.doors.iter().find(|d| d.rooms[0].is_none()).unwrap();
        assert_eq!((entrance.placed.x, entrance.placed.y), (1, 0));
        assert_eq!(entrance.rooms[1], Some(0));
        let inner = s.doors.iter().find(|d| d.rooms[0].is_some()).unwrap();
        assert_eq!(inner.rooms, [Some(0), Some(1)]);
        let bed = s.objects.iter().find(|o| o.id == BED).unwrap();
        assert_eq!((bed.x, bed.y, bed.room), (4, 2, Some(1)));
        let banner = s.objects.iter().find(|o| o.id == BANNER).unwrap();
        assert_eq!((banner.x, banner.room), (-1, None));
        assert_eq!(s.roofs.len(), 1);
        assert_eq!(s.walls.iter().filter(|w| w.id == DOOR).count(), 0);
        assert!(!s.clipped);
    }

    #[test]
    fn a_shared_wall_without_a_door_keeps_houses_apart() {
        let mut r = region();
        house(&mut r, 0, (10, 10), (12, 12), Some(11));
        house(&mut r, 0, (13, 10), (15, 12), Some(14));
        let s = find(&[r], &defs());
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].origin, s[1].origin), ((10, 10), (13, 10)));
    }

    #[test]
    fn an_upper_floor_joins_its_building() {
        let mut r = region();
        house(&mut r, 0, (10, 10), (12, 12), Some(11));
        house(&mut r, 1, (10, 10), (12, 12), None);
        let s = find(&[r], &defs());
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].planes, [0, 1]);
        assert_eq!(
            s[0].rooms.iter().map(|r| r.plane).collect::<Vec<_>>(),
            [0, 1]
        );
    }

    #[test]
    fn a_closet_without_a_door_joins_the_room_beside_it() {
        let mut r = region();
        house(&mut r, 0, (10, 10), (12, 12), Some(11));
        house(&mut r, 0, (13, 11), (13, 12), None);
        let s = find(&[r], &defs());
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].rooms.len(), 2);
    }

    #[test]
    fn a_house_across_a_region_boundary_comes_out_whole() {
        // Tiles 62–65 east: the west half in region 0_0, the east half in
        // region 1_0, each half's walls placed in its own region, and the
        // door in the eastern one.
        let mut west = region();
        let mut east = Region {
            id: 1 << 8,
            x: 1,
            ..region()
        };
        for y in 10..=12u8 {
            for x in [62u8, 63] {
                west.planes[0].flags[usize::from(x) * 64 + usize::from(y)] |= ROOFED;
            }
            for x in [0u8, 1] {
                east.planes[0].flags[usize::from(x) * 64 + usize::from(y)] |= ROOFED;
            }
            put(&mut west, WALL, 0, 62, y, 0, 0);
            put(&mut east, WALL, 0, 1, y, 0, 2);
        }
        for x in [62u8, 63] {
            put(&mut west, WALL, 0, x, 12, 0, 1);
            put(&mut west, WALL, 0, x, 10, 0, 3);
        }
        put(&mut east, WALL, 0, 0, 12, 0, 1);
        put(&mut east, DOOR, 0, 0, 10, 0, 3);
        put(&mut east, WALL, 0, 1, 12, 0, 1);
        put(&mut east, WALL, 0, 1, 10, 0, 3);
        put(&mut east, BED, 0, 1, 11, 10, 0);
        let both = find(&[west.clone(), east.clone()], &defs());
        assert_eq!(both.len(), 1);
        let s = &both[0];
        assert_eq!((s.origin, s.size, s.rooms.len()), ((62, 10), (4, 3), 1));
        assert_eq!(s.doors.len(), 1);
        assert_eq!(
            (s.doors[0].placed.x, s.doors[0].rooms),
            (2, [None, Some(0)])
        );
        assert_eq!(s.objects[0].x, 3);
        assert_eq!(s.walls.len(), 13);
        assert!(!s.clipped);
        // Alone, the western half is cut off at the boundary.
        let alone = find(&[west], &defs());
        assert_eq!((alone.len(), alone[0].size), (1, (2, 3)));
        assert!(alone[0].clipped);
    }

    #[test]
    fn walls_lie_on_the_edges_the_original_client_blocks() {
        // Straight walls: west, north, east, south.
        assert_eq!(
            edge(0, 5, 5, wall_dirs(0, 0)[0]),
            Some((0, 4, 5, Side::East))
        );
        assert_eq!(
            edge(0, 5, 5, wall_dirs(0, 1)[0]),
            Some((0, 5, 5, Side::North))
        );
        assert_eq!(
            edge(0, 5, 5, wall_dirs(0, 2)[0]),
            Some((0, 5, 5, Side::East))
        );
        assert_eq!(
            edge(0, 5, 5, wall_dirs(0, 3)[0]),
            Some((0, 5, 4, Side::North))
        );
        // A corner wall turned once covers north and east.
        assert_eq!(wall_dirs(2, 1), [1, 2]);
        // Posts and diagonals cover none.
        assert!(wall_dirs(1, 0).is_empty() && wall_dirs(9, 0).is_empty());
    }
}
