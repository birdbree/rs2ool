//! What a structure's walls, roofs and floors are made of: the dressing
//! half of a blueprint, so a building can be rebuilt in other materials.
//!
//! A wall, door or roof piece is made of its model's main texture, when
//! textured faces cover at least half its area, and is otherwise plain,
//! with the area-weighted colour of its faces. What each texture is made of
//! was read off the textures by eye ([`TEXTURES`]), and checked against
//! the names of the floors that use them. A floor is made of its
//! texture, or else what its name says ([`FLOOR_KEYWORDS`]), or else it's
//! plain, with its colour.

use std::collections::BTreeMap;

use contentpack::palette::hsl_to_rgb;
use rs2cache::Records;
use rs2cache::types::config::{FloDef, LocDef};
use rs2cache::types::model::Model;
use serde::{Deserialize, Serialize};

use crate::structure::Structure;

/// A building material.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Material {
    /// Planks, boards, panels.
    Wood,
    /// Rough stone, stone blocks and slabs.
    Stone,
    /// Fired brick.
    Brick,
    /// Marble.
    Marble,
    /// Sandstone blocks, carved or plain.
    Sandstone,
    /// Cobbles, roads and paths.
    Cobbles,
    /// Plaster, parchment, paper.
    Plaster,
    /// Thatch and straw.
    Thatch,
    /// Roof tiles and shingles.
    RoofTiles,
    /// Metal: mesh, bars, plate.
    Metal,
    /// Leaves and creepers.
    Foliage,
    /// Grass.
    Grass,
    /// Mud and earth.
    Mud,
    /// Sand.
    Sand,
    /// Snow and ice.
    Snow,
    /// Carpet and rugs.
    Carpet,
    /// Water.
    Water,
    /// Lava.
    Lava,
    /// A picture or a bookshelf: a texture that shows a thing rather than a
    /// surface.
    Picture,
    /// Drawn as nothing: a floor of the colour the original client leaves
    /// out.
    Invisible,
    /// No texture, just colour.
    Plain,
}

/// What something is made of, with its texture or, for plain materials,
/// its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dressing {
    /// The material.
    pub material: Material,
    /// The texture it's drawn with, 0–49, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture: Option<u16>,
    /// Its colour, `0xRRGGBB`, when it's plain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour: Option<u32>,
}

impl Dressing {
    fn plain(colour: u32) -> Self {
        Self {
            material: Material::Plain,
            texture: None,
            colour: Some(colour),
        }
    }
}

/// What each texture shows and what it's made of, read off the textures
/// by eye. `None` for textures that are no material (a cobweb, stars).
pub const TEXTURES: &[(u16, &str, Option<Material>)] = &[
    (0, "dark wooden planks with a handle", Some(Material::Wood)),
    (1, "rippling blue water", Some(Material::Water)),
    (2, "grey stone bricks laid upright", Some(Material::Stone)),
    (3, "brown floorboards", Some(Material::Wood)),
    (4, "an ornate panel", Some(Material::Wood)),
    (5, "dark wood", Some(Material::Wood)),
    (6, "red roof tiles", Some(Material::RoofTiles)),
    (7, "dark slats", Some(Material::Wood)),
    (8, "green leaves", Some(Material::Foliage)),
    (9, "the rings of a cut log", Some(Material::Wood)),
    (10, "plain blue", None),
    (11, "rough grey-brown stone", Some(Material::Stone)),
    (12, "a dark lattice", Some(Material::Metal)),
    (13, "a landscape painting", Some(Material::Picture)),
    (14, "a portrait", Some(Material::Picture)),
    (15, "marble", Some(Material::Marble)),
    (16, "wooden planks", Some(Material::Wood)),
    (17, "falling drops of water", Some(Material::Water)),
    (18, "straw", Some(Material::Thatch)),
    (19, "dark shingles", Some(Material::RoofTiles)),
    (20, "a bookshelf", Some(Material::Picture)),
    (21, "brown thatch", Some(Material::Thatch)),
    (22, "reddish planks", Some(Material::Wood)),
    (23, "mossy stone bricks laid upright", Some(Material::Stone)),
    (24, "rippling blue water", Some(Material::Water)),
    (25, "green water", Some(Material::Water)),
    (26, "a cobweb", None),
    (27, "brown shingles", Some(Material::RoofTiles)),
    (28, "dark moss", Some(Material::Foliage)),
    (29, "a fringe of grass", Some(Material::Grass)),
    (30, "a fern", Some(Material::Foliage)),
    (31, "lava", Some(Material::Lava)),
    (32, "brown earth", Some(Material::Mud)),
    (33, "autumn leaves", Some(Material::Foliage)),
    (34, "stars", None),
    (35, "sandstone blocks", Some(Material::Sandstone)),
    (36, "a carved wooden panel", Some(Material::Wood)),
    (37, "metal mesh", Some(Material::Metal)),
    (38, "sand", Some(Material::Sand)),
    (39, "a portrait", Some(Material::Picture)),
    (40, "lava", Some(Material::Lava)),
    (41, "leaves", Some(Material::Foliage)),
    (42, "plain blue", None),
    (43, "grey stone slabs", Some(Material::Stone)),
    (44, "grey stone bricks", Some(Material::Stone)),
    (45, "a wooden panel", Some(Material::Wood)),
    (46, "cobblestones", Some(Material::Cobbles)),
    (47, "pale stone bricks laid upright", Some(Material::Stone)),
    (
        48,
        "sandstone carved with glyphs",
        Some(Material::Sandstone),
    ),
    (49, "parchment with writing", Some(Material::Plaster)),
];

