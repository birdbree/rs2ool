//! Map regions from index 4.
//!
//! A region is a 64 × 64-tile square of the world with four planes. Region
//! `(x, y)` covers world tiles `x * 64 ..= x * 64 + 63` and the same for y.

use serde::{Deserialize, Serialize};

/// Tiles per region side.
pub const REGION_SIZE: usize = 64;

/// One plane of a region. Every grid is 64 × 64, indexed `x * 64 + y`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plane {
    /// Corner height in client units. Negative is up, and a plane is 240
    /// units tall.
    pub heights: Vec<i32>,
    /// Whether the height was derived (noise on plane 0, `below − 240`
    /// above) because the tile gave none.
    pub height_derived: Vec<bool>,
    /// Overlay floor (0 = none, otherwise `flo` id + 1).
    pub overlays: Vec<u8>,
    /// Overlay shape, 0–11.
    pub shapes: Vec<u8>,
    /// Overlay rotation, 0–3.
    pub rotations: Vec<u8>,
    /// Tile flags: 1 = blocked, 2 = bridge (the plane above is walked as
    /// this one), 4 = roof, 8 = drawn on plane 0, 16 = no shadow.
    pub flags: Vec<u8>,
    /// Underlay floor (0 = none, otherwise `flo` id + 1).
    pub underlays: Vec<u8>,
}

/// One placed location (world object).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    /// The `loc` definition id.
    pub id: u16,
    /// Plane 0–3.
    pub plane: u8,
    /// Tile x within the region, 0–63.
    pub x: u8,
    /// Tile y within the region, 0–63.
    pub y: u8,
    /// 317 location type, 0–22.
    pub kind: u8,
    /// Rotation in quarter turns, 0–3.
    pub rotation: u8,
}

/// A 64 × 64 map region and its locations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    /// `x << 8 | y`.
    pub id: u16,
    /// Region x, 0–255.
    pub x: u8,
    /// Region y, 0–255.
    pub y: u8,
    /// Members-only region, from `map_index`.
    pub members: bool,
    /// The four planes, 0 (ground) to 3.
    pub planes: Vec<Plane>,
    /// Placed objects, grouped by id in stored order.
    pub locations: Vec<Location>,
}
