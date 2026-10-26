//! `structures` records: blueprints, sites and class profiles.
//!
//! A `structures` pack holds three kinds of record, told apart by their
//! magic and by their ids' top bits ([`site_id`], [`profile_id`]):
//!
//! - a [`Blueprint`] (`RBLU`): one structure's rooms, tiles, doors,
//!   openings, walls, roofs and objects, placed from its north-west corner;
//! - a [`Site`] (`RSIT`): the blueprints that belong together, placed from
//!   the site's north-west corner;
//! - a [`Profile`] (`RPRF`): what the structures of one class are like.
//!
//! Every record starts with its magic and version byte [`VERSION`].
//! Positions are tiles in pack axes: x east and z south, from the north-west
//! corner of the blueprint's (or site's) box, since pack space negates the
//! cache's north. Sides of a tile are named by compass: 0 west, 1 north,
//! 2 east, 3 south. Class, use, category, material and via values are codes
//! whose names the pack's manifest lists (`docs/pack/structures.md`).
//!
//! ```text
//! dressing  { material u8 (255 = none), materialRef u32, colour u32 }
//! piece     { object u32, category u8, icon u16, kind u8, rotation u8, plane u8,
//!             x i16, z i16, room u16, dressing }
//! spread    { min u16, low u16, median u16, high u16, max u16 }
//! ```

use crate::bytes::{Reader, Writer};

use super::{RecordError, finish, header};

/// Version byte of every `structures` record.
pub const VERSION: u8 = 1;
/// Magic at the start of a blueprint record.
pub const BLUEPRINT_MAGIC: &[u8; 4] = b"RBLU";
/// Magic at the start of a site record.
pub const SITE_MAGIC: &[u8; 4] = b"RSIT";
/// Magic at the start of a profile record.
pub const PROFILE_MAGIC: &[u8; 4] = b"RPRF";

/// A missing `u32` reference.
pub const NONE: u32 = u32::MAX;
/// A missing `u16` index (a room, an icon) or "outside" for a door's side.
pub const NONE16: u16 = u16::MAX;
/// A missing `u8` code (a category, a material).
pub const NONE8: u8 = u8::MAX;

/// Top bits of a site's record id.
pub const SITE_BASE: u32 = 0x4000_0000;
/// Top bits of a profile's record id.
pub const PROFILE_BASE: u32 = 0x8000_0000;

/// A blueprint's record id: its lowest plane and its origin (south-west
/// corner) in world tiles, `plane << 28 | x << 14 | y`. `None` when the
/// plane is over 3 or a coordinate over 16383.
pub fn blueprint_id(plane: u8, x: u16, y: u16) -> Option<u32> {
    (plane < 4 && x < 1 << 14 && y < 1 << 14)
        .then(|| u32::from(plane) << 28 | u32::from(x) << 14 | u32::from(y))
}

/// A site's record id, from its first blueprint's.
pub fn site_id(first: u32) -> u32 {
    SITE_BASE | first
}

/// A class profile's record id.
pub fn profile_id(class: u8) -> u32 {
    PROFILE_BASE | u32::from(class)
}

/// What a wall, roof piece or floor is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dressing {
    /// Material code, or [`NONE8`] for nothing known.
    pub material: u8,
    /// The `materials` record it's drawn with (`1 + texture`), or [`NONE`]
    /// when it's plain.
    pub material_ref: u32,
    /// Its colour, `0xRRGGBB`, when it's plain; otherwise [`NONE`].
    pub colour: u32,
}

impl Dressing {
    /// Nothing known.
    pub const NONE: Self = Self {
        material: NONE8,
        material_ref: NONE,
        colour: NONE,
    };
}

/// One placed location: a wall, door, roof piece or object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Piece {
    /// Its `objects` record id (the location definition id).
    pub object: u32,
    /// Its furnishing category code, or [`NONE8`].
    pub category: u8,
    /// The minimap icon it draws, or [`NONE16`].
    pub icon: u16,
    /// The 317 location type, 0–22.
    pub kind: u8,
    /// Quarter turns clockwise seen from above, 0–3, as the map places it.
    pub rotation: u8,
    /// Its plane.
    pub plane: u8,
    /// Tiles east of the blueprint's north-west corner; negative just
    /// outside it.
    pub x: i16,
    /// Tiles south of it, the same way.
    pub z: i16,
    /// The room it's in, or [`NONE16`].
    pub room: u16,
    /// What it's made of.
    pub dressing: Dressing,
}

