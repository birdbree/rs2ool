//! Projection, face ordering and drawing, as the client's model-viewer
//! path does it.

use crate::light::LitModel;
use crate::raster::{self, State, TexSpace};
use crate::tables::{cos, hsl, sin};
use crate::texture::Textures;

/// A 0xRRGGBB pixel buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Framebuffer {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
}

impl Framebuffer {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height],
        }
    }

    pub fn clear(&mut self, rgb: u32) {
        self.pixels.fill(rgb);
    }

    /// RGBA8 bytes, opaque.
    pub fn to_rgba(&self) -> Vec<u8> {
        self.pixels
            .iter()
            .flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8, 255])
            .collect()
    }
}

/// The client's model-viewer camera: the model is rolled about z, turned by
/// `yaw` about y, moved by `(x, y, z)` (z is the distance), then pitched
/// about x. Angles are 2048 units per turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Camera {
    pub yaw: i32,
    pub pitch: i32,
    pub roll: i32,
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl Camera {
    /// An orbit camera around a model centred on the origin: the model sits
    /// at `distance` on the view axis whatever the pitch. (The client's
    /// pitch turns about the camera, so the offset must follow it.)
    pub fn orbit(yaw: i32, pitch: i32, distance: i32) -> Self {
        Self {
            yaw,
            pitch,
            roll: 0,
            x: 0,
            y: distance.wrapping_mul(sin(pitch)) >> 16,
            z: distance.wrapping_mul(cos(pitch)) >> 16,
        }
    }
}

/// Depth buckets span the model's extent: `(radius, depth range)` as
/// `Model.method466` computes them from the vertices.
fn extent(vertices: &[[i32; 3]]) -> (i32, i32) {
    let (mut up, mut down, mut xz) = (0i32, 0i32, 0i64);
    for v in vertices {
        up = up.max(-v[1]);
        down = down.max(v[1]);
        xz = xz.max(i64::from(v[0]) * i64::from(v[0]) + i64::from(v[2]) * i64::from(v[2]));
    }
    let r = ((xz as f64).sqrt() + 0.99) as i64;
    let near = ((r as f64 * r as f64 + f64::from(up) * f64::from(up)).sqrt() + 0.99) as i32;
    let range =
        near + ((r as f64 * r as f64 + f64::from(down) * f64::from(down)).sqrt() + 0.99) as i32;
    (near, range)
}

