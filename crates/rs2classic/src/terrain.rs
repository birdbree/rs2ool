//! Ground the way the original client draws it: underlay colours blended
//! across neighbouring tiles, each tile corner lit from the slope of the
//! ground and darkened by nearby walls and scenery, overlays cut into
//! their shapes and turned, and textured floors textured.
//!
//! The result is a [`Ground`]: a [`LitModel`] of gouraud and textured
//! triangles plus its vertices in cache coordinates (y down, z along the
//! tile y axis, 128 units to a tile), ready for
//! [`render_in_view`](crate::render_in_view) or for turning into meshes
//! elsewhere.
//!
//! The original client builds ground over its whole loaded scene, several
//! regions wide. This builds one region's ground at a time from [`Tiles`]:
//! the region alone, or stitched together with its eight neighbours so
//! that blending, lighting and shadows read across its edges and it meets
//! them without a seam. Where there's no neighbour, it makes do: the blend
//! window stops at the edge, and grid points past the last tile reuse the
//! nearest tile's height. Bridges (tile flag 2) and the low-detail mode
//! aren't modelled.
//!
//! Like the rest of the crate, this is written from the original client's
//! observable behaviour; see `docs/client/terrain.md` for the rules.

use rs2cache::types::config::FloDef;
use rs2cache::types::map::{Plane, REGION_SIZE};

use crate::light::LitModel;

/// A region's grid points per side: one more than tiles, for the far
/// corners.
pub const GRID: usize = REGION_SIZE + 1;

/// A floor's colour terms, worked out once from its RGB.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FloorColour {
    /// Hue, 256 to a turn. Reds leaning towards blue come out slightly
    /// negative (down to −42); that's kept, not wrapped.
    pub hue: i32,
    /// Saturation, 0–255.
    pub saturation: i32,
    /// Lightness, 0–255.
    pub lightness: i32,
    /// How strongly coloured the floor is, at least 1 for a floor with a
    /// colour: the hue's weight in a blend.
    pub chroma: i32,
    /// The hue (as a fraction of a turn) times `chroma`, for blending.
    pub hue_weight: i32,
    /// The floor's texture, if it has one.
    pub texture: Option<u8>,
    /// The floor is invisible as an overlay (RGB `0xFF00FF`): nothing is
    /// drawn where it lies.
    pub hidden: bool,
}

/// RGB `0xFF00FF` marks a floor that's drawn as nothing.
const HIDDEN_RGB: u32 = 0xff00ff;

impl FloorColour {
    /// The colour terms of a floor definition. A floor without an RGB
    /// colour has every term 0.
    pub fn new(def: &FloDef) -> Self {
        let mut f = def.rgb.map_or_else(Self::default, Self::from_rgb);
        f.texture = def.texture;
        f.hidden = def.rgb == Some(HIDDEN_RGB);
        f
    }

    /// The colour terms of a `0xRRGGBB` colour.
    pub fn from_rgb(rgb: u32) -> Self {
        // Channels over 256, not 255, as the original client does.
        let [r, g, b] = [16, 8, 0].map(|s| f64::from((rgb >> s) & 0xff) / 256.0);
        let (lo, hi) = (r.min(g).min(b), r.max(g).max(b));
        let l = (lo + hi) / 2.0;
        let (mut h, mut s) = (0.0, 0.0);
        if lo != hi {
            let d = hi - lo;
            s = if l < 0.5 {
                d / (hi + lo)
            } else {
                d / (2.0 - hi - lo)
            };
            h = if r == hi {
                (g - b) / d
            } else if g == hi {
                2.0 + (b - r) / d
            } else {
                4.0 + (r - g) / d
            } / 6.0;
        }
        let chroma = (if l > 0.5 { (1.0 - l) * s } else { l * s } * 512.0) as i32;
        let chroma = chroma.max(1);
        Self {
            hue: (h * 256.0) as i32,
            saturation: ((s * 256.0) as i32).clamp(0, 255),
            lightness: ((l * 256.0) as i32).clamp(0, 255),
            chroma,
            hue_weight: (h * f64::from(chroma)) as i32,
            texture: None,
            hidden: false,
        }
    }

    /// The floor's own colour as packed 16-bit HSL (see [`pack_hsl`]).
    pub fn hsl(&self) -> i32 {
        pack_hsl(self.hue, self.saturation, self.lightness)
    }
}

/// Packs 0–255 hue, saturation and lightness into the 16-bit HSL layout
/// (6 bits hue, 3 saturation, 7 lightness). Saturation is halved once for
/// each of the lightness thresholds 180, 193, 218 and 244 it reaches, so
/// light colours wash out.
///
/// A slightly negative hue gives a negative result; its low 16 bits are
/// still the right colour, and [`shade`] masks them.
pub fn pack_hsl(hue: i32, saturation: i32, lightness: i32) -> i32 {
    let mut s = saturation;
    for t in [179, 192, 217, 243] {
        if lightness > t {
            s /= 2;
        }
    }
    ((hue / 4) << 10) + ((s / 32) << 7) + lightness / 2
}

/// Lights packed HSL `hsl` with corner light `light`: lightness scaled by
/// `light / 128` and clamped to 2–126, hue and saturation kept.
pub fn shade(hsl: i32, light: i32) -> i32 {
    let l = (light * (hsl & 0x7f) / 128).clamp(2, 126);
    (hsl & 0xff80) + l
}

/// A textured corner's shade from its light: 0 fully lit to 127 darkest.
pub fn texture_shade(light: i32) -> i32 {
    127 - light.clamp(0, 127)
}