/// One tile of a room.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tile {
    /// Tiles east of the blueprint's north-west corner.
    pub x: u16,
    /// Tiles south of it.
    pub z: u16,
    /// Overlay floor: 0 for none, otherwise `flo` id + 1.
    pub overlay: u8,
    /// Underlay floor, the same way.
    pub underlay: u8,
    /// The height of its corner above the blueprint's lowest, in client
    /// units (240 to a plane), up positive.
    pub height: i16,
}

/// One room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Room {
    /// Its plane.
    pub plane: u8,
    /// Whether it's roofed; a yard isn't.
    pub roofed: bool,
    /// Its use code.
    pub use_: u8,
    /// What its floor is made of.
    pub floor: Dressing,
    /// Its tiles.
    pub tiles: Vec<Tile>,
}

/// A door and the rooms either side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Door {
    /// The door itself.
    pub piece: Piece,
    /// Whether the map places it open.
    pub open: bool,
    /// The rooms either side, west then east or south then north, each
    /// [`NONE16`] for outside.
    pub rooms: [u16; 2],
}

/// An edge where a room meets the outside with no wall.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Opening {
    /// The room.
    pub room: u16,
    /// Its plane.
    pub plane: u8,
    /// The room's tile on the edge, east of the corner.
    pub x: u16,
    /// South of the corner.
    pub z: u16,
    /// Which side of the tile: 0 west, 1 north, 2 east, 3 south.
    pub side: u8,
}

/// One structure, ready to put down anywhere.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blueprint {
    /// Its box's size in tiles, east–west.
    pub width: u16,
    /// North–south.
    pub depth: u16,
    /// The planes it has rooms on, lowest first.
    pub planes: Vec<u8>,
    /// Its class code.
    pub class: u8,
    /// Its site's record id, or [`NONE`] when it stands alone.
    pub site: u32,
    /// Whether it reached the edge of what was searched.
    pub clipped: bool,
    /// Its rooms.
    pub rooms: Vec<Room>,
    /// Its doors.
    pub doors: Vec<Door>,
    /// Its openings.
    pub openings: Vec<Opening>,
    /// Its walls, corner posts and diagonal walls.
    pub walls: Vec<Piece>,
    /// Its roof pieces.
    pub roofs: Vec<Piece>,
    /// Everything else in or on it.
    pub objects: Vec<Piece>,
}

/// One member of a site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Member {
    /// Its blueprint's record id.
    pub blueprint: u32,
    /// Tiles east of the site's north-west corner to the blueprint's.
    pub x: u16,
    /// Tiles south.
    pub z: u16,
}

/// Blueprints that belong together.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Site {
    /// The size of the box round them all, east–west.
    pub width: u16,
    /// North–south.
    pub depth: u16,
    /// Its members, by blueprint id.
    pub members: Vec<Member>,
}

/// How a number spreads: the least, the quartiles and the most.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spread(pub [u16; 5]);

/// A code and how often it comes up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Count {
    /// The code.
    pub code: u8,
    /// How many.
    pub count: u32,
}

/// A furnishing category in rooms of one use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Furnishing {
    /// The category code.
    pub category: u8,
    /// How many rooms have at least one.
    pub rooms: u16,
    /// How many in all.
    pub count: u32,
}

/// The rooms of one use in a class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseProfile {
    /// The use code.
    pub use_: u8,
    /// How many rooms.
    pub rooms: u16,
    /// How many structures have one.
    pub structures: u16,
    /// Their sizes in tiles.
    pub tiles: Spread,
    /// How many are on the lowest plane.
    pub ground: u16,
    /// How many are roofed.
    pub roofed: u16,
    /// How many lead outside.
    pub entrances: u16,
    /// What's in them.
    pub furnishings: Vec<Furnishing>,
}

/// Rooms of one use connected to rooms of another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Link {
    /// One use code.
    pub from: u8,
    /// The other.
    pub to: u8,
    /// How they connect: 0 a door, 1 `to` on the plane above `from`.
    pub via: u8,
    /// How many pairs of rooms.
    pub count: u16,
}

