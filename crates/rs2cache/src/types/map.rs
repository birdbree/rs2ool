//! Map regions from index 4.

use serde::{Deserialize, Serialize};

/// Tiles per region side.
pub const REGION_SIZE: usize = 64;

/// One plane of a region. Every grid is 64 × 64, indexed `x * 64 + y`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plane {
    /// Corner height in client units (negative is up; 240 per storey).
    pub heights: Vec<i32>,
    /// Whether the height was derived (noise on plane 0, `below − 240`
    /// above) because the tile gave none.
    pub height_derived: Vec<bool>,
    /// Overlay floor id (0 = none, otherwise `flo` id + 1).
    pub overlays: Vec<u8>,
    /// Overlay shape, 0–11.
    pub shapes: Vec<u8>,
    /// Overlay rotation, 0–3.
    pub rotations: Vec<u8>,
    /// Tile flags: 1 = blocked, 2 = bridge (the plane above is walked as
    /// this one), 4 = roof, 8 = drawn on plane 0, 16 = no shadow.
    pub flags: Vec<u8>,
    /// Underlay floor id (0 = none, otherwise `flo` id + 1).
    pub underlays: Vec<u8>,
}

/// One placed location (world object).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    pub id: u16,
    pub plane: u8,
    pub x: u8,
    pub y: u8,
    /// 317 location type, 0–22.
    pub kind: u8,
    pub rotation: u8,
}

/// A 64 × 64 map region and its locations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    /// `x << 8 | y`.
    pub id: u16,
    pub x: u8,
    pub y: u8,
    pub members: bool,
    pub planes: Vec<Plane>,
    pub locations: Vec<Location>,
}