/// Each tile's blended underlay colour as packed HSL, indexed
/// `x * 64 + y`, or `None` for a tile without an underlay.
///
/// A tile blends the underlays of the 10 × 10 tiles from 4 before it to 5
/// after it on each axis (so the window sits half a tile off centre),
/// clipped to what `tiles` holds. Tiles without an underlay add nothing.
/// Hue is averaged weighted by chroma; saturation and lightness are plain
/// means. `floors` is indexed by floor id (map ids are one more).
///
/// Only the region's own tiles come back, but the window reads its
/// neighbours' when `tiles` has them.
pub fn blend_underlays(tiles: &Tiles, floors: &[FloorColour]) -> Vec<Option<i32>> {
    let n = tiles.size as isize;
    let floor = |x: isize, y: isize| floor_of(floors, tiles.underlays[(x * n + y) as usize]);
    let o = tiles.origin as isize;
    let r = REGION_SIZE as isize;
    let mut out = vec![None; REGION_SIZE * REGION_SIZE];
    for x in o..o + r {
        for y in o..o + r {
            if floor(x, y).is_none() {
                continue;
            }
            let (mut hw, mut s, mut l, mut c, mut count) = (0, 0, 0, 0, 0);
            for bx in (x - 4).max(0)..(x + 6).min(n) {
                for by in (y - 4).max(0)..(y + 6).min(n) {
                    if let Some(f) = floor(bx, by) {
                        hw += f.hue_weight;
                        s += f.saturation;
                        l += f.lightness;
                        c += f.chroma;
                        count += 1;
                    }
                }
            }
            // Only floors without a colour have no chroma.
            let hue = if c == 0 { 0 } else { hw * 256 / c };
            out[((x - o) * r + y - o) as usize] = Some(pack_hsl(hue, s / count, l / count));
        }
    }
    out
}

/// The floor a map id names: ids are floor ids plus one, 0 for none.
fn floor_of(floors: &[FloorColour], id: u8) -> Option<&FloorColour> {
    floors.get(usize::from(id).checked_sub(1)?)
}

/// One plane's tiles: a region's, or a region's with its neighbours'
/// around it. Every grid is `size × size`, indexed `x * size + y`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tiles {
    /// Tiles per side: 64 for a region alone, 192 stitched.
    pub size: usize,
    /// Where the region's tile (0, 0) is: 0 alone, 64 stitched.
    pub origin: usize,
    /// Tile heights, negative up.
    pub heights: Vec<i32>,
    /// Underlay ids (floor id + 1, 0 for none).
    pub underlays: Vec<u8>,
    /// Overlay ids (floor id + 1, 0 for none).
    pub overlays: Vec<u8>,
    /// Overlay shapes, 0–11.
    pub shapes: Vec<u8>,
    /// Overlay rotations, 0–3.
    pub rotations: Vec<u8>,
}

impl Tiles {
    /// A region's plane on its own.
    pub fn region(plane: &Plane) -> Self {
        Self {
            size: REGION_SIZE,
            origin: 0,
            heights: plane.heights.clone(),
            underlays: plane.underlays.clone(),
            overlays: plane.overlays.clone(),
            shapes: plane.shapes.clone(),
            rotations: plane.rotations.clone(),
        }
    }

    /// A region's plane with its neighbours' around it, as a 3 × 3 block
    /// of regions. `around[i][j]` is the region `i − 1` regions east and
    /// `j − 1` north (the centre slot is ignored for `centre`), or `None`
    /// where there's no region.
    ///
    /// A missing neighbour has no floors, and its heights are the nearest
    /// present tile's: across its edge, or the centre region's corner.
    pub fn stitched(centre: &Plane, around: [[Option<&Plane>; 3]; 3]) -> Self {
        let r = REGION_SIZE;
        let size = 3 * r;
        let plane = |i: usize, j: usize| {
            if (i, j) == (1, 1) {
                Some(centre)
            } else {
                around[i][j]
            }
        };
        let mut t = Self {
            size,
            origin: r,
            heights: vec![0; size * size],
            underlays: vec![0; size * size],
            overlays: vec![0; size * size],
            shapes: vec![0; size * size],
            rotations: vec![0; size * size],
        };
        // Clamp a coordinate into the centre region's tiles.
        let inner = |v: usize| v.clamp(r, 2 * r - 1);
        for x in 0..size {
            for y in 0..size {
                let k = x * size + y;
                let local = |x: usize, y: usize| (x % r) * r + y % r;
                if let Some(p) = plane(x / r, y / r) {
                    let i = local(x, y);
                    t.heights[k] = p.heights[i];
                    t.underlays[k] = p.underlays[i];
                    t.overlays[k] = p.overlays[i];
                    t.shapes[k] = p.shapes[i];
                    t.rotations[k] = p.rotations[i];
                    continue;
                }
                // No region here: borrow the nearest height, trying the
                // region across one axis, then the other, then the centre.
                // The centre is always there, so the last try never fails.
                t.heights[k] = [(inner(x), y), (x, inner(y)), (inner(x), inner(y))]
                    .into_iter()
                    .find_map(|(bx, by)| Some(plane(bx / r, by / r)?.heights[local(bx, by)]))
                    .unwrap_or_default();
            }
        }
        t
    }

    /// The region's tile `(x, y)`'s index in the grids.
    fn index(&self, x: usize, y: usize) -> usize {
        (self.origin + x) * self.size + self.origin + y
    }

    /// The height at grid point `(x, y)` of the whole grid; past the last
    /// tile it reuses the edge tile's height.
    fn height(&self, x: usize, y: usize) -> i32 {
        self.heights[x.min(self.size - 1) * self.size + y.min(self.size - 1)]
    }
}