/// Draw `model` with `vertices` (its rest pose or an animated pose) into
/// `fb`. Faces that cross behind the camera are skipped (the client's
/// viewer path never places a model there).
pub fn render(
    fb: &mut Framebuffer,
    model: &LitModel,
    vertices: &[[i32; 3]],
    cam: &Camera,
    textures: &Textures,
) {
    let (cx, cy) = (fb.width as i32 / 2, fb.height as i32 / 2);
    let (sy, cyw) = (sin(cam.yaw), cos(cam.yaw));
    let (sr, cr) = (sin(cam.roll), cos(cam.roll));
    let (sp, cp) = (sin(cam.pitch), cos(cam.pitch));
    let origin_depth = (cam.y.wrapping_mul(sp).wrapping_add(cam.z.wrapping_mul(cp))) >> 16;
    let n = vertices.len();
    let (mut sx, mut syv, mut depth) = (vec![0i32; n], vec![0i32; n], vec![0i32; n]);
    let mut camv = vec![[0i32; 3]; n];
    let mut ok = vec![true; n];
    for (i, v) in vertices.iter().enumerate() {
        let [mut x, mut y, mut z] = *v;
        if cam.roll != 0 {
            let t = (y.wrapping_mul(sr).wrapping_add(x.wrapping_mul(cr))) >> 16;
            y = (y.wrapping_mul(cr).wrapping_sub(x.wrapping_mul(sr))) >> 16;
            x = t;
        }
        if cam.yaw != 0 {
            let t = (z.wrapping_mul(sy).wrapping_add(x.wrapping_mul(cyw))) >> 16;
            z = (z.wrapping_mul(cyw).wrapping_sub(x.wrapping_mul(sy))) >> 16;
            x = t;
        }
        x += cam.x;
        y += cam.y;
        z += cam.z;
        let t = (y.wrapping_mul(cp).wrapping_sub(z.wrapping_mul(sp))) >> 16;
        z = (y.wrapping_mul(sp).wrapping_add(z.wrapping_mul(cp))) >> 16;
        y = t;
        depth[i] = z - origin_depth;
        camv[i] = [x, y, z];
        if z > 0 {
            sx[i] = cx + x.wrapping_shl(9) / z;
            syv[i] = cy + y.wrapping_shl(9) / z;
        } else {
            ok[i] = false;
        }
    }

    // Visible faces into depth buckets.
    let (near, range) = extent(vertices);
    let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); range.max(1) as usize];
    let mut clip = vec![false; model.faces.len()];
    for (f, face) in model.faces.iter().enumerate() {
        let [a, b, c] = face.map(usize::from);
        if !(ok[a] && ok[b] && ok[c]) {
            continue;
        }
        let (x0, x1, x2) = (sx[a], sx[b], sx[c]);
        if (x0 - x1) * (syv[c] - syv[b]) - (syv[a] - syv[b]) * (x2 - x1) <= 0 {
            continue;
        }
        let w = fb.width as i32 - 1;
        clip[f] = [x0, x1, x2].iter().any(|&x| x < 0 || x > w);
        let key = (depth[a] + depth[b] + depth[c]) / 3 + near;
        if let Some(bucket) = usize::try_from(key).ok().and_then(|k| buckets.get_mut(k)) {
            bucket.push(f);
        }
    }

    let draw = |f: usize, fb: &mut Framebuffer| {
        let [a, b, c] = model.faces[f].map(usize::from);
        let ys = [syv[a], syv[b], syv[c]];
        let xs = [sx[a], sx[b], sx[c]];
        let col = model.colours[f];
        let st = State {
            clip: clip[f],
            alpha: i32::from(model.alphas[f]),
        };
        match model.kinds[f] {
            0 => raster::gouraud(fb, st, ys, xs, col),
            1 => raster::flat(fb, st, ys, xs, hsl(col[0])),
            kind => {
                let Some(&[p, m, nn]) = model.textured_faces.get(model.texture_triple[f]) else {
                    return;
                };
                let Some(page) = textures.page(model.base[f]) else {
                    // No texture loaded: draw it as its shade in grey.
                    raster::flat(fb, st, ys, xs, 0x808080);
                    return;
                };
                let shades = if kind == 3 { [col[0]; 3] } else { col };
                let tex = TexSpace {
                    p: camv[usize::from(p)],
                    m: camv[usize::from(m)],
                    n: camv[usize::from(nn)],
                    centre: (cx, cy),
                };
                let st = State { alpha: 0, ..st };
                raster::textured(fb, st, ys, xs, shades, tex, page);
            }
        }
    };

    let Some(priorities) = &model.priorities else {
        for bucket in buckets.iter().rev() {
            for &f in bucket {
                draw(f, fb);
            }
        }
        return;
    };

    // Priority groups 0–9 draw in order; groups 10 and 11 (in depth order)
    // interleave before groups 0, 3 and 5 while they are deeper than the
    // average depth of groups 1–2, 3–4 and 6/8 respectively.
    let mut groups: Vec<Vec<usize>> = vec![Vec::new(); 12];
    let mut sums = [0i64; 12];
    let mut special_depth: [Vec<i32>; 2] = [Vec::new(), Vec::new()];
    for (d, bucket) in buckets.iter().enumerate().rev() {
        for &f in bucket {
            let p = usize::from(priorities[f]);
            if p >= 12 {
                continue;
            }
            groups[p].push(f);
            if p < 10 {
                sums[p] += d as i64;
            } else {
                special_depth[p - 10].push(d as i32);
            }
        }
    }
    let avg = |a: usize, b: usize| {
        let n = (groups[a].len() + groups[b].len()) as i64;
        if n > 0 {
            ((sums[a] + sums[b]) / n) as i32
        } else {
            0
        }
    };
    let (d12, d34, d68) = (avg(1, 2), avg(3, 4), avg(6, 8));
    let (mut list, mut idx) = (if groups[10].is_empty() { 11 } else { 10 }, 0usize);
    let depth_of =
        |list: usize, idx: usize| special_depth[list - 10].get(idx).copied().unwrap_or(-1000);
    let mut next_depth = depth_of(list, idx);
    let draw_special =
        |fb: &mut Framebuffer, list: &mut usize, idx: &mut usize, next_depth: &mut i32| {
            draw(groups[*list][*idx], fb);
            *idx += 1;
            if *idx == groups[*list].len() && *list == 10 {
                *list = 11;
                *idx = 0;
            }
            *next_depth = depth_of(*list, *idx);
        };
    for (g, group) in groups.iter().enumerate().take(10) {
        let threshold = match g {
            0 => Some(d12),
            3 => Some(d34),
            5 => Some(d68),
            _ => None,
        };
        if let Some(t) = threshold {
            while next_depth > t {
                draw_special(fb, &mut list, &mut idx, &mut next_depth);
            }
        }
        for &f in group {
            draw(f, fb);
        }
    }
    while next_depth != -1000 {
        draw_special(fb, &mut list, &mut idx, &mut next_depth);
    }
}
