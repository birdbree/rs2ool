//! A sequence drawn as one image: a few frames, evenly spaced in time, in a
//! grid, with the classic renderer.
//!
//! Every tile shares one camera fitted to all the frames shown, so a model
//! that falls, shrinks or moves away does so on screen. The darker band at
//! the bottom of each tile is the ground where the model starts.

use std::io::Cursor;

use image::{ImageFormat, RgbaImage};
use rs2cache::Records;
use rs2cache::types::record::{Record, RecordType};
use rs2classic::tables::{cos, sin};
use rs2classic::{Camera, Framebuffer, Textures, light, render};
use rs2convert::creature::Creature;

/// Width and height of one frame's tile, in pixels.
pub const TILE: usize = 256;
/// Turn of the camera about the model, 2048 per turn: three-quarters on.
const YAW: i32 = 256;
/// Tilt of the camera, looking slightly down.
const PITCH: i32 = 48;
/// Space left between the model and the tile's edge, in pixels.
const MARGIN: i32 = 16;
const BACKGROUND: u32 = 0x8a9ba8;
const GROUND: u32 = 0x5c6b58;
const BORDER: u32 = 0x202020;

/// The frames to show out of a sequence with these `delays`: `n` of them
/// evenly spaced in time from the first frame to the last, both included,
/// with repeats dropped.
pub fn sample(delays: &[u32], n: usize) -> Vec<usize> {
    let (len, n) = (delays.len(), n.max(1));
    if len == 0 {
        return Vec::new();
    }
    // When each frame starts; the last frame starts at `end`.
    let starts: Vec<u64> = delays
        .iter()
        .scan(0u64, |t, &d| {
            let s = *t;
            *t += u64::from(d);
            Some(s)
        })
        .collect();
    let end = starts[len - 1];
    let mut out: Vec<usize> = (0..n)
        .map(|k| {
            let t = if n == 1 {
                0
            } else {
                end * k as u64 / (n as u64 - 1)
            };
            starts.partition_point(|&s| s <= t) - 1
        })
        .collect();
    out.dedup();
    out
}

/// The 50 texture pages, for drawing textured faces.
pub fn textures(records: &Records) -> Textures {
    let sheets: Vec<_> = (0..50)
        .map(|t| match records.decode(RecordType::Texture, t) {
            Ok(Record::Texture(s)) => Some(s),
            _ => None,
        })
        .collect();
    Textures::from_sheets(&sheets, contentpack::palette::DEFAULT_BRIGHTNESS)
}

/// Sequence `s` of `c` (an index into its sequences) as a grid of up to
/// `n` frames, left to right and top to bottom. `None` when a frame won't
/// pose or the model is empty.
pub fn sheet(c: &Creature, s: usize, n: usize, textures: &Textures) -> Option<RgbaImage> {
    let poses: Vec<Vec<[i32; 3]>> = sample(&c.delays(s), n)
        .into_iter()
        .map(|i| c.posed(s, i))
        .collect::<Option<_>>()?;
    let first = poses.first().filter(|p| !p.is_empty())?;
    // One box around every pose shown, so every tile shares a camera.
    let (mut lo, mut hi) = ([i32::MAX; 3], [i32::MIN; 3]);
    for v in poses.iter().flatten() {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    let centre = [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2);
    let radius = [0, 1, 2]
        .map(|k| f64::from(hi[k] - lo[k]) / 2.0)
        .iter()
        .map(|h| h * h)
        .sum::<f64>()
        .sqrt()
        .max(1.0);
    // Fit the box's bounding sphere in the tile: x · 512 / z on screen.
    let fit = radius * 512.0 / f64::from(TILE as i32 / 2 - MARGIN);
    let distance = fit.max(radius + 64.0) as i32;
    let cam = Camera::orbit(YAW, PITCH, distance);
    let ground = ground_row(&cam, first.iter().map(|v| v[1]).max()? - centre[1]);
    let lit = light(&c.rest_model(), &rs2classic::light::NPC);

    let cols = (poses.len() as f64).sqrt().ceil() as usize;
    let rows = poses.len().div_ceil(cols);
    let border = image::Rgba([(BORDER >> 16) as u8, (BORDER >> 8) as u8, BORDER as u8, 255]);
    let mut img = RgbaImage::from_pixel((cols * TILE) as u32, (rows * TILE) as u32, border);
    for (j, pose) in poses.iter().enumerate() {
        let mut fb = Framebuffer::new(TILE, TILE);
        fb.clear(BACKGROUND);
        if let Some(g) = ground {
            fb.pixels[g * TILE..].fill(GROUND);
        }
        let moved: Vec<[i32; 3]> = pose
            .iter()
            .map(|v| [v[0] - centre[0], v[1] - centre[1], v[2] - centre[2]])
            .collect();
        render(&mut fb, &lit, &moved, &cam, textures);
        for i in 0..TILE {
            for p in [i, (TILE - 1) * TILE + i, i * TILE, i * TILE + TILE - 1] {
                fb.pixels[p] = BORDER;
            }
        }
        let (ox, oy) = ((j % cols) * TILE, (j / cols) * TILE);
        for (i, &p) in fb.pixels.iter().enumerate() {
            let (x, y) = ((ox + i % TILE) as u32, (oy + i / TILE) as u32);
            img.put_pixel(
                x,
                y,
                image::Rgba([(p >> 16) as u8, (p >> 8) as u8, p as u8, 255]),
            );
        }
    }
    Some(img)
}

/// The screen row of the ground at height `y` (cache space, y down) under
/// the model's centre, or `None` if it's off the tile.
fn ground_row(cam: &Camera, y: i32) -> Option<usize> {
    // The camera transform of `(0, y, 0)`: offset, then pitch about x.
    let (y, z) = (y + cam.y, cam.z);
    let (sp, cp) = (sin(cam.pitch), cos(cam.pitch));
    let py = (y.wrapping_mul(cp).wrapping_sub(z.wrapping_mul(sp))) >> 16;
    let pz = (y.wrapping_mul(sp).wrapping_add(z.wrapping_mul(cp))) >> 16;
    if pz <= 0 {
        return None;
    }
    let row = TILE as i32 / 2 + (py << 9) / pz;
    usize::try_from(row).ok().filter(|&r| r < TILE)
}

/// `img` as PNG bytes.
pub fn png(img: &RgbaImage) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png)
        .expect("PNG encoding into memory doesn't fail");
    out.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_span_the_sequence_in_time() {
        // Ten frames, the fifth held for a long time.
        let mut delays = vec![2; 10];
        delays[4] = 40;
        let picks = sample(&delays, 6);
        assert_eq!(picks.first(), Some(&0));
        assert_eq!(picks.last(), Some(&9));
        // The long frame takes up most of the time, so it's picked, once.
        assert_eq!(picks.iter().filter(|&&i| i == 4).count(), 1);
        assert!(picks.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(sample(&[3, 3], 6), vec![0, 1]);
        assert_eq!(sample(&[5], 6), vec![0]);
        assert_eq!(sample(&[1, 1, 1], 1), vec![0]);
        assert!(sample(&[], 6).is_empty());
    }

    #[test]
    fn the_ground_is_below_the_centre() {
        let cam = Camera::orbit(YAW, PITCH, 1000);
        let row = ground_row(&cam, 100).unwrap();
        assert!(row > TILE / 2);
        assert!(ground_row(&cam, -100).unwrap() < row);
    }
}