/// The light at each of the region's grid points, indexed `x * GRID + y`:
/// 84 on level ground, more on slopes facing the light (which comes from
/// the −x and −z side, a little from above), less facing away, minus the
/// shadow around the point.
///
/// `shadows` is the plane's shadow grid over the same tiles (see
/// [`Shadows`]), or `None` for none. Heights and shadows are read across
/// the region's edges when `tiles` has its neighbours.
pub fn light_grid(tiles: &Tiles, shadows: Option<&[u8]>) -> Vec<i32> {
    let side = tiles.size as isize + 1;
    let shadow = |x: isize, y: isize| -> i32 {
        match shadows {
            Some(s) if (0..side).contains(&x) && (0..side).contains(&y) => {
                i32::from(s[(x * side + y) as usize])
            }
            _ => 0,
        }
    };
    let mut out = vec![0; GRID * GRID];
    for x in 0..GRID {
        for y in 0..GRID {
            // The point in the whole grid.
            let (gx, gy) = (tiles.origin + x, tiles.origin + y);
            let h = |dx: isize, dy: isize| {
                let at = |v: usize, d: isize| v.saturating_add_signed(d);
                tiles.height(at(gx, dx), at(gy, dy))
            };
            // Height differences across the point, both ways; heights
            // grow downwards, so these lean the normal uphill.
            let dx = h(1, 0) - h(-1, 0);
            let dy = h(0, 1) - h(0, -1);
            let len = f64::from(dx * dx + 65536 + dy * dy).sqrt() as i32;
            let (nx, ny, nz) = ((dx << 8) / len, 65536 / len, (dy << 8) / len);
            // Light (−50, −10, −50): length 71, and 768 · 71 / 256 = 213.
            let lit = 96 + (-50 * nx - 10 * ny - 50 * nz) / 213;
            let (xi, yi) = (gx as isize, gy as isize);
            // Shadow is weighted towards the point itself and its −x and
            // −y neighbours.
            let dark = (shadow(xi - 1, yi) >> 2)
                + (shadow(xi + 1, yi) >> 3)
                + (shadow(xi, yi - 1) >> 2)
                + (shadow(xi, yi + 1) >> 3)
                + (shadow(xi, yi) >> 1);
            out[x * GRID + y] = lit - dark;
        }
    }
    out
}

/// The shadow walls and scenery cast on the ground, one grid per plane,
/// each with one point per tile corner (`side × side`, indexed
/// `x * side + y`), 0 (none) to 127.
///
/// Feed it the locations in file order: a wall sets its points outright,
/// so a later one wins. For stitched [`Tiles`], size it to match and add
/// the neighbours' locations too, at their place in the block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shadows {
    /// Grid points per side: tiles plus one.
    pub side: usize,
    /// Plane 0 to 3.
    pub planes: [Vec<u8>; 4],
}

impl Default for Shadows {
    /// Shadows over one region.
    fn default() -> Self {
        Self::new(REGION_SIZE)
    }
}

/// How dark a wall's ends make the ground.
const WALL_SHADOW: u8 = 50;

impl Shadows {
    /// No shadows over `tiles` tiles per side.
    pub fn new(tiles: usize) -> Self {
        let side = tiles + 1;
        Self {
            side,
            planes: std::array::from_fn(|_| vec![0; side * side]),
        }
    }

    fn set(&mut self, plane: u8, x: usize, y: usize, v: u8, max: bool) {
        let side = self.side;
        let Some(p) = self.planes.get_mut(usize::from(plane)) else {
            return;
        };
        if x < side && y < side {
            let s = &mut p[x * side + y];
            *s = if max { (*s).max(v) } else { v };
        }
    }

    /// A location that casts a shadow (its definition says so). Straight
    /// walls (type 0) darken both ends of the tile edge they stand on,
    /// wall corners (types 1 and 3) the one tile corner they fill, and
    /// scenery (types 10 and 11) every grid point of its footprint, edges
    /// included, by a quarter of the model's horizontal radius (see
    /// [`horizontal_radius`]) up to 30. Other types cast none.
    ///
    /// `(x, y)` is the location's tile in the grid and `size` its
    /// definition's footprint, before turning.
    pub fn add(
        &mut self,
        plane: u8,
        (x, y): (usize, usize),
        kind: u8,
        rotation: u8,
        size: (u8, u8),
        radius: i32,
    ) {
        // Corners of the tile going round from (x, y + 1): the end each
        // rotation's edge or corner starts from.
        let corner = |r: u8| match r & 3 {
            0 => (x, y + 1),
            1 => (x + 1, y + 1),
            2 => (x + 1, y),
            _ => (x, y),
        };
        match kind {
            0 => {
                // A straight wall runs along the west, north, east or
                // south edge.
                let (a, b) = match rotation & 3 {
                    0 => ((x, y), (x, y + 1)),
                    1 => ((x, y + 1), (x + 1, y + 1)),
                    2 => ((x + 1, y), (x + 1, y + 1)),
                    _ => ((x, y), (x + 1, y)),
                };
                self.set(plane, a.0, a.1, WALL_SHADOW, false);
                self.set(plane, b.0, b.1, WALL_SHADOW, false);
            }
            1 | 3 => {
                let (cx, cy) = corner(rotation);
                self.set(plane, cx, cy, WALL_SHADOW, false);
            }
            10 | 11 => {
                let v = (radius / 4).clamp(0, 30) as u8;
                let (w, l) = if rotation & 1 == 1 {
                    (size.1, size.0)
                } else {
                    (size.0, size.1)
                };
                for i in 0..=usize::from(w) {
                    for j in 0..=usize::from(l) {
                        self.set(plane, x + i, y + j, v, true);
                    }
                }
            }
            _ => {}
        }
    }
}