/// What the structures of one class are like.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// The class code.
    pub class: u8,
    /// How many structures.
    pub structures: u16,
    /// Width, depth, planes, rooms, yards and ways out, in that order.
    pub sizes: [Spread; 6],
    /// Rooms by use.
    pub uses: Vec<UseProfile>,
    /// How rooms connect.
    pub links: Vec<Link>,
    /// Floor materials, by tiles.
    pub floors: Vec<Count>,
    /// Wall materials, by pieces.
    pub walls: Vec<Count>,
    /// Roof materials, by pieces.
    pub roofs: Vec<Count>,
    /// Outside furnishings, by category.
    pub outside: Vec<Count>,
    /// How many share a site.
    pub on_sites: u16,
    /// Classes they share a site with.
    pub site_classes: Vec<Count>,
    /// The gap to the nearest other member of their site, if any are on
    /// one.
    pub site_gap: Option<Spread>,
}

/// Any record of a `structures` pack, by its magic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructureRecord {
    /// A blueprint.
    Blueprint(Blueprint),
    /// A site.
    Site(Site),
    /// A profile.
    Profile(Profile),
}

impl StructureRecord {
    /// Decode whichever record `data` holds.
    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        match data.get(..4) {
            Some(m) if m == SITE_MAGIC => Site::decode(data).map(Self::Site),
            Some(m) if m == PROFILE_MAGIC => Profile::decode(data).map(Self::Profile),
            _ => Blueprint::decode(data).map(Self::Blueprint),
        }
    }
}

/// A list's length as a `u16` or `u8` count, or an error naming it.
fn count<T: TryFrom<usize>>(n: usize, what: &str) -> Result<T, RecordError> {
    T::try_from(n).map_err(|_| RecordError::Invalid(format!("too many {what}: {n}")))
}

fn put_dressing(w: &mut Writer, d: &Dressing) {
    w.u8(d.material).u32(d.material_ref).u32(d.colour);
}

fn get_dressing(r: &mut Reader) -> Result<Dressing, RecordError> {
    Ok(Dressing {
        material: r.u8()?,
        material_ref: r.u32()?,
        colour: r.u32()?,
    })
}

fn put_piece(w: &mut Writer, p: &Piece) {
    w.u32(p.object)
        .u8(p.category)
        .u16(p.icon)
        .u8(p.kind)
        .u8(p.rotation)
        .u8(p.plane)
        .i16(p.x)
        .i16(p.z)
        .u16(p.room);
    put_dressing(w, &p.dressing);
}

fn get_piece(r: &mut Reader) -> Result<Piece, RecordError> {
    Ok(Piece {
        object: r.u32()?,
        category: r.u8()?,
        icon: r.u16()?,
        kind: r.u8()?,
        rotation: r.u8()?,
        plane: r.u8()?,
        x: r.i16()?,
        z: r.i16()?,
        room: r.u16()?,
        dressing: get_dressing(r)?,
    })
}

fn put_pieces(w: &mut Writer, pieces: &[Piece], what: &str) -> Result<(), RecordError> {
    w.u16(count(pieces.len(), what)?);
    for p in pieces {
        put_piece(w, p);
    }
    Ok(())
}

fn get_pieces(r: &mut Reader) -> Result<Vec<Piece>, RecordError> {
    (0..r.u16()?).map(|_| get_piece(r)).collect()
}

fn put_spread(w: &mut Writer, s: &Spread) {
    for v in s.0 {
        w.u16(v);
    }
}

fn get_spread(r: &mut Reader) -> Result<Spread, RecordError> {
    Ok(Spread([r.u16()?, r.u16()?, r.u16()?, r.u16()?, r.u16()?]))
}

fn put_counts(w: &mut Writer, counts: &[Count], what: &str) -> Result<(), RecordError> {
    w.u8(count(counts.len(), what)?);
    for c in counts {
        w.u8(c.code).u32(c.count);
    }
    Ok(())
}

fn get_counts(r: &mut Reader) -> Result<Vec<Count>, RecordError> {
    (0..r.u8()?)
        .map(|_| {
            Ok(Count {
                code: r.u8()?,
                count: r.u32()?,
            })
        })
        .collect()
}

impl Blueprint {
    fn check(&self) -> Result<(), RecordError> {
        let rooms = self.rooms.len();
        let room_ok = |i: u16| i == NONE16 || usize::from(i) < rooms;
        let pieces = self.walls.iter().chain(&self.roofs).chain(&self.objects);
        let bad = self
            .doors
            .iter()
            .any(|d| !d.rooms.iter().all(|&i| room_ok(i)))
            || self.openings.iter().any(|o| usize::from(o.room) >= rooms)
            || pieces
                .chain(self.doors.iter().map(|d| &d.piece))
                .any(|p| !room_ok(p.room));
        if bad {
            return Err(RecordError::Invalid("a room index out of range".into()));
        }
        Ok(())
    }

