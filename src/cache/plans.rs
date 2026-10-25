//! Floor plans of structures, for looking at them and for labelling them:
//! a picture of every plane side by side ([`plan`]) and a text sheet
//! listing what's in each room and what stands nearby ([`sheet`]).
//!
//! A plan is drawn from above, one tile to [`TILE`] pixels, north up, each
//! plane in its own panel from the lowest. Roofed rooms are tinted by their
//! floor, yards are pale grey, walls are coloured by what they're made of,
//! closed doors are green and open ones pale green, openings red, and each
//! furnishing is a square in its category's colour ([`CATEGORY_COLOURS`]),
//! with minimap icons as yellow diamonds. Room numbers are drawn in each
//! room; the sheet says the rest.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use image::{Rgba, RgbaImage};
use rs2convert::furnish::{self, Category};
use rs2convert::material::{Dressing, Material};
use rs2convert::purpose::Purpose;
use rs2convert::structure::{self, Placed, Structure};

/// Pixels to a tile.
pub const TILE: u32 = 16;

/// The colour each furnishing category's squares are drawn in, `0xRRGGBB`.
pub const CATEGORY_COLOURS: &[(Category, u32)] = &[
    (Category::Bank, 0xFFD700),
    (Category::Bar, 0xB8860B),
    (Category::Smithing, 0x404040),
    (Category::Crafting, 0x8B4513),
    (Category::Worship, 0x9370DB),
    (Category::Throne, 0xFF00FF),
    (Category::Counter, 0xFF8C00),
    (Category::Display, 0xFFA07A),
    (Category::Military, 0xB22222),
    (Category::Ship, 0x1E90FF),
    (Category::Mill, 0xDEB887),
    (Category::Farm, 0x9ACD32),
    (Category::Magic, 0x00CED1),
    (Category::Books, 0x800000),
    (Category::Cooking, 0xFF4500),
    (Category::Washing, 0x87CEEB),
    (Category::Bed, 0x4169E1),
    (Category::Seat, 0xA0522D),
    (Category::Table, 0xD2691E),
    (Category::Storage, 0x808000),
    (Category::Hearth, 0xDC143C),
    (Category::Light, 0xFFFF66),
    (Category::Decoration, 0xEE82EE),
    (Category::Plant, 0x228B22),
    (Category::Grave, 0x696969),
    (Category::Prison, 0x2F4F4F),
    (Category::Fixture, 0xC0C0C0),
    (Category::Nature, 0x556B2F),
    (Category::Stairs, 0x000080),
    (Category::Remains, 0xF5F5DC),
];

fn category_colour(c: Category) -> u32 {
    CATEGORY_COLOURS
        .iter()
        .find(|(cc, _)| *cc == c)
        .map_or(0x888888, |&(_, rgb)| rgb)
}

/// The colour a material is drawn in: a plain one's own colour, otherwise
/// a fixed colour per material.
fn material_colour(d: Option<&Dressing>) -> u32 {
    match d {
        None => 0x202020,
        Some(d) => match (d.material, d.colour) {
            (Material::Plain, Some(c)) => c,
            (Material::Wood, _) => 0x8B5A2B,
            (Material::Stone | Material::Cobbles, _) => 0x6E6E6E,
            (Material::Brick, _) => 0xA0402A,
            (Material::Marble, _) => 0xE8E8E8,
            (Material::Sandstone | Material::Sand, _) => 0xD8C080,
            (Material::Plaster, _) => 0xF0E8D0,
            (Material::Thatch, _) => 0xC8A040,
            (Material::RoofTiles, _) => 0xB04030,
            (Material::Metal, _) => 0x505868,
            (Material::Mud, _) => 0x6B4A2A,
            (Material::Water, _) => 0x3070C0,
            (Material::Lava, _) => 0xE05010,
            _ => 0x909090,
        },
    }
}

fn rgba(rgb: u32) -> Rgba<u8> {
    Rgba([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255])
}

/// Halfway from `rgb` to white, for floors under the furniture.
fn pale(rgb: u32) -> u32 {
    let m = |s: u32| (((rgb >> s) & 0xFF) + 255) / 2;
    m(16) << 16 | m(8) << 8 | m(0)
}

/// A small canvas that clips what's drawn off its edges.
struct Canvas(RgbaImage);

impl Canvas {
    fn rect(&mut self, x0: i64, y0: i64, x1: i64, y1: i64, rgb: u32) {
        let c = rgba(rgb);
        let (w, h) = (i64::from(self.0.width()), i64::from(self.0.height()));
        for y in y0.max(0)..y1.min(h) {
            for x in x0.max(0)..x1.min(w) {
                self.0.put_pixel(x as u32, y as u32, c);
            }
        }
    }