/// A model's horizontal radius, as the original client measures it for
/// shadows: the square root of the largest `x² + z²`, rounded up.
pub fn horizontal_radius(vertices: &[[i32; 3]]) -> i32 {
    let max = vertices
        .iter()
        .map(|v| i64::from(v[0]).pow(2) + i64::from(v[2]).pow(2))
        .max()
        .unwrap_or(0);
    ((max as f64).sqrt() + 0.99) as i32
}

/// A plane's ground, ready to draw.
#[derive(Debug, Clone)]
pub struct Ground {
    /// The triangles: kind 0 (gouraud, lit HSL per corner) or 2
    /// (textured, a shade per corner), with each textured face's P/M/N
    /// triangle on tile corners.
    pub model: LitModel,
    /// Vertex positions in cache coordinates, with the region's tile
    /// (0, 0) at the origin.
    pub vertices: Vec<[i32; 3]>,
    /// The light at each of the region's grid points, from
    /// [`light_grid`].
    pub light: Vec<i32>,
    /// Each tile's faces, indexed `x * 64 + y`: a range into the model's
    /// faces, empty for a tile with none. A tile's faces are listed in the
    /// order the original client draws them (see
    /// [`draw_in_order`](crate::render::draw_in_order)).
    pub tile_faces: Vec<std::ops::Range<usize>>,
}

/// Marks a corner that isn't drawn (no underlay, or a hidden overlay).
const NONE: i32 = 12_345_678;

/// Points a tile is cut at, numbered 1–16: eight round the edge from the
/// south-west corner (corners odd, edge midpoints even), the four inner
/// midpoints (south, east, north, west), then the four inner corners
/// (south-west, south-east, north-east, north-west). Offsets are within
/// the tile, x east and z north.
const POINTS: [(i32, i32); 16] = [
    (0, 0),
    (64, 0),
    (128, 0),
    (128, 64),
    (128, 128),
    (64, 128),
    (0, 128),
    (0, 64),
    (64, 32),
    (96, 64),
    (64, 96),
    (32, 64),
    (32, 32),
    (96, 32),
    (96, 96),
    (32, 96),
];

/// Which triangles of a shaped tile are underlay and which overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Under,
    Over,
}

/// One overlay shape: the points it uses beyond the four corners, and its
/// triangles. A triangle's corners are 0–3 for the tile corners
/// (south-west, south-east, north-east, north-west) and 4 and up for the
/// extra points in order.
struct Shape {
    extra: &'static [u8],
    faces: &'static [(Part, [u8; 3])],
}

use Part::{Over as O, Under as U};

/// Shapes 1–11 (shape 0 is the whole tile).
const SHAPES: [Shape; 11] = [
    Shape {
        extra: &[],
        faces: &[(U, [1, 2, 3]), (O, [0, 1, 3])],
    },
    Shape {
        extra: &[6],
        faces: &[(U, [0, 1, 2]), (U, [0, 2, 4]), (O, [0, 4, 3])],
    },
    Shape {
        extra: &[6],
        faces: &[(U, [0, 1, 4]), (U, [0, 4, 3]), (O, [1, 2, 4])],
    },
    Shape {
        extra: &[6],
        faces: &[(U, [0, 4, 3]), (O, [0, 1, 2]), (O, [0, 2, 4])],
    },
    Shape {
        extra: &[6],
        faces: &[(U, [1, 2, 4]), (O, [0, 1, 4]), (O, [0, 4, 3])],
    },
    Shape {
        extra: &[2, 6],
        faces: &[
            (U, [4, 1, 2]),
            (U, [4, 2, 5]),
            (O, [0, 4, 5]),
            (O, [0, 5, 3]),
        ],
    },
    Shape {
        extra: &[2, 8],
        faces: &[
            (U, [4, 1, 2]),
            (U, [4, 2, 3]),
            (U, [4, 3, 5]),
            (O, [0, 4, 5]),
        ],
    },
    Shape {
        extra: &[2, 8],
        faces: &[
            (U, [0, 4, 5]),
            (O, [4, 1, 2]),
            (O, [4, 2, 3]),
            (O, [4, 3, 5]),
        ],
    },
    Shape {
        extra: &[11, 12],
        faces: &[
            (U, [0, 1, 5]),
            (U, [1, 4, 5]),
            (U, [1, 2, 4]),
            (O, [0, 5, 3]),
            (O, [5, 4, 3]),
            (O, [4, 2, 3]),
        ],
    },
    Shape {
        extra: &[11, 12],
        faces: &[
            (O, [0, 1, 5]),
            (O, [1, 4, 5]),
            (O, [1, 2, 4]),
            (U, [0, 5, 3]),
            (U, [5, 4, 3]),
            (U, [4, 2, 3]),
        ],
    },
    Shape {
        extra: &[13, 14],
        faces: &[
            (O, [0, 5, 4]),
            (O, [0, 1, 5]),
            (U, [0, 4, 3]),
            (U, [4, 5, 3]),
            (U, [5, 2, 3]),
            (U, [1, 2, 5]),
        ],
    },
];

/// Point `p` (1–16) after `r` quarter turns of the tile, each taking the
/// south-west corner to the north-west one: points step backwards round
/// their ring, two places on the outer ring of eight, one on each inner
/// ring of four.
fn turn(p: u8, r: u8) -> u8 {
    let r = r & 3;
    match p {
        1..=8 => (p - 1 + 8 - 2 * r) % 8 + 1,
        9..=12 => (p - 9 + 4 - r) % 4 + 9,
        _ => (p - 13 + 4 - r) % 4 + 13,
    }
}

/// The values at a tile's four corners (south-west, south-east,
/// north-east, north-west).
type Corners = [i32; 4];

