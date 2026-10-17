//! Cache space to pack space. The cache is right-handed with y down and z
//! along the tile y axis; packs are y up. Negating y and z together is a
//! rotation (half a turn about x), so assets keep their handedness;
//! negating y alone would mirror them.

use glam::DVec3;

/// The per-axis signs from cache space to pack space.
pub const FLIP: DVec3 = DVec3::new(1.0, -1.0, -1.0);

/// A cache-space point or offset in pack space.
pub fn to_pack([x, y, z]: [i32; 3]) -> [i32; 3] {
    [x, -y, -z]
}

/// A cache-space point in pack space, as a vector.
pub fn to_pack_vec(p: [i32; 3]) -> DVec3 {
    let [x, y, z] = to_pack(p);
    DVec3::new(x.into(), y.into(), z.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mapping_is_a_rotation() {
        let [x, y, z] = [[1, 0, 0], [0, 1, 0], [0, 0, 1]].map(to_pack_vec);
        assert_eq!(x.cross(y), z, "handedness kept");
        assert_eq!(to_pack([1, -2, 3]), [1, 2, -3]);
    }
}