    /// A line from `(x0, y0)` to `(x1, y1)`, `width` pixels thick.
    fn line(&mut self, (x0, y0): (i64, i64), (x1, y1): (i64, i64), width: i64, rgb: u32) {
        let steps = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        let r = width / 2;
        for i in 0..=steps {
            let x = x0 + (x1 - x0) * i / steps;
            let y = y0 + (y1 - y0) * i / steps;
            self.rect(x - r, y - r, x - r + width, y - r + width, rgb);
        }
    }

    /// Digits in a 3 × 5 pixel font, each pixel `scale` wide.
    fn number(&mut self, n: usize, x: i64, y: i64, scale: i64, rgb: u32) {
        for (k, ch) in n.to_string().bytes().enumerate() {
            let glyph = DIGITS[usize::from(ch - b'0')];
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..3 {
                    if bits >> (2 - col) & 1 == 1 {
                        let px = x + (k as i64 * 4 + col) * scale;
                        let py = y + row as i64 * scale;
                        self.rect(px, py, px + scale, py + scale, rgb);
                    }
                }
            }
        }
    }
}

/// The digits 0–9, five rows of three pixels each, most significant bit
/// leftmost.
const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b010, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

/// Draws every plane of `s` side by side, lowest first, with a margin of
/// one tile round each panel and the plane's number in its corner.
pub fn plan(s: &Structure) -> RgbaImage {
    let (w, h) = (u32::from(s.size.0) + 2, u32::from(s.size.1) + 2);
    let panels = s.planes.len().max(1) as u32;
    let gap = TILE;
    let mut c = Canvas(RgbaImage::from_pixel(
        panels * w * TILE + (panels - 1) * gap,
        h * TILE,
        rgba(0xFFFFFF),
    ));
    for (k, &plane) in s.planes.iter().enumerate() {
        let ox = i64::from(k as u32 * (w * TILE + gap));
        panel(&mut c, s, plane, ox, h);
    }
    c.0
}

/// One plane's panel, its left edge at `ox`, `h` tiles tall.
fn panel(c: &mut Canvas, s: &Structure, plane: u8, ox: i64, h: u32) {
    let t = i64::from(TILE);
    // Tile (x, y) of the structure, north up, inside the one-tile margin.
    let px = |x: i64| ox + (x + 1) * t;
    let py = |y: i64| i64::from(h) * t - (y + 2) * t;
    c.number(usize::from(plane), ox + 2, 2, 2, 0x000000);
    for r in s.rooms.iter().filter(|r| r.plane == plane) {
        let fill = if r.roofed {
            pale(material_colour(r.floor.as_ref()))
        } else {
            0xE4E4E4
        };
        for tile in &r.tiles {
            let (x, y) = (px(i64::from(tile.x)), py(i64::from(tile.y)));
            c.rect(x, y, x + t, y + t, fill);
            // Faint tile lines, so sizes can be counted.
            c.rect(x, y, x + t, y + 1, 0xD0D0D0);
            c.rect(x, y, x + 1, y + t, 0xD0D0D0);
        }
    }
    let on = |p: &Placed| p.plane == plane;
    // Furnishings and icons under the walls.
    for o in s.objects.iter().filter(|o| on(o)) {
        let (x, y) = (px(i64::from(o.x)), py(i64::from(o.y)));
        if o.icon.is_some() {
            let (cx, cy) = (x + t / 2, y + t / 2);
            for d in 0..5 {
                c.rect(cx - d, cy - 4 + d, cx + d + 1, cy - 3 + d, 0xE0C000);
                c.rect(cx - d, cy + 4 - d, cx + d + 1, cy + 5 - d, 0xE0C000);
            }
        } else if matches!(o.kind, 10 | 11 | 22) {
            if let Some(cat) = o.category {
                let m = t / 4;
                c.rect(x + m, y + m, x + t - m, y + t - m, category_colour(cat));
            }
        } else if let Some(cat) = o.category {
            // A wall decoration: a smaller square.
            let m = t * 3 / 8;
            c.rect(x + m, y + m, x + t - m, y + t - m, category_colour(cat));
        }
    }
    // An edge of tile (x, y): 0 west, 1 north, 2 east, 3 south.
    let edge = |c: &mut Canvas, x: i64, y: i64, side: u8, width: i64, rgb: u32| {
        let (l, r, top, bottom) = (px(x), px(x) + t, py(y), py(y) + t);
        let (a, b) = match side & 3 {
            0 => ((l, top), (l, bottom)),
            1 => ((l, top), (r, top)),
            2 => ((r, top), (r, bottom)),
            _ => ((l, bottom), (r, bottom)),
        };
        c.line(a, b, width, rgb);
    };
    for o in s.openings.iter().filter(|o| o.plane == plane) {
        edge(c, i64::from(o.x), i64::from(o.y), o.side, 2, 0xE00000);
    }
    for w in s.walls.iter().filter(|w| on(w)) {
        let (x, y) = (i64::from(w.x), i64::from(w.y));
        let colour = material_colour(w.material.as_ref());
        match w.kind {
            0 => edge(c, x, y, w.rotation, 3, colour),
            2 => {
                edge(c, x, y, w.rotation, 3, colour);
                edge(c, x, y, w.rotation + 1, 3, colour);
            }
            9 => {
                let (l, r, top, bottom) = (px(x), px(x) + t, py(y), py(y) + t);
                if w.rotation & 1 == 0 {
                    c.line((l, bottom), (r, top), 3, colour);
                } else {
                    c.line((l, top), (r, bottom), 3, colour);
                }
            }
            _ => {
                // A corner post at the corner its rotation names.
                let (cx, cy) = match w.rotation & 3 {
                    0 => (px(x), py(y)),
                    1 => (px(x) + t, py(y)),
                    2 => (px(x) + t, py(y) + t),
                    _ => (px(x), py(y) + t),
                };
                c.rect(cx - 2, cy - 2, cx + 3, cy + 3, colour);
            }
        }
    }
    for d in s.doors.iter().filter(|d| on(&d.placed)) {
        let colour = if d.open { 0x90E090 } else { 0x00A000 };
        let p = &d.placed;
        edge(c, i64::from(p.x), i64::from(p.y), p.rotation, 5, colour);
    }
    // Room numbers last, on a white patch, in the room's middle tile.
    for (i, r) in s.rooms.iter().enumerate().filter(|(_, r)| r.plane == plane) {
        if let Some(tile) = r.tiles.get(r.tiles.len() / 2) {
            let (x, y) = (px(i64::from(tile.x)), py(i64::from(tile.y)));
            let digits = i.to_string().len() as i64;
            c.rect(x + 1, y + 2, x + 3 + digits * 8, y + 14, 0xFFFFFF);
            c.number(i, x + 2, y + 3, 2, 0x000000);
        }
    }
}