    /// Encode the record, after checking its room indices.
    pub fn encode(&self) -> Result<Vec<u8>, RecordError> {
        self.check()?;
        let mut w = Writer::default();
        w.bytes(BLUEPRINT_MAGIC)
            .u8(VERSION)
            .u16(self.width)
            .u16(self.depth);
        w.u8(count(self.planes.len(), "planes")?)
            .bytes(&self.planes);
        w.u8(self.class).u32(self.site).u8(u8::from(self.clipped));
        w.u16(count(self.rooms.len(), "rooms")?);
        for room in &self.rooms {
            w.u8(room.plane).u8(u8::from(room.roofed)).u8(room.use_);
            put_dressing(&mut w, &room.floor);
            w.u16(count(room.tiles.len(), "tiles")?);
            for t in &room.tiles {
                w.u16(t.x)
                    .u16(t.z)
                    .u8(t.overlay)
                    .u8(t.underlay)
                    .i16(t.height);
            }
        }
        w.u16(count(self.doors.len(), "doors")?);
        for d in &self.doors {
            put_piece(&mut w, &d.piece);
            w.u8(u8::from(d.open)).u16(d.rooms[0]).u16(d.rooms[1]);
        }
        w.u16(count(self.openings.len(), "openings")?);
        for o in &self.openings {
            w.u16(o.room).u8(o.plane).u16(o.x).u16(o.z).u8(o.side);
        }
        put_pieces(&mut w, &self.walls, "walls")?;
        put_pieces(&mut w, &self.roofs, "roof pieces")?;
        put_pieces(&mut w, &self.objects, "objects")?;
        Ok(w.0)
    }

    /// Decode a record and check its room indices.
    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader::new(data);
        header(&mut r, BLUEPRINT_MAGIC, &[VERSION])?;
        let (width, depth) = (r.u16()?, r.u16()?);
        let n = r.u8()?;
        let planes = r.bytes(n.into())?.to_vec();
        let (class, site, clipped) = (r.u8()?, r.u32()?, r.u8()? != 0);
        let rooms = (0..r.u16()?)
            .map(|_| {
                let (plane, roofed, use_) = (r.u8()?, r.u8()? != 0, r.u8()?);
                let floor = get_dressing(&mut r)?;
                let tiles = (0..r.u16()?)
                    .map(|_| {
                        Ok(Tile {
                            x: r.u16()?,
                            z: r.u16()?,
                            overlay: r.u8()?,
                            underlay: r.u8()?,
                            height: r.i16()?,
                        })
                    })
                    .collect::<Result<_, RecordError>>()?;
                Ok(Room {
                    plane,
                    roofed,
                    use_,
                    floor,
                    tiles,
                })
            })
            .collect::<Result<_, RecordError>>()?;
        let doors = (0..r.u16()?)
            .map(|_| {
                Ok(Door {
                    piece: get_piece(&mut r)?,
                    open: r.u8()? != 0,
                    rooms: [r.u16()?, r.u16()?],
                })
            })
            .collect::<Result<_, RecordError>>()?;
        let openings = (0..r.u16()?)
            .map(|_| {
                Ok(Opening {
                    room: r.u16()?,
                    plane: r.u8()?,
                    x: r.u16()?,
                    z: r.u16()?,
                    side: r.u8()?,
                })
            })
            .collect::<Result<_, RecordError>>()?;
        let walls = get_pieces(&mut r)?;
        let roofs = get_pieces(&mut r)?;
        let objects = get_pieces(&mut r)?;
        finish(&r)?;
        let b = Self {
            width,
            depth,
            planes,
            class,
            site,
            clipped,
            rooms,
            doors,
            openings,
            walls,
            roofs,
            objects,
        };
        b.check()?;
        Ok(b)
    }
}

impl Site {
    fn check(&self) -> Result<(), RecordError> {
        if !self.members.is_sorted_by_key(|m| m.blueprint) {
            return Err(RecordError::Invalid(
                "members not in blueprint order".into(),
            ));
        }
        Ok(())
    }