/// What texture `id` is made of, if it's a known texture and a material.
pub fn texture_material(id: u16) -> Option<Material> {
    TEXTURES
        .iter()
        .find(|(i, _, _)| *i == id)
        .and_then(|&(_, _, m)| m)
}

/// Words a floor's name may contain, and the material each means, tried in
/// order. Floor names run words together ("woodenfloor"), so these match
/// anywhere in the name, ignoring case.
pub const FLOOR_KEYWORDS: &[(Material, &[&str])] = &[
    (Material::Invisible, &["invisible"]),
    (Material::Water, &["water", "swamp"]),
    (Material::Lava, &["lava"]),
    (Material::Marble, &["marble"]),
    (Material::Brick, &["brick"]),
    (Material::Carpet, &["carpet", "rug"]),
    (Material::Wood, &["wood", "plank"]),
    (Material::Thatch, &["thatch", "straw"]),
    (Material::RoofTiles, &["roof"]),
    (Material::Mud, &["mud", "dirt", "earth"]),
    (Material::Sand, &["sand", "desert"]),
    (Material::Snow, &["snow", "ice"]),
    (Material::Grass, &["grass"]),
    (Material::Cobbles, &["road", "cobble", "path"]),
    (
        Material::Stone,
        &[
            "stone", "rock", "cliff", "mountain", "tile", "slate", "granite",
        ],
    ),
];

/// The RGB colour the original client draws as nothing on an overlay.
const HOLE: u32 = 0xFF00FF;

/// What a floor is made of.
pub fn of_floor(def: &FloDef) -> Dressing {
    if let Some(t) = def.texture {
        let t = u16::from(t);
        return Dressing {
            material: texture_material(t).unwrap_or(Material::Plain),
            texture: Some(t),
            colour: None,
        };
    }
    if def.rgb == Some(HOLE) {
        return Dressing {
            material: Material::Invisible,
            texture: None,
            colour: None,
        };
    }
    let name = def.name.as_deref().unwrap_or("").to_ascii_lowercase();
    let named = FLOOR_KEYWORDS
        .iter()
        .find(|(_, keys)| keys.iter().any(|k| name.contains(k)))
        .map(|&(m, _)| m);
    Dressing {
        material: named.unwrap_or(Material::Plain),
        texture: None,
        colour: def.rgb,
    }
}

/// What a model is made of: its main texture, when textured faces cover at
/// least half its area, or else plain with its faces' area-weighted
/// colour. `None` for a model with no area.
pub fn of_model(m: &Model) -> Option<Dressing> {
    let mut by_texture: BTreeMap<u16, f64> = BTreeMap::new();
    let (mut total, mut plain) = (0.0, 0.0);
    let mut rgb = [0.0f64; 3];
    for (i, f) in m.faces.iter().enumerate() {
        let p = f.map(|v| m.vertices.get(usize::from(v)).copied().unwrap_or_default());
        let area = area(p);
        total += area;
        let ty = m.face_types.as_ref().map_or(0, |t| t[i]);
        let textured = ty & 2 != 0
            && m.textured_faces.get(usize::from(ty >> 2)).is_some()
            && m.face_colours[i] < 50;
        if textured {
            *by_texture.entry(m.face_colours[i]).or_default() += area;
        } else {
            let c = hsl_to_rgb(m.face_colours[i]);
            for (k, shift) in [16, 8, 0].into_iter().enumerate() {
                rgb[k] += area * f64::from((c >> shift) & 0xFF);
            }
            plain += area;
        }
    }
    if total <= 0.0 {
        return None;
    }
    let textured: f64 = by_texture.values().sum();
    if textured * 2.0 >= total {
        // The texture covering the most area; ties to the lower id.
        let (&t, _) = by_texture
            .iter()
            .max_by(|a, b| a.1.total_cmp(b.1).then(b.0.cmp(a.0)))?;
        return Some(Dressing {
            material: texture_material(t).unwrap_or(Material::Plain),
            texture: Some(t),
            colour: None,
        });
    }
    let c = rgb.map(|v| (v / plain).round().clamp(0.0, 255.0) as u32);
    Some(Dressing::plain(c[0] << 16 | c[1] << 8 | c[2]))
}