/// A text sheet for `s`: its size, materials and guess, each room's
/// contents, and its `neighbours` (each with its guess and gap in tiles).
pub fn sheet(
    s: &Structure,
    purpose: &Purpose,
    neighbours: &[(&Structure, &Purpose, u32)],
) -> String {
    let mut out = String::new();
    let yards = s.rooms.iter().filter(|r| !r.roofed).count();
    let planes: Vec<String> = s.planes.iter().map(u8::to_string).collect();
    let _ = writeln!(out, "# Structure {}", structure::id(s));
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "{} × {} tiles, planes {}, {} rooms ({yards} yards), {} doors, {} open edges.",
        s.size.0,
        s.size.1,
        planes.join(", "),
        s.rooms.len(),
        s.doors.len(),
        s.openings.len(),
    );
    let _ = writeln!(
        out,
        "Walls: {}.",
        materials(s.walls.iter().filter(|w| matches!(w.kind, 0 | 2)))
    );
    let _ = writeln!(out, "Roof pieces: {}.", materials(s.roofs.iter()));
    let _ = writeln!(out, "Current guess: {}.", guess(purpose));
    let _ = writeln!(out);
    let _ = writeln!(out, "## Rooms");
    let _ = writeln!(out);
    for (i, r) in s.rooms.iter().enumerate() {
        let floor = r.floor.map_or("no floor".to_string(), |f| describe(&f));
        let kind = if r.roofed {
            "roofed"
        } else {
            "open to the sky"
        };
        let _ = write!(
            out,
            "- Room {i}: plane {}, {kind}, {} tiles, {floor} floor. Guess: {}.",
            r.plane,
            r.tiles.len(),
            purpose
                .rooms
                .get(i)
                .map_or("none".into(), |g| name(&g.what))
        );
        let inside: Vec<&Placed> = s.objects.iter().filter(|o| o.room == Some(i)).collect();
        let _ = writeln!(out, " Contains: {}.", contents(inside.iter().copied()));
    }
    let outside: Vec<&Placed> = s.objects.iter().filter(|o| o.room.is_none()).collect();
    if !outside.is_empty() {
        let _ = writeln!(
            out,
            "- Outside walls: {}.",
            contents(outside.iter().copied())
        );
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "## Neighbours within 24 tiles");
    let _ = writeln!(out);
    if neighbours.is_empty() {
        let _ = writeln!(out, "None.");
    }
    for (n, p, gap) in neighbours {
        let _ = writeln!(
            out,
            "- {}: {} × {} tiles, planes {:?}, {gap} tiles away, to the {}. Guess: {}.",
            structure::id(n),
            n.size.0,
            n.size.1,
            n.planes,
            direction(s, n),
            guess(p),
        );
    }
    out
}

