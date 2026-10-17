//! Map regions (index 4) and the versionlist's `map_index`.
//!
//! `map_index` (in the versionlist archive) is a run of 7-byte records
//! saying which two index-4 files make up each region.
//!
//! The tile file (`m`) walks plane 0–3, then x 0–63, then y 0–63. Each tile
//! is a run of opcodes that ends at a 0 or a 1; a later opcode overwrites an
//! earlier one:
//!
//! ```text
//! 0        end of tile; height derived
//! 1        height u8, end of tile; explicit height (a value of 1 counts as 0)
//! 2–49     overlay u8; shape = (op − 2) / 4, rotation = (op − 2) & 3
//! 50–81    flags = op − 49
//! 82–255   underlay = op − 81
//! ```
//!
//! An explicit height is `below − h × 8`, where `below` is the same tile on
//! the plane underneath (0 on plane 0). A derived one is `below − 240` above
//! plane 0, and the terrain noise ([`derived_height`]) on plane 0.
//!
//! The location file (`l`) is two levels of unsigned smarts, each ending at
//! a 0:
//!
//! ```text
//! idDelta     smart   0 = end of file; id += idDelta (id starts at −1)
//!   posDelta  smart   0 = end of this id; pos += posDelta − 1 (pos starts at 0)
//!   attrs     u8      type << 2 | rotation
//! pos = plane << 12 | x << 6 | y
//! ```

use crate::types::map::{Location, Plane, REGION_SIZE, Region};
use thiserror::Error;

use crate::io::{Cursor, IoError};

/// A region that wouldn't decode; each variant wraps the read that ran out.
#[derive(Debug, Error)]
pub enum MapError {
    /// Reading `map_index` failed.
    #[error("map_index: {0}")]
    Index(IoError),
    /// The tile file ran out.
    #[error("tile file: {0}")]
    Tiles(IoError),
    /// The location file ran out.
    #[error("location file: {0}")]
    Locations(IoError),
}

/// One `map_index` record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapIndexEntry {
    /// Region id, `x << 8 | y`.
    pub region: u16,
    /// Index-4 file id of the tile file.
    pub tile_file: u16,
    /// Index-4 file id of the location file.
    pub location_file: u16,
    /// Members-only region (the byte is exactly 1).
    pub members: bool,
}

impl MapIndexEntry {
    /// Region x, the high byte of the id.
    pub fn x(&self) -> u8 {
        (self.region >> 8) as u8
    }
    /// Region y, the low byte of the id.
    pub fn y(&self) -> u8 {
        self.region as u8
    }
}

/// Parse `map_index`: 7-byte records `{region u16, tileFile u16,
/// locationFile u16, members u8}`. A trailing partial record is ignored.
pub fn map_index(data: &[u8]) -> Result<Vec<MapIndexEntry>, MapError> {
    let mut c = Cursor::new(data);
    (0..data.len() / 7)
        .map(|_| {
            Ok(MapIndexEntry {
                region: c.u16()?,
                tile_file: c.u16()?,
                location_file: c.u16()?,
                members: c.u8()? == 1,
            })
        })
        .collect::<Result<_, IoError>>()
        .map_err(MapError::Index)
}

// The original client's terrain noise, ported with 32-bit wrapping
// arithmetic so overflow behaves the same.

// Integer hash noise, 0–255, for one lattice point.
fn noise(x: i32, y: i32) -> i32 {
    // Fold the point into one seed, then mix its bits.
    let k = x.wrapping_add(y.wrapping_mul(57));
    let k = (k << 13) ^ k;
    // Polynomial hash of the seed, made non-negative.
    let l = k
        .wrapping_mul(k.wrapping_mul(k).wrapping_mul(15731).wrapping_add(0xc0ae5))
        .wrapping_add(0x5208dd0d)
        & 0x7fff_ffff;
    // Keep 8 bits from the middle of the hash.
    (l >> 19) & 0xff
}

// The point's noise blurred with its 8 neighbours (centre 1/4, sides 1/8,
// corners 1/16).
fn smooth_noise(x: i32, y: i32) -> i32 {
    let corners =
        noise(x - 1, y - 1) + noise(x + 1, y - 1) + noise(x - 1, y + 1) + noise(x + 1, y + 1);
    let sides = noise(x - 1, y) + noise(x + 1, y) + noise(x, y - 1) + noise(x, y + 1);
    corners / 16 + sides / 8 + noise(x, y) / 4
}

// cos of `k` 2048ths of a turn (0.0030679615 = 2π / 2048), in 16.16 fixed
// point.
fn cosine(k: i32) -> i32 {
    (65536.0 * (f64::from(k) * 0.003_067_961_5).cos()) as i32
}

