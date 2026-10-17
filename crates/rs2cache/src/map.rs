//! Map regions (index 4) and the versionlist's `map_index`.
//!
//! A region has two index-4 files. The tile file (`m`) runs
//! `plane 0..4, x 0..64, y 0..64`, each tile a run of opcodes ending with
//! 0 (height derived) or 1 (explicit height): `1` → height u8 and end;
//! `2–49` → overlay u8, shape = (op − 2) / 4, rotation = (op − 2) & 3;
//! `50–81` → flags = op − 49; `≥ 82` → underlay = op − 81. The location file
//! (`l`) is nested unsigned-smart deltas: location id, then packed
//! positions `plane << 12 | x << 6 | y`, then an attribute byte
//! `type << 2 | rotation`.

use rs2intermediate::map::{Location, Plane, REGION_SIZE, Region};
use thiserror::Error;

use crate::io::{Cursor, IoError};

#[derive(Debug, Error)]
pub enum MapError {
    #[error("map_index: {0}")]
    Index(IoError),
    #[error("tile file: {0}")]
    Tiles(IoError),
    #[error("location file: {0}")]
    Locations(IoError),
}

/// One `map_index` record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MapIndexEntry {
    pub region: u16,
    pub tile_file: u16,
    pub location_file: u16,
    pub members: bool,
}

impl MapIndexEntry {
    pub fn x(&self) -> u8 {
        (self.region >> 8) as u8
    }
    pub fn y(&self) -> u8 {
        self.region as u8
    }
}

/// Parse `map_index`: 7-byte records `{region u16, tileFile u16,
/// locationFile u16, members u8}`.
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

// The client's terrain noise, ported with Java's wrapping int arithmetic.

fn noise(x: i32, y: i32) -> i32 {
    let k = x.wrapping_add(y.wrapping_mul(57));
    let k = (k << 13) ^ k;
    let l = k
        .wrapping_mul(k.wrapping_mul(k).wrapping_mul(15731).wrapping_add(0xc0ae5))
        .wrapping_add(0x5208dd0d)
        & 0x7fff_ffff;
    (l >> 19) & 0xff
}

fn smooth_noise(x: i32, y: i32) -> i32 {
    let corners =
        noise(x - 1, y - 1) + noise(x + 1, y - 1) + noise(x - 1, y + 1) + noise(x + 1, y + 1);
    let sides = noise(x - 1, y) + noise(x + 1, y) + noise(x, y - 1) + noise(x, y + 1);
    corners / 16 + sides / 8 + noise(x, y) / 4
}

fn cosine(k: i32) -> i32 {
    (65536.0 * (f64::from(k) * 0.003_067_961_5).cos()) as i32
}

fn interpolate(a: i32, b: i32, t: i32, scale: i32) -> i32 {
    let f = (0x10000 - cosine(t * 1024 / scale)) >> 1;
    ((a * (0x10000 - f)) >> 16) + ((b * f) >> 16)
}

fn interpolated_noise(x: i32, y: i32, scale: i32) -> i32 {
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

/// The plane-0 height the client derives for world tile `(x, y)` when the
/// tile file gives none, in client units.
pub fn derived_height(x: i32, y: i32) -> i32 {
    let (x, y) = (x + 0xe3b7b, y + 0x87cce);
    let mut k = interpolated_noise(x + 45365, y + 0x16713, 4) - 128
        + ((interpolated_noise(x + 10294, y + 37821, 2) - 128) >> 1)
        + ((interpolated_noise(x, y, 1) - 128) >> 2);
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

/// Decode a tile file, returning the four planes and the bytes read.
pub fn decode_tiles(entry: &MapIndexEntry, tiles: &[u8]) -> Result<(Vec<Plane>, usize), MapError> {
    let n = REGION_SIZE * REGION_SIZE;
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
    let mut id: i32 = -1;
    loop {
        let delta = c.smart().map_err(MapError::Locations)?;
        if delta == 0 {
            break;
        }
        id += i32::from(delta);
        let mut pos: i32 = 0;
        loop {
            let delta = c.smart().map_err(MapError::Locations)?;
            if delta == 0 {
                break;
            }
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