    /// Encode the record, after checking its members are in order.
    pub fn encode(&self) -> Result<Vec<u8>, RecordError> {
        self.check()?;
        let mut w = Writer::default();
        w.bytes(SITE_MAGIC)
            .u8(VERSION)
            .u16(self.width)
            .u16(self.depth);
        w.u16(count(self.members.len(), "members")?);
        for m in &self.members {
            w.u32(m.blueprint).u16(m.x).u16(m.z);
        }
        Ok(w.0)
    }

    /// Decode a record and check its members are in order.
    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader::new(data);
        header(&mut r, SITE_MAGIC, &[VERSION])?;
        let (width, depth) = (r.u16()?, r.u16()?);
        let members = (0..r.u16()?)
            .map(|_| {
                Ok(Member {
                    blueprint: r.u32()?,
                    x: r.u16()?,
                    z: r.u16()?,
                })
            })
            .collect::<Result<_, RecordError>>()?;
        finish(&r)?;
        let s = Self {
            width,
            depth,
            members,
        };
        s.check()?;
        Ok(s)
    }
}

impl Profile {
    /// Encode the record.
    pub fn encode(&self) -> Result<Vec<u8>, RecordError> {
        let mut w = Writer::default();
        w.bytes(PROFILE_MAGIC)
            .u8(VERSION)
            .u8(self.class)
            .u16(self.structures);
        for s in &self.sizes {
            put_spread(&mut w, s);
        }
        w.u8(count(self.uses.len(), "uses")?);
        for u in &self.uses {
            w.u8(u.use_).u16(u.rooms).u16(u.structures);
            put_spread(&mut w, &u.tiles);
            w.u16(u.ground).u16(u.roofed).u16(u.entrances);
            w.u8(count(u.furnishings.len(), "furnishings")?);
            for f in &u.furnishings {
                w.u8(f.category).u16(f.rooms).u32(f.count);
            }
        }
        w.u16(count(self.links.len(), "links")?);
        for l in &self.links {
            w.u8(l.from).u8(l.to).u8(l.via).u16(l.count);
        }
        put_counts(&mut w, &self.floors, "floor materials")?;
        put_counts(&mut w, &self.walls, "wall materials")?;
        put_counts(&mut w, &self.roofs, "roof materials")?;
        put_counts(&mut w, &self.outside, "outside categories")?;
        w.u16(self.on_sites);
        put_counts(&mut w, &self.site_classes, "site classes")?;
        match &self.site_gap {
            Some(s) => {
                w.u8(1);
                put_spread(&mut w, s);
            }
            None => {
                w.u8(0);
            }
        }
        Ok(w.0)
    }