/// A class or use by the name it has in the JSON.
fn name(what: &impl serde::Serialize) -> String {
    serde_json::to_value(what)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// A guess as words: the class, a shop's detail and the evidence.
fn guess(p: &Purpose) -> String {
    let what = name(&p.class.what);
    let detail = p
        .class
        .detail
        .as_ref()
        .map(|d| format!(" ({d})"))
        .unwrap_or_default();
    if p.class.evidence.is_empty() {
        format!("{what}{detail}")
    } else {
        format!("{what}{detail}, from {}", p.class.evidence.join("; "))
    }
}

/// Which way `b` lies from `a`, by their boxes' centres.
fn direction(a: &Structure, b: &Structure) -> &'static str {
    let centre = |s: &Structure| {
        (
            f64::from(s.origin.0) + f64::from(s.size.0) / 2.0,
            f64::from(s.origin.1) + f64::from(s.size.1) / 2.0,
        )
    };
    let ((ax, ay), (bx, by)) = (centre(a), centre(b));
    let angle = (by - ay).atan2(bx - ax).to_degrees();
    const NAMES: [&str; 8] = [
        "east",
        "north-east",
        "north",
        "north-west",
        "west",
        "south-west",
        "south",
        "south-east",
    ];
    NAMES[(((angle + 360.0 + 22.5) % 360.0) / 45.0) as usize % 8]
}

/// A material as words.
fn describe(d: &Dressing) -> String {
    match (d.material, d.colour) {
        (Material::Plain, Some(c)) => format!("plain #{c:06x}"),
        (m, _) => format!("{m:?}").to_lowercase(),
    }
}

/// The materials of some placements, commonest first, with counts.
fn materials<'a>(ps: impl Iterator<Item = &'a Placed>) -> String {
    let mut count: BTreeMap<String, usize> = BTreeMap::new();
    for p in ps {
        if let Some(m) = &p.material {
            *count.entry(describe(m)).or_default() += 1;
        }
    }
    if count.is_empty() {
        return "none".into();
    }
    let mut v: Vec<(String, usize)> = count.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.iter()
        .take(4)
        .map(|(m, n)| format!("{m} ×{n}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Named objects with their categories, and icons with what they show,
/// commonest first; unnamed ones only counted.
fn contents<'a>(objects: impl Iterator<Item = &'a Placed>) -> String {
    let mut named: BTreeMap<String, usize> = BTreeMap::new();
    let mut unnamed = 0;
    for o in objects {
        if let Some(i) = o.icon {
            let shows = furnish::icon(i).map_or("an unknown icon", |(s, _)| s);
            *named
                .entry(format!("minimap icon showing {shows}"))
                .or_default() += 1;
        } else if let Some(n) = &o.name {
            let cat = o
                .category
                .map(|c| format!(" [{c:?}]").to_lowercase())
                .unwrap_or_default();
            *named.entry(format!("{n}{cat}")).or_default() += 1;
        } else {
            unnamed += 1;
        }
    }
    let mut v: Vec<(String, usize)> = named.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut parts: Vec<String> = v
        .into_iter()
        .map(|(n, c)| if c > 1 { format!("{n} ×{c}") } else { n })
        .collect();
    if unnamed > 0 {
        parts.push(format!("{unnamed} unnamed decorations"));
    }
    if parts.is_empty() {
        "nothing".into()
    } else {
        parts.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_category_has_a_colour() {
        assert_eq!(CATEGORY_COLOURS.len(), 30);
        let mut seen = std::collections::BTreeSet::new();
        assert!(CATEGORY_COLOURS.iter().all(|(c, _)| seen.insert(*c)));
    }

    #[test]
    fn digits_draw_where_they_should() {
        let mut c = Canvas(RgbaImage::from_pixel(8, 5, rgba(0xFFFFFF)));
        c.number(1, 0, 0, 1, 0x000000);
        // The 1's stem runs down its middle column.
        assert!((0..5).all(|y| c.0.get_pixel(1, y)[0] == 0));
        assert_eq!(c.0.get_pixel(0, 2)[0], 255);
    }
}
