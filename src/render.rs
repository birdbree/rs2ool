//! Drawing models with the classic renderer into images: single frames,
//! grids of frames, and animations as APNG or GIF. `rs2ool cache render`
//! and caption frame grids both draw through here.
//!
//! Every frame of one drawing shares one camera, fitted to all the poses
//! shown ([`frame`]), so a model that falls, shrinks or moves away does so
//! on screen. The camera is the original client's model-viewer camera
//! ([`Camera::orbit`]), turned about the model's centre.

use std::io::Cursor;

use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, ImageFormat, RgbaImage};
use rs2cache::Records;
use rs2cache::types::record::{Record, RecordType};
use rs2classic::tables::{cos, sin};
use rs2classic::{Camera, Framebuffer, LitModel, Textures, render};

/// The background of every drawing unless asked otherwise.
pub const BACKGROUND: u32 = 0x8a9ba8;
/// The band of ground drawn under an animated model.
pub const GROUND: u32 = 0x5c6b58;
/// The line around each tile of a grid.
pub const BORDER: u32 = 0x202020;
/// How long a client cycle is, in milliseconds: frame delays count them.
pub const CYCLE_MS: u32 = 20;

/// The image size and camera of a drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct View {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// Turn of the camera about the model, 2048 per turn.
    pub yaw: i32,
    /// Tilt of the camera, 2048 per turn; positive looks down.
    pub pitch: i32,
    /// Distance from the camera to the model's centre, or `None` to fit
    /// the model in the image.
    pub distance: Option<i32>,
    /// Background colour, `0xRRGGBB`.
    pub background: u32,
}

impl Default for View {
    /// 512 × 512, three-quarters on, looking slightly down, fitted.
    fn default() -> Self {
        Self {
            width: 512,
            height: 512,
            yaw: 256,
            pitch: 48,
            distance: None,
            background: BACKGROUND,
        }
    }
}

/// Where the camera sits for one drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Framing {
    /// The camera, about `centre`.
    pub cam: Camera,
    /// The point the camera turns about: the centre of the box around
    /// every pose.
    pub centre: [i32; 3],
    /// The screen row the ground starts at, if one is drawn and it's on
    /// screen.
    pub ground: Option<usize>,
}

/// One camera for every pose in `poses`, fitting them all in the view
/// unless the view gives a distance. With `ground`, the ground is drawn at
/// the lowest point of that pose (y is down). `None` when there are no
/// vertices.
pub fn frame(view: &View, poses: &[Vec<[i32; 3]>], ground: Option<&[[i32; 3]]>) -> Option<Framing> {
    let (mut lo, mut hi) = ([i32::MAX; 3], [i32::MIN; 3]);
    for v in poses.iter().flatten() {
        for k in 0..3 {
            lo[k] = lo[k].min(v[k]);
            hi[k] = hi[k].max(v[k]);
        }
    }
    if lo[0] > hi[0] {
        return None;
    }
    let centre = [0, 1, 2].map(|k| (lo[k] + hi[k]) / 2);
    let radius = [0, 1, 2]
        .map(|k| f64::from(hi[k] - lo[k]) / 2.0)
        .iter()
        .map(|h| h * h)
        .sum::<f64>()
        .sqrt()
        .max(1.0);
    // Fit the box's bounding sphere inside a margin: x · 512 / z on screen.
    let side = view.width.min(view.height) as i32;
    let fit = radius * 512.0 / f64::from((side / 2 - side / 16).max(1));
    let distance = view
        .distance
        .unwrap_or_else(|| fit.max(radius + 64.0) as i32);
    let cam = Camera::orbit(view.yaw, view.pitch, distance);
    let ground = ground
        .and_then(|g| g.iter().map(|v| v[1]).max())
        .and_then(|y| ground_row(&cam, y - centre[1], view.height));
    Some(Framing {
        cam,
        centre,
        ground,
    })
}

/// The screen row of the ground at height `y` (relative to the centre,
/// y down) under the model's centre, or `None` if it's off screen.
fn ground_row(cam: &Camera, y: i32, height: usize) -> Option<usize> {
    // The camera transform of `(0, y, 0)`: offset, then pitch about x.
    let (y, z) = (y + cam.y, cam.z);
    let (sp, cp) = (sin(cam.pitch), cos(cam.pitch));
    let py = (y.wrapping_mul(cp).wrapping_sub(z.wrapping_mul(sp))) >> 16;
    let pz = (y.wrapping_mul(sp).wrapping_add(z.wrapping_mul(cp))) >> 16;
    if pz <= 0 {
        return None;
    }
    let row = height as i32 / 2 + (py << 9) / pz;
    usize::try_from(row).ok().filter(|&r| r < height)
}