// Cosine-interpolate from `a` to `b` at step `t` of `scale`.
fn interpolate(a: i32, b: i32, t: i32, scale: i32) -> i32 {
    // `t * 1024 / scale` is 0 to half a turn; `f` is the 16.16 weight of
    // `b`, easing from 0 to 1.
    let f = (0x10000 - cosine(t * 1024 / scale)) >> 1;
    ((a * (0x10000 - f)) >> 16) + ((b * f) >> 16)
}

// Smoothed noise sampled on a lattice `scale` tiles apart and interpolated
// between the four surrounding points. `scale` is a power of two, so the
// mask gives the remainder.
fn interpolated_noise(x: i32, y: i32, scale: i32) -> i32 {
    // Lattice cell (`ix`, `iy`) and the offset inside it (`fx`, `fy`).
    let (ix, fx) = (x / scale, x & (scale - 1));
    let (iy, fy) = (y / scale, y & (scale - 1));
    let a = smooth_noise(ix, iy);
    let b = smooth_noise(ix + 1, iy);
    let c = smooth_noise(ix, iy + 1);
    let d = smooth_noise(ix + 1, iy + 1);
    interpolate(
        interpolate(a, b, fx, scale),
        interpolate(c, d, fx, scale),
        fy,
        scale,
    )
}

/// The plane-0 height the original client derives for world tile `(x, y)`
/// when the tile file gives none, in client units (negative is up). Always
/// between −480 and −80.
pub fn derived_height(x: i32, y: i32) -> i32 {
    // Fixed offsets move the sample point into a fixed patch of noise.
    let (x, y) = (x + 0xe3b7b, y + 0x87cce);
    // Three octaves (scales 4, 2, 1), each centred on 0 and worth half the
    // one before.
    let mut k = interpolated_noise(x + 45365, y + 0x16713, 4) - 128
        + ((interpolated_noise(x + 10294, y + 37821, 2) - 128) >> 1)
        + ((interpolated_noise(x, y, 1) - 128) >> 2);
    // Squash into 10–60 and scale by 8, the same step as explicit heights.
    k = ((f64::from(k) * 0.3) as i32 + 35).clamp(10, 60);
    -k * 8
}

/// Decode a region's inflated tile and location files.
pub fn decode(entry: &MapIndexEntry, tiles: &[u8], locations: &[u8]) -> Result<Region, MapError> {
    let (planes, _) = decode_tiles(entry, tiles)?;
    let (locations, _) = decode_locations(locations)?;
    Ok(Region {
        id: entry.region,
        x: entry.x(),
        y: entry.y(),
        members: entry.members,
        planes,
        locations,
    })
}

/// Decode a tile file, returning the four planes and the bytes read. The
/// entry supplies the region's world position for derived heights.
pub fn decode_tiles(entry: &MapIndexEntry, tiles: &[u8]) -> Result<(Vec<Plane>, usize), MapError> {
    let n = REGION_SIZE * REGION_SIZE;
    // World tile coordinates of the region's corner.
    let (base_x, base_y) = (i32::from(entry.x()) * 64, i32::from(entry.y()) * 64);
    let mut planes: Vec<Plane> = Vec::with_capacity(4);
    let mut c = Cursor::new(tiles);
    for _ in 0..4 {
        let mut p = Plane {
            heights: vec![0; n],
            height_derived: vec![false; n],
            overlays: vec![0; n],
            shapes: vec![0; n],
            rotations: vec![0; n],
            flags: vec![0; n],
            underlays: vec![0; n],
        };
        for x in 0..REGION_SIZE {
            for y in 0..REGION_SIZE {
                let i = x * REGION_SIZE + y;
                // Same tile on the plane underneath; `None` on plane 0.
                let below = planes.last().map(|b: &Plane| b.heights[i]);
                loop {
                    let op = c.u8().map_err(MapError::Tiles)?;
                    match op {
                        0 => {
                            p.heights[i] = match below {
                                None => derived_height(base_x + x as i32, base_y + y as i32),
                                Some(h) => h - 240,
                            };
                            p.height_derived[i] = true;
                            break;
                        }
                        1 => {
                            let mut h = i32::from(c.u8().map_err(MapError::Tiles)?);
                            if h == 1 {
                                h = 0;
                            }
                            p.heights[i] = below.unwrap_or(0) - h * 8;
                            break;
                        }
                        2..=49 => {
                            p.overlays[i] = c.u8().map_err(MapError::Tiles)?;
                            p.shapes[i] = (op - 2) / 4;
                            p.rotations[i] = (op - 2) & 3;
                        }
                        50..=81 => p.flags[i] = op - 49,
                        _ => p.underlays[i] = op - 81,
                    }
                }
            }
        }
        planes.push(p);
    }

    Ok((planes, c.pos()))
}