/// A triangle's area, in square model units.
fn area([a, b, c]: [[i32; 3]; 3]) -> f64 {
    let u = [0, 1, 2].map(|k| f64::from(b[k] - a[k]));
    let v = [0, 1, 2].map(|k| f64::from(c[k] - a[k]));
    let x = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt() / 2.0
}

/// Fills in what each structure is made of: every wall, door and roof
/// piece's material from the model its definition draws for its placement
/// type, and each room's floor, its commonest floor's material (overlay,
/// else underlay). `locs` and `floors` hold the definitions by id.
pub fn dress(
    structures: &mut [Structure],
    records: &Records,
    locs: &BTreeMap<u16, LocDef>,
    floors: &BTreeMap<u16, FloDef>,
) {
    let mut models: BTreeMap<(u16, u8), Option<Dressing>> = BTreeMap::new();
    let mut of = |id: u16, kind: u8| {
        *models.entry((id, kind)).or_insert_with(|| {
            let def = locs.get(&id)?;
            let m = crate::preview::location_model(records, def, kind).ok()??;
            of_model(&m)
        })
    };
    for s in structures.iter_mut() {
        for p in s
            .walls
            .iter_mut()
            .chain(s.doors.iter_mut().map(|d| &mut d.placed))
            .chain(s.roofs.iter_mut())
        {
            p.material = of(p.id, p.kind);
        }
        for r in &mut s.rooms {
            let mut count: BTreeMap<u16, usize> = BTreeMap::new();
            for t in &r.tiles {
                let flo = if t.overlay != 0 {
                    t.overlay
                } else {
                    t.underlay
                };
                if flo != 0 {
                    *count.entry(u16::from(flo) - 1).or_default() += 1;
                }
            }
            // The commonest floor; ties to the lower id.
            r.floor = count
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
                .and_then(|(id, _)| floors.get(id))
                .map(of_floor);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flo(name: Option<&str>, rgb: Option<u32>, texture: Option<u8>) -> FloDef {
        FloDef {
            id: 0,
            rgb,
            texture,
            hide_underlay: false,
            occlude: true,
            name: name.map(str::to_string),
            secondary_rgb: None,
            unknown_opcodes: Vec::new(),
        }
    }

    #[test]
    fn floors_by_texture_then_name_then_colour() {
        assert_eq!(
            of_floor(&flo(Some("mud"), None, Some(16))).material,
            Material::Wood
        );
        assert_eq!(
            of_floor(&flo(Some("woodenfloor"), Some(0x664422), None)).material,
            Material::Wood
        );
        assert_eq!(
            of_floor(&flo(Some("mudfloor2"), Some(0x663300), None)).material,
            Material::Mud
        );
        assert_eq!(
            of_floor(&flo(Some("x"), Some(HOLE), None)).material,
            Material::Invisible
        );
        let plain = of_floor(&flo(Some("odd"), Some(0x123456), None));
        assert_eq!(
            (plain.material, plain.colour),
            (Material::Plain, Some(0x123456))
        );
    }

    /// A square of two faces in the plane z = 0, side `s`.
    fn square(colours: [u16; 2], types: Option<Vec<u8>>, s: i32) -> Model {
        Model {
            vertices: vec![[0, 0, 0], [s, 0, 0], [s, s, 0], [0, s, 0]],
            faces: vec![[0, 1, 2], [0, 2, 3]],
            face_colours: colours.to_vec(),
            face_types: types,
            priority: 0,
            face_priorities: None,
            face_alphas: None,
            face_labels: None,
            vertex_labels: None,
            textured_faces: vec![[0, 1, 3]],
        }
    }

    #[test]
    fn models_by_their_main_texture_or_their_colour() {
        // Both faces textured with marble.
        let marble = square([15, 15], Some(vec![2, 2]), 10);
        assert_eq!(
            of_model(&marble),
            Some(Dressing {
                material: Material::Marble,
                texture: Some(15),
                colour: None
            })
        );
        // Half textured is enough.
        let half = square([15, 0], Some(vec![2, 0]), 10);
        assert_eq!(of_model(&half).map(|d| d.material), Some(Material::Marble));
        // Untextured: plain, with its colour.
        let plain = square([0, 0], None, 10);
        let d = of_model(&plain).unwrap();
        assert_eq!(d.material, Material::Plain);
        assert_eq!(d.colour, Some(hsl_to_rgb(0)));
        // No area, no material.
        assert_eq!(of_model(&square([0, 0], None, 0)), None);
    }

    #[test]
    fn textures_are_listed_once_in_order() {
        assert_eq!(TEXTURES.len(), 50);
        assert!(
            TEXTURES
                .iter()
                .enumerate()
                .all(|(i, t)| usize::from(t.0) == i)
        );
    }
}