    /// Decode a record.
    pub fn decode(data: &[u8]) -> Result<Self, RecordError> {
        let mut r = Reader::new(data);
        header(&mut r, PROFILE_MAGIC, &[VERSION])?;
        let (class, structures) = (r.u8()?, r.u16()?);
        let mut sizes = [Spread([0; 5]); 6];
        for s in &mut sizes {
            *s = get_spread(&mut r)?;
        }
        let uses = (0..r.u8()?)
            .map(|_| {
                let (use_, rooms, structures) = (r.u8()?, r.u16()?, r.u16()?);
                let tiles = get_spread(&mut r)?;
                let (ground, roofed, entrances) = (r.u16()?, r.u16()?, r.u16()?);
                let furnishings = (0..r.u8()?)
                    .map(|_| {
                        Ok(Furnishing {
                            category: r.u8()?,
                            rooms: r.u16()?,
                            count: r.u32()?,
                        })
                    })
                    .collect::<Result<_, RecordError>>()?;
                Ok(UseProfile {
                    use_,
                    rooms,
                    structures,
                    tiles,
                    ground,
                    roofed,
                    entrances,
                    furnishings,
                })
            })
            .collect::<Result<_, RecordError>>()?;
        let links = (0..r.u16()?)
            .map(|_| {
                Ok(Link {
                    from: r.u8()?,
                    to: r.u8()?,
                    via: r.u8()?,
                    count: r.u16()?,
                })
            })
            .collect::<Result<_, RecordError>>()?;
        let floors = get_counts(&mut r)?;
        let walls = get_counts(&mut r)?;
        let roofs = get_counts(&mut r)?;
        let outside = get_counts(&mut r)?;
        let on_sites = r.u16()?;
        let site_classes = get_counts(&mut r)?;
        let site_gap = match r.u8()? {
            0 => None,
            _ => Some(get_spread(&mut r)?),
        };
        finish(&r)?;
        Ok(Self {
            class,
            structures,
            sizes,
            uses,
            links,
            floors,
            walls,
            roofs,
            outside,
            on_sites,
            site_classes,
            site_gap,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(room: u16) -> Piece {
        Piece {
            object: 1234,
            category: 3,
            icon: NONE16,
            kind: 10,
            rotation: 1,
            plane: 0,
            x: -1,
            z: 2,
            room,
            dressing: Dressing::NONE,
        }
    }

    fn blueprint() -> Blueprint {
        Blueprint {
            width: 4,
            depth: 3,
            planes: vec![0, 1],
            class: 2,
            site: site_id(blueprint_id(0, 3200, 3200).unwrap()),
            clipped: false,
            rooms: vec![Room {
                plane: 0,
                roofed: true,
                use_: 1,
                floor: Dressing {
                    material: 0,
                    material_ref: 4,
                    colour: NONE,
                },
                tiles: vec![Tile {
                    x: 0,
                    z: 2,
                    overlay: 3,
                    underlay: 0,
                    height: -12,
                }],
            }],
            doors: vec![Door {
                piece: piece(NONE16),
                open: false,
                rooms: [NONE16, 0],
            }],
            openings: vec![Opening {
                room: 0,
                plane: 0,
                x: 3,
                z: 1,
                side: 2,
            }],
            walls: vec![piece(NONE16)],
            roofs: Vec::new(),
            objects: vec![piece(0)],
        }
    }

    #[test]
    fn blueprints_round_trip_and_check_rooms() {
        let b = blueprint();
        let bytes = b.encode().unwrap();
        assert_eq!(Blueprint::decode(&bytes).unwrap(), b);
        assert_eq!(
            StructureRecord::decode(&bytes).unwrap(),
            StructureRecord::Blueprint(b.clone())
        );
        let mut bad = b.clone();
        bad.objects[0].room = 1;
        assert!(bad.encode().is_err());
        let mut long = bytes.clone();
        long.push(0);
        assert!(matches!(
            Blueprint::decode(&long),
            Err(RecordError::Trailing(1))
        ));
        assert!(Blueprint::decode(&bytes[..bytes.len() - 1]).is_err());
    }

    #[test]
    fn sites_and_profiles_round_trip() {
        let s = Site {
            width: 30,
            depth: 20,
            members: vec![
                Member {
                    blueprint: 5,
                    x: 0,
                    z: 4,
                },
                Member {
                    blueprint: 9,
                    x: 20,
                    z: 0,
                },
            ],
        };
        let bytes = s.encode().unwrap();
        assert_eq!(
            StructureRecord::decode(&bytes).unwrap(),
            StructureRecord::Site(s.clone())
        );
        let mut bad = s;
        bad.members.reverse();
        assert!(bad.encode().is_err());
        let spread = Spread([1, 2, 3, 4, 5]);
        let p = Profile {
            class: 7,
            structures: 30,
            sizes: [spread; 6],
            uses: vec![UseProfile {
                use_: 3,
                rooms: 29,
                structures: 28,
                tiles: spread,
                ground: 26,
                roofed: 29,
                entrances: 27,
                furnishings: vec![Furnishing {
                    category: 17,
                    rooms: 27,
                    count: 193,
                }],
            }],
            links: vec![Link {
                from: 0,
                to: 3,
                via: 1,
                count: 10,
            }],
            floors: vec![Count {
                code: 0,
                count: 70000,
            }],
            walls: Vec::new(),
            roofs: Vec::new(),
            outside: Vec::new(),
            on_sites: 2,
            site_classes: vec![Count { code: 7, count: 2 }],
            site_gap: Some(spread),
        };
        let bytes = p.encode().unwrap();
        assert_eq!(
            StructureRecord::decode(&bytes).unwrap(),
            StructureRecord::Profile(p)
        );
    }

    #[test]
    fn ids_keep_their_namespaces_apart() {
        let b = blueprint_id(3, 16383, 16383).unwrap();
        assert!(b < SITE_BASE);
        assert_eq!(blueprint_id(4, 0, 0), None);
        assert_eq!(blueprint_id(0, 1 << 14, 0), None);
        assert_eq!(site_id(b) & !SITE_BASE, b);
        assert!(profile_id(255) >= PROFILE_BASE);
    }
}