/// A cut point's value from the corner values: a corner's own, an edge
/// midpoint's the mean of its two corners (an arithmetic shift, even on
/// packed colours), and an inner point the same as the nearest edge point.
fn at_point(c: Corners, p: u8) -> i32 {
    let mid = |a: usize, b: usize| (c[a] + c[b]) >> 1;
    match p {
        1 | 13 => c[0],
        3 | 14 => c[1],
        5 | 15 => c[2],
        7 | 16 => c[3],
        2 | 9 => mid(0, 1),
        4 | 10 => mid(1, 2),
        6 | 11 => mid(2, 3),
        _ => mid(3, 0),
    }
}

/// Collects a plane's triangles.
#[derive(Default)]
struct Builder {
    vertices: Vec<[i32; 3]>,
    faces: Vec<[u16; 3]>,
    kinds: Vec<u8>,
    colours: Vec<[i32; 3]>,
    base: Vec<u16>,
    texture_triple: Vec<usize>,
    textured_faces: Vec<[u16; 3]>,
    /// The tile the next faces belong to, and each face's tile.
    tile: usize,
    face_tiles: Vec<usize>,
}

impl Builder {
    /// A gouraud triangle, unless its first corner isn't drawn.
    fn plain(&mut self, v: [u16; 3], c: [i32; 3]) {
        if c[0] == NONE {
            return;
        }
        self.push(v, 0, c, 0, None);
    }

    /// A textured triangle with texture `t` laid out on `pmn`.
    fn textured(&mut self, v: [u16; 3], shades: [i32; 3], t: u8, pmn: [u16; 3]) {
        self.push(v, 2, shades, u16::from(t), Some(pmn));
    }

    fn push(&mut self, v: [u16; 3], kind: u8, c: [i32; 3], base: u16, pmn: Option<[u16; 3]>) {
        self.faces.push(v);
        self.face_tiles.push(self.tile);
        self.kinds.push(kind);
        self.colours.push(c);
        self.base.push(base);
        self.texture_triple.push(self.textured_faces.len());
        if let Some(t) = pmn {
            self.textured_faces.push(t);
        }
    }
}