/// `lit` drawn with `pose` (its vertices, rest or posed) through
/// `framing`, over the background and ground.
pub fn draw(
    view: &View,
    framing: &Framing,
    lit: &LitModel,
    pose: &[[i32; 3]],
    textures: &Textures,
) -> Framebuffer {
    let mut fb = Framebuffer::new(view.width, view.height);
    fb.clear(view.background);
    if let Some(g) = framing.ground {
        fb.pixels[g * view.width..].fill(GROUND);
    }
    let c = framing.centre;
    let moved: Vec<[i32; 3]> = pose
        .iter()
        .map(|v| [v[0] - c[0], v[1] - c[1], v[2] - c[2]])
        .collect();
    render(&mut fb, lit, &moved, &framing.cam, textures);
    fb
}

/// Draw a one-pixel line in `rgb` around the edge of `fb`.
pub fn outline(fb: &mut Framebuffer, rgb: u32) {
    let (w, h) = (fb.width, fb.height);
    for x in 0..w {
        fb.pixels[x] = rgb;
        fb.pixels[(h - 1) * w + x] = rgb;
    }
    for y in 0..h {
        fb.pixels[y * w] = rgb;
        fb.pixels[y * w + w - 1] = rgb;
    }
}

/// `fb` as an opaque RGBA image.
pub fn image(fb: &Framebuffer) -> RgbaImage {
    RgbaImage::from_raw(fb.width as u32, fb.height as u32, fb.to_rgba())
        .expect("a framebuffer holds width × height pixels")
}

/// Same-sized `tiles` in a grid, left to right and top to bottom, `cols`
/// to a row, or with `None` as close to square as the count allows.
/// Spare cells are left in [`BORDER`].
pub fn grid(tiles: &[RgbaImage], cols: Option<usize>) -> RgbaImage {
    let Some(first) = tiles.first() else {
        return RgbaImage::new(0, 0);
    };
    let (w, h) = first.dimensions();
    let cols = cols
        .unwrap_or_else(|| (tiles.len() as f64).sqrt().ceil() as usize)
        .max(1) as u32;
    let rows = (tiles.len() as u32).div_ceil(cols);
    let mut img = RgbaImage::from_pixel(cols * w, rows * h, rgba(BORDER));
    for (j, t) in tiles.iter().enumerate() {
        let j = j as u32;
        image::imageops::replace(
            &mut img,
            t,
            i64::from(j % cols * w),
            i64::from(j / cols * h),
        );
    }
    img
}

fn rgba(rgb: u32) -> image::Rgba<u8> {
    image::Rgba([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8, 255])
}

/// `img` as PNG bytes.
pub fn png(img: &RgbaImage) -> Vec<u8> {
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, ImageFormat::Png)
        .expect("PNG encoding into memory doesn't fail");
    out.into_inner()
}

/// `frames` as an animated PNG that loops forever, frame `i` showing for
/// `delays[i]` client cycles. The frames must all be one size.
pub fn apng(frames: &[RgbaImage], delays: &[u32]) -> Result<Vec<u8>, png::EncodingError> {
    let (w, h) = frames.first().map_or((1, 1), |f| f.dimensions());
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_animated(frames.len().max(1) as u32, 0)?;
    let mut writer = enc.write_header()?;
    for (f, &d) in frames.iter().zip(delays) {
        // A cycle is 1/50 s.
        writer.set_frame_delay(d.min(u32::from(u16::MAX)) as u16, 1000 / CYCLE_MS as u16)?;
        writer.write_image_data(f.as_raw())?;
    }
    writer.finish()?;
    Ok(out)
}

/// `frames` as a GIF that loops forever, frame `i` showing for
/// `delays[i]` client cycles. Each frame is cut down to 256 colours.
pub fn gif(frames: &[RgbaImage], delays: &[u32]) -> Result<Vec<u8>, image::ImageError> {
    let mut out = Vec::new();
    {
        let mut enc = GifEncoder::new_with_speed(&mut out, 10);
        enc.set_repeat(Repeat::Infinite)?;
        enc.encode_frames(frames.iter().zip(delays).map(|(f, &d)| {
            image::Frame::from_parts(f.clone(), 0, 0, Delay::from_numer_denom_ms(d * CYCLE_MS, 1))
        }))?;
    }
    Ok(out)
}

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