/// Decode a location file, returning the locations and the bytes read.
pub fn decode_locations(locations: &[u8]) -> Result<(Vec<Location>, usize), MapError> {
    let mut out = Vec::new();
    let mut c = Cursor::new(locations);
    // Starts at -1 so the first delta (at least 1) lands on a real id.
    let mut id: i32 = -1;
    loop {
        let delta = c.smart().map_err(MapError::Locations)?;
        if delta == 0 {
            break;
        }
        id += i32::from(delta);
        // Packed `plane << 12 | x << 6 | y`, reset for each id.
        let mut pos: i32 = 0;
        loop {
            let delta = c.smart().map_err(MapError::Locations)?;
            if delta == 0 {
                break;
            }
            // Deltas are stored plus 1 so that 0 can end the list.
            pos += i32::from(delta) - 1;
            let attr = c.u8().map_err(MapError::Locations)?;
            out.push(Location {
                id: id as u16,
                plane: (pos >> 12) as u8,
                x: ((pos >> 6) & 63) as u8,
                y: (pos & 63) as u8,
                kind: attr >> 2,
                rotation: attr & 3,
            });
        }
    }
    Ok((out, c.pos()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> MapIndexEntry {
        MapIndexEntry {
            region: 50 << 8 | 50,
            tile_file: 0,
            location_file: 1,
            members: false,
        }
    }

    /// A tile file where tile (1, 2) on plane 0 has an overlay, flags and an
    /// underlay and an explicit height, and every other tile is derived.
    fn tiles() -> Vec<u8> {
        let mut out = Vec::new();
        for plane in 0..4 {
            for x in 0..64 {
                for y in 0..64 {
                    if plane == 0 && x == 1 && y == 2 {
                        out.extend([2 + 4 * 3 + 2, 7, 49 + 2, 81 + 5, 1, 10]);
                    } else {
                        out.push(0);
                    }
                }
            }
        }
        out
    }

    #[test]
    fn decodes_tiles() {
        let r = decode(&entry(), &tiles(), &[0]).unwrap();
        assert_eq!((r.x, r.y), (50, 50));
        let p = &r.planes[0];
        let i = 64 + 2;
        assert_eq!((p.overlays[i], p.shapes[i], p.rotations[i]), (7, 3, 2));
        assert_eq!((p.flags[i], p.underlays[i]), (2, 5));
        assert_eq!(p.heights[i], -80);
        assert!(!p.height_derived[i]);
        assert!(p.height_derived[0]);
        assert_eq!(r.planes[1].heights[i], -80 - 240);
        assert_eq!(r.planes[1].heights[0], p.heights[0] - 240);
    }

    #[test]
    fn derived_heights_are_in_the_client_range() {
        for (x, y) in [(3200, 3200), (0, 0), (6399, 6399), (3222, 3218)] {
            let h = derived_height(x, y);
            assert!((-480..=-80).contains(&h) && h % 8 == 0, "{x},{y}: {h}");
        }
    }

    #[test]
    fn decodes_locations() {
        // Location 1 at plane 1 (1,2) type 10 rot 3; and at plane 0 (0,0)
        // type 0; location 3 at (63,63) type 22.
        let p1 = 1 << 12 | 1 << 6 | 2;
        let mut l = vec![2];
        l.extend((0x8000u16 + p1 as u16 + 1).to_be_bytes());
        l.push(10 << 2 | 3);
        l.extend([0, 2]);
        let p2 = 63 << 6 | 63;
        l.extend((0x8000u16 + p2 as u16 + 1).to_be_bytes());
        l.push(22 << 2);
        l.extend([0, 0]);
        let r = decode(&entry(), &tiles(), &l).unwrap();
        let locs: Vec<_> = r
            .locations
            .iter()
            .map(|l| (l.id, l.plane, l.x, l.y, l.kind, l.rotation))
            .collect();
        assert_eq!(locs, [(1, 1, 1, 2, 10, 3), (3, 0, 63, 63, 22, 0)]);
    }

    #[test]
    fn truncated_files_are_errors() {
        assert!(matches!(
            decode(&entry(), &tiles()[..100], &[0]),
            Err(MapError::Tiles(_))
        ));
        assert!(matches!(
            decode(&entry(), &tiles(), &[5]),
            Err(MapError::Locations(_))
        ));
    }
}