/// A plane's ground: every tile with an underlay or an overlay, lit and
/// coloured as the original client colours it. `None` when there's
/// nothing to draw.
///
/// Only the region's own tiles are drawn; neighbours in `tiles` only
/// inform them. `floors` is indexed by floor id; `shadows` is the plane's
/// grid from [`Shadows`] over the same tiles, or `None`.
pub fn ground(tiles: &Tiles, floors: &[FloorColour], shadows: Option<&[u8]>) -> Option<Ground> {
    let under = blend_underlays(tiles, floors);
    let light = light_grid(tiles, shadows);
    let mut b = Builder::default();
    for x in 0..REGION_SIZE {
        for y in 0..REGION_SIZE {
            let i = x * REGION_SIZE + y;
            b.tile = i;
            let t = tiles.index(x, y);
            let over = floor_of(floors, tiles.overlays[t]);
            if under[i].is_none() && over.is_none() {
                continue;
            }
            let at = |dx: usize, dy: usize| (x + dx, y + dy);
            let corners = [at(0, 0), at(1, 0), at(1, 1), at(0, 1)];
            let h: Corners =
                corners.map(|(cx, cy)| tiles.height(tiles.origin + cx, tiles.origin + cy));
            let l: Corners = corners.map(|(cx, cy)| light[cx * GRID + cy]);
            let u: Corners = match under[i] {
                Some(c) => l.map(|v| shade(c, v)),
                None => [NONE; 4],
            };
            // The overlay's corners: shades for a textured floor, nothing
            // for a hidden one, otherwise its own colour lit.
            let (o, texture): (Corners, Option<u8>) = match over {
                None => ([NONE; 4], None),
                Some(f) if f.texture.is_some() => (l.map(texture_shade), f.texture),
                Some(f) if f.hidden => ([NONE; 4], None),
                Some(f) => (l.map(|v| shade(f.hsl(), v)), None),
            };
            let flat = h.iter().all(|&v| v == h[0]);
            let shape = tiles.shapes[t];
            let rotation = tiles.rotations[t];
            let place = |p: u8| {
                let (dx, dz) = POINTS[usize::from(p - 1)];
                [x as i32 * 128 + dx, at_point(h, p), y as i32 * 128 + dz]
            };
            let first = b.vertices.len() as u16;
            for p in [1, 3, 5, 7] {
                b.vertices.push(place(p));
            }
            // The south-west, south-east and north-west corners: the
            // texture's layout on a level tile.
            let level = [first, first + 1, first + 3];
            if over.is_none() || shape == 0 {
                // A whole tile, split along the south-east to north-west
                // diagonal: the north-east half first.
                let c = if over.is_some() { o } else { u };
                let [sw, se, ne, nw] = [0, 1, 2, 3].map(|k| first + k);
                let halves = [
                    ([ne, nw, se], [c[2], c[3], c[1]]),
                    ([sw, se, nw], [c[0], c[1], c[3]]),
                ];
                for (n, (v, cs)) in halves.into_iter().enumerate() {
                    match texture {
                        Some(t) => {
                            // A sloping tile lays the north-east half's
                            // texture from its own corners.
                            let pmn = if n == 0 && !flat { v } else { level };
                            b.textured(v, cs, t, pmn);
                        }
                        None => b.plain(v, cs),
                    }
                }
                continue;
            }
            let Some(s) = SHAPES.get(usize::from(shape) - 1) else {
                continue;
            };
            let extra: Vec<u8> = s.extra.iter().map(|&p| turn(p, rotation)).collect();
            for &p in &extra {
                b.vertices.push(place(p));
            }
            // Every point's id and vertex: corners turn with the shape.
            let point = |k: u8| -> (u8, u16) {
                if k < 4 {
                    let c = (k + 4 - (rotation & 3)) % 4;
                    (2 * c + 1, first + u16::from(c))
                } else {
                    let e = usize::from(k - 4);
                    (extra[e], first + 4 + e as u16)
                }
            };
            for &(part, ks) in s.faces {
                let pts = ks.map(point);
                let v = pts.map(|(_, v)| v);
                let colours = |c: Corners| pts.map(|(p, _)| at_point(c, p));
                match (part, texture) {
                    (Part::Over, Some(t)) => {
                        let pmn = if flat { level } else { v };
                        b.textured(v, colours(o), t, pmn);
                    }
                    (Part::Over, None) => b.plain(v, colours(o)),
                    (Part::Under, _) => b.plain(v, colours(u)),
                }
            }
        }
    }
    if b.faces.is_empty() {
        return None;
    }
    let n = b.faces.len();
    // Tiles are built in order, so each one's faces are contiguous.
    let mut tile_faces = vec![0..0; REGION_SIZE * REGION_SIZE];
    for (f, &t) in b.face_tiles.iter().enumerate() {
        let r = &mut tile_faces[t];
        if r.start == r.end {
            *r = f..f;
        }
        r.end = f + 1;
    }
    Some(Ground {
        model: LitModel {
            faces: b.faces,
            kinds: b.kinds,
            colours: b.colours,
            base: b.base,
            texture_triple: b.texture_triple,
            textured_faces: b.textured_faces,
            alphas: vec![0; n],
            priorities: None,
        },
        vertices: b.vertices,
        light,
        tile_faces,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane() -> Plane {
        let n = REGION_SIZE * REGION_SIZE;
        Plane {
            heights: vec![0; n],
            height_derived: vec![false; n],
            overlays: vec![0; n],
            shapes: vec![0; n],
            rotations: vec![0; n],
            flags: vec![0; n],
            underlays: vec![0; n],
        }
    }

    #[test]
    fn floor_colour_terms() {
        // Pure red: hue 0, full saturation, half lightness.
        let red = FloorColour::from_rgb(0xff0000);
        assert_eq!((red.hue, red.saturation, red.lightness), (0, 255, 127));
        assert_eq!(red.chroma, 255);
        assert_eq!(red.hue_weight, 0);
        // A red leaning blue has a slightly negative hue.
        let rose = FloorColour::from_rgb(0xff0040);
        assert!(rose.hue < 0 && rose.hue_weight < 0);
        // Grey has no hue or saturation, but chroma is at least 1.
        let grey = FloorColour::from_rgb(0x808080);
        assert_eq!((grey.hue, grey.saturation, grey.chroma), (0, 0, 1));
        // Light colours lose saturation when packed.
        assert_eq!(pack_hsl(0, 255, 100) >> 7 & 7, 7);
        assert_eq!(pack_hsl(0, 255, 250) >> 7 & 7, 0);
    }

    #[test]
    fn shade_clamps_lightness() {
        let c = pack_hsl(128, 128, 200);
        assert_eq!(shade(c, 128) & 0x7f, 100);
        assert_eq!(shade(c, 0) & 0x7f, 2);
        assert_eq!(shade(c, 1000) & 0x7f, 126);
        assert_eq!(shade(c, 128) & 0xff80, c & 0xff80);
        // A negative hue still lands in 16 bits.
        assert!((0..65536).contains(&shade(pack_hsl(-40, 200, 100), 96)));
    }

    #[test]
    fn blend_window_is_offset_and_skips_empty_tiles() {
        let mut p = plane();
        let floors = [
            FloorColour::from_rgb(0x000000),
            FloorColour::from_rgb(0xfefefe),
        ];
        // Black everywhere on rows below y = 32, white from 32 up.
        for x in 0..64 {
            for y in 0..64 {
                p.underlays[x * 64 + y] = if y < 32 { 1 } else { 2 };
            }
        }
        let b = blend_underlays(&Tiles::region(&p), &floors);
        let light = |x: usize, y: usize| b[x * 64 + y].unwrap() & 0x7f;
        // The window reaches 5 tiles up but only 4 down, so the row just
        // below the boundary sees 5 white rows of 10, the row above it 6.
        assert!(light(10, 31) < light(10, 32));
        assert_eq!(light(10, 26), 0, "y + 5 = 31 is still black");
        assert!(light(10, 27) > 0, "y + 5 = 32 reaches white");
        assert_eq!(light(10, 36), 127, "y − 4 = 32 is all white");
        assert!(light(10, 35) < 127, "y − 4 = 31 still sees black");
        // A tile without an underlay isn't coloured and doesn't count.
        p.underlays[10 * 64 + 40] = 0;
        let b = blend_underlays(&Tiles::region(&p), &floors);
        assert_eq!(b[10 * 64 + 40], None);
        assert_eq!(b[10 * 64 + 41].unwrap() & 0x7f, 127);
    }

    #[test]
    fn level_ground_gets_84_and_slopes_face_the_light() {
        let mut p = plane();
        assert!(
            light_grid(&Tiles::region(&p), None)
                .iter()
                .all(|&l| l == 84)
        );
        // Ground rising towards +x (heights grow downwards) faces −x,
        // where the light comes from; ground falling that way faces away.
        for x in 0..64 {
            for y in 0..64 {
                p.heights[x * 64 + y] = -(x as i32) * 32;
            }
        }
        assert!(light_grid(&Tiles::region(&p), None)[10 * GRID + 10] > 84);
        for x in 0..64 {
            for y in 0..64 {
                p.heights[x * 64 + y] = (x as i32) * 32;
            }
        }
        assert!(light_grid(&Tiles::region(&p), None)[10 * GRID + 10] < 84);
    }

    #[test]
    fn shadows_darken_their_points() {
        let mut s = Shadows::default();
        // A straight wall on the west edge of tile (5, 5).
        s.add(0, (5, 5), 0, 0, (1, 1), 0);
        assert_eq!(s.planes[0][5 * GRID + 5], 50);
        assert_eq!(s.planes[0][5 * GRID + 6], 50);
        assert_eq!(s.planes[0][6 * GRID + 5], 0);
        // Scenery of radius 200 with a 2 × 1 footprint, turned once: 1 × 2,
        // so it covers 2 × 3 grid points.
        s.add(1, (10, 10), 10, 1, (2, 1), 200);
        let p1 = &s.planes[1];
        assert_eq!(p1[10 * GRID + 10], 30, "capped at 30");
        assert_eq!(p1[11 * GRID + 12], 30);
        assert_eq!(p1[12 * GRID + 10], 0);
        let light = light_grid(&Tiles::region(&plane()), Some(&s.planes[0]));
        assert_eq!(light[5 * GRID + 5], 84 - 25 - 6);
        // Decorations cast none.
        let before = s.clone();
        s.add(0, (20, 20), 4, 0, (1, 1), 500);
        assert_eq!(s, before);
        assert_eq!(horizontal_radius(&[[3, 99, 4], [-1, 0, 0]]), 5);
    }

    /// Twice the signed area of a face's footprint (x east, z north).
    fn area(g: &Ground, f: usize) -> i64 {
        let [a, b, c] = g.model.faces[f].map(|v| g.vertices[usize::from(v)]);
        let (ux, uz) = (i64::from(b[0] - a[0]), i64::from(b[2] - a[2]));
        let (vx, vz) = (i64::from(c[0] - a[0]), i64::from(c[2] - a[2]));
        ux * vz - uz * vx
    }

    /// One tile at (0, 0) with an underlay and a shaped overlay.
    fn shaped(shape: u8, rotation: u8) -> Ground {
        let mut p = plane();
        p.underlays[0] = 1;
        p.overlays[0] = 2;
        p.shapes[0] = shape;
        p.rotations[0] = rotation;
        let floors = [
            FloorColour::from_rgb(0x00ff00),
            FloorColour::from_rgb(0x0000ff),
        ];
        ground(&Tiles::region(&p), &floors, None).unwrap()
    }

    #[test]
    fn every_shape_and_rotation_tiles_the_square() {
        for shape in 0..12 {
            for rotation in 0..4 {
                let g = shaped(shape, rotation);
                let mut total = 0;
                for f in 0..g.model.faces.len() {
                    let a = area(&g, f);
                    assert!(
                        a > 0,
                        "shape {shape} rotation {rotation}: face {f} faces down"
                    );
                    total += a;
                }
                assert_eq!(total, 2 * 128 * 128, "shape {shape} rotation {rotation}");
            }
        }
    }

    #[test]
    fn rotation_turns_the_overlay() {
        // Shape 1 puts the overlay on the south-west half; centroid of the
        // overlay faces, relative to the tile centre, per rotation.
        let blue = |g: &Ground, f: usize| {
            let hue = (g.model.colours[f][0] >> 10) & 63;
            hue > 32
        };
        let centre = |r| {
            let g = shaped(1, r);
            let (mut sx, mut sz, mut n) = (0, 0, 0);
            for f in (0..g.model.faces.len()).filter(|&f| blue(&g, f)) {
                for v in g.model.faces[f] {
                    let p = g.vertices[usize::from(v)];
                    sx += p[0] - 64;
                    sz += p[2] - 64;
                    n += 1;
                }
            }
            (sx.signum(), sz.signum(), n)
        };
        assert_eq!(centre(0), (-1, -1, 3), "south-west");
        assert_eq!(centre(1), (-1, 1, 3), "north-west");
        assert_eq!(centre(2), (1, 1, 3), "north-east");
        assert_eq!(centre(3), (1, -1, 3), "south-east");
    }

    #[test]
    fn hidden_overlays_and_missing_underlays_draw_nothing() {
        let mut p = plane();
        p.overlays[0] = 1;
        let floors = [FloorColour::new(&FloDef {
            id: 0,
            rgb: Some(HIDDEN_RGB),
            texture: None,
            hide_underlay: false,
            occlude: true,
            name: None,
            secondary_rgb: None,
            unknown_opcodes: Vec::new(),
        })];
        assert!(ground(&Tiles::region(&p), &floors, None).is_none());
        // A shaped overlay with no underlay draws only its overlay half.
        p.overlays[0] = 2;
        p.shapes[0] = 1;
        let floors = [floors[0], FloorColour::from_rgb(0x0000ff)];
        let g = ground(&Tiles::region(&p), &floors, None).unwrap();
        assert_eq!(g.model.faces.len(), 1);
    }

    #[test]
    fn textured_floors_are_textured_on_level_layouts() {
        let mut p = plane();
        p.overlays[0] = 1;
        let floors = [FloorColour {
            texture: Some(24),
            ..FloorColour::default()
        }];
        let g = ground(&Tiles::region(&p), &floors, None).unwrap();
        assert_eq!(g.model.kinds, [2, 2]);
        assert_eq!(g.model.base, [24, 24]);
        // Level: both halves lay the texture on SW, SE, NW.
        assert_eq!(g.model.textured_faces, [[0, 1, 3], [0, 1, 3]]);
        assert_eq!(g.model.colours[0], [127 - 84; 3]);
        // Sloping: the north-east half uses its own corners.
        p.heights[0] = -64;
        let g = ground(&Tiles::region(&p), &floors, None).unwrap();
        assert_eq!(g.model.textured_faces[0], g.model.faces[0]);
    }

    /// Two regions side by side, west and east, on bumpy ground that
    /// carries on across the boundary, with different floors.
    fn pair() -> (Plane, Plane) {
        let (mut west, mut east) = (plane(), plane());
        for x in 0..64 {
            for y in 0..64 {
                let h = |wx: i32| -(((wx * 7 + y as i32 * 3) % 23) * 8);
                west.heights[x * 64 + y] = h(x as i32);
                east.heights[x * 64 + y] = h(64 + x as i32);
                west.underlays[x * 64 + y] = 1;
                east.underlays[x * 64 + y] = 2;
            }
        }
        (west, east)
    }

    fn floors() -> [FloorColour; 2] {
        [
            FloorColour::from_rgb(0x20a020),
            FloorColour::from_rgb(0xa0a020),
        ]
    }

    #[test]
    fn stitched_neighbours_meet_without_a_seam() {
        let (west, east) = pair();
        let mut around = [[None; 3]; 3];
        around[2][1] = Some(&east);
        let w = Tiles::stitched(&west, around);
        let mut around = [[None; 3]; 3];
        around[0][1] = Some(&west);
        let e = Tiles::stitched(&east, around);
        let (gw, ge) = (
            ground(&w, &floors(), None).unwrap(),
            ground(&e, &floors(), None).unwrap(),
        );
        // The shared edge: west's x = 64 is east's x = 0, lit the same.
        for y in 0..GRID {
            assert_eq!(gw.light[64 * GRID + y], ge.light[y], "y {y}");
        }
        // And the corners there sit at the same heights.
        let edge = |g: &Ground, x: i32| {
            let mut v: Vec<[i32; 2]> = g
                .vertices
                .iter()
                .filter(|p| p[0] == x)
                .map(|p| [p[2], p[1]])
                .collect();
            v.sort();
            v.dedup();
            v
        };
        assert_eq!(edge(&gw, 64 * 128), edge(&ge, 0));
        // Alone, the west region's edge is lit differently.
        let alone = ground(&Tiles::region(&west), &floors(), None).unwrap();
        assert_ne!(alone.light[64 * GRID..], gw.light[64 * GRID..]);
        // The blend reaches across: the west edge tiles take on some of
        // the east floor's colour, the ones further in don't.
        let (b, a) = (
            blend_underlays(&w, &floors()),
            blend_underlays(&Tiles::region(&west), &floors()),
        );
        assert_ne!(b[63 * 64 + 10], a[63 * 64 + 10]);
        assert_eq!(b[58 * 64 + 10], a[58 * 64 + 10], "x + 5 = 63 stays inside");
        assert_ne!(b[59 * 64 + 10], a[59 * 64 + 10], "x + 5 = 64 reaches east");
    }

    #[test]
    fn stitching_without_neighbours_changes_nothing() {
        let (west, _) = pair();
        let alone = ground(&Tiles::region(&west), &floors(), None).unwrap();
        let stitched = ground(&Tiles::stitched(&west, [[None; 3]; 3]), &floors(), None).unwrap();
        assert_eq!(alone.vertices, stitched.vertices);
        assert_eq!(alone.model.colours, stitched.model.colours);
        assert_eq!(alone.light, stitched.light);
    }

    #[test]
    fn a_neighbours_wall_shades_the_shared_edge() {
        let (west, east) = pair();
        let mut around = [[None; 3]; 3];
        around[2][1] = Some(&east);
        let w = Tiles::stitched(&west, around);
        let mut s = Shadows::new(w.size);
        assert_eq!(s.side, 193);
        // A wall on the west edge of the east region's tile (0, 10): grid
        // points (64, 10) and (64, 11) of the west region.
        s.add(0, (128, 64 + 10), 0, 0, (1, 1), 0);
        let lit = light_grid(&w, Some(&s.planes[0]));
        let unlit = light_grid(&w, None);
        assert_eq!(lit[64 * GRID + 10], unlit[64 * GRID + 10] - 25 - 6);
        assert_eq!(lit[64 * GRID + 20], unlit[64 * GRID + 20]);
    }

    #[test]
    fn each_tile_lists_its_own_faces() {
        let (west, _) = pair();
        let mut p = west;
        p.overlays[3 * 64 + 4] = 2;
        p.shapes[3 * 64 + 4] = 9;
        let g = ground(&Tiles::region(&p), &floors(), None).unwrap();
        // Every face belongs to exactly one tile, in tile order.
        let mut next = 0;
        for r in &g.tile_faces {
            if !r.is_empty() {
                assert_eq!(r.start, next);
                next = r.end;
            }
        }
        assert_eq!(next, g.model.faces.len());
        // A whole tile has two faces; the shaped one has six.
        assert_eq!(g.tile_faces[0].len(), 2);
        assert_eq!(g.tile_faces[3 * 64 + 4].len(), 6);
        // And they sit on that tile.
        for f in g.tile_faces[3 * 64 + 4].clone() {
            for v in g.model.faces[f] {
                let [x, _, z] = g.vertices[usize::from(v)];
                assert!((384..=512).contains(&x) && (512..=640).contains(&z));
            }
        }
    }

    #[test]
    fn turning_points_round_their_rings() {
        // A quarter turn takes the south-west corner to the north-west,
        // the south midpoint to the west, and so on round each ring.
        assert_eq!(turn(1, 1), 7);
        assert_eq!(turn(2, 1), 8);
        assert_eq!(turn(9, 1), 12);
        assert_eq!(turn(13, 1), 16);
        for p in 1..=16 {
            assert_eq!(turn(turn(p, 1), 3), p);
            assert_eq!(turn(p, 4), p);
        }
    }
}