/// The 50 texture pages, for drawing textured faces. Missing ones are
/// left empty.
pub fn textures(records: &Records) -> Textures {
    let sheets: Vec<_> = (0..50)
        .map(|t| match records.decode(RecordType::Texture, t) {
            Ok(Record::Texture(s)) => Some(s),
            _ => None,
        })
        .collect();
    Textures::from_sheets(&sheets, contentpack::palette::DEFAULT_BRIGHTNESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rs2cache::types::model::Model;

    /// A square pyramid, 100 across and 100 tall, in one colour.
    fn pyramid() -> Model {
        Model {
            vertices: vec![
                [-50, 0, -50],
                [50, 0, -50],
                [50, 0, 50],
                [-50, 0, 50],
                [0, -100, 0],
            ],
            faces: vec![
                [0, 1, 4],
                [1, 2, 4],
                [2, 3, 4],
                [3, 0, 4],
                [0, 2, 1],
                [0, 3, 2],
            ],
            face_colours: vec![0x2a7f; 6],
            face_types: None,
            priority: 0,
            face_priorities: None,
            face_alphas: None,
            face_labels: None,
            vertex_labels: None,
            textured_faces: vec![],
        }
    }

    fn drawn(view: &View, ground: bool) -> Framebuffer {
        let m = pyramid();
        let poses = [m.vertices.clone()];
        let f = frame(view, &poses, ground.then_some(&m.vertices[..])).unwrap();
        let lit = rs2classic::light(&m, &rs2classic::light::NPC);
        draw(view, &f, &lit, &m.vertices, &Textures::default())
    }

    #[test]
    fn a_synthetic_model_renders_to_a_png() {
        let view = View {
            width: 96,
            height: 64,
            ..View::default()
        };
        let fb = drawn(&view, false);
        let bytes = png(&image(&fb));
        let back = image::load_from_memory_with_format(&bytes, ImageFormat::Png)
            .unwrap()
            .to_rgba8();
        assert_eq!(back.dimensions(), (96, 64));
        assert_eq!(back.as_raw(), &fb.to_rgba());
        // The model sits in the middle, fitted inside the edges.
        assert_ne!(fb.pixels[32 * 96 + 48], BACKGROUND);
        let edge = (0..96).chain((0..64).map(|y| y * 96));
        assert!(edge.into_iter().all(|p| fb.pixels[p] == BACKGROUND));
        let covered = fb.pixels.iter().filter(|&&p| p != BACKGROUND).count();
        assert!(covered > 96 * 64 / 10, "{covered} pixels drawn");
    }

    #[test]
    fn a_given_distance_overrides_the_fit() {
        let near = drawn(&View::default(), false);
        let far = drawn(
            &View {
                distance: Some(20_000),
                ..View::default()
            },
            false,
        );
        let count = |fb: &Framebuffer| fb.pixels.iter().filter(|&&p| p != BACKGROUND).count();
        assert!(count(&far) * 10 < count(&near));
    }

    #[test]
    fn the_ground_is_below_the_centre() {
        let cam = Camera::orbit(256, 48, 1000);
        let row = ground_row(&cam, 100, 256).unwrap();
        assert!(row > 128);
        assert!(ground_row(&cam, -100, 256).unwrap() < row);
        let fb = drawn(&View::default(), true);
        assert_eq!(fb.pixels[511 * 512], GROUND);
        assert_eq!(fb.pixels[0], BACKGROUND);
    }

    #[test]
    fn animations_keep_every_frame() {
        let a = RgbaImage::from_pixel(8, 4, rgba(0xff0000));
        let b = RgbaImage::from_pixel(8, 4, rgba(0x0000ff));
        let frames = [a.clone(), b.clone(), a];
        let delays = [2, 5, 1];

        let bytes = apng(&frames, &delays).unwrap();
        let mut reader = png::Decoder::new(Cursor::new(&bytes)).read_info().unwrap();
        let actl = reader.info().animation_control.unwrap();
        assert_eq!((actl.num_frames, actl.num_plays), (3, 0));
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let mut seen = Vec::new();
        for _ in 0..3 {
            reader.next_frame(&mut buf).unwrap();
            let fc = reader.info().frame_control.unwrap();
            seen.push((buf[0], buf[2], fc.delay_num, fc.delay_den));
        }
        assert_eq!(seen, [(255, 0, 2, 50), (0, 255, 5, 50), (255, 0, 1, 50)]);

        use image::AnimationDecoder;
        let bytes = gif(&frames, &delays).unwrap();
        let decoded = image::codecs::gif::GifDecoder::new(Cursor::new(bytes))
            .unwrap()
            .into_frames()
            .collect_frames()
            .unwrap();
        let got: Vec<(u8, u8, u32)> = decoded
            .iter()
            .map(|f| {
                let p = f.buffer().get_pixel(0, 0);
                let (n, d) = f.delay().numer_denom_ms();
                (p[0], p[2], n / d)
            })
            .collect();
        assert_eq!(got, [(255, 0, 40), (0, 255, 100), (255, 0, 20)]);
    }

    #[test]
    fn grids_lay_tiles_out() {
        let t = |rgb| RgbaImage::from_pixel(4, 3, rgba(rgb));
        let g = grid(&[t(1), t(2), t(3)], None);
        assert_eq!(g.dimensions(), (8, 6));
        assert_eq!(g.get_pixel(4, 0), &rgba(2));
        assert_eq!(g.get_pixel(0, 3), &rgba(3));
        assert_eq!(g.get_pixel(4, 3), &rgba(BORDER));
        let row = grid(&[t(1), t(2), t(3)], Some(3));
        assert_eq!(row.dimensions(), (12, 3));
        assert_eq!(row.get_pixel(9, 1), &rgba(3));
    }

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
}
