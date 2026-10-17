//! Projection, face ordering and drawing, as the original client's
//! model-viewer path does it.
//!
//! [`render`] draws through the model-viewer [`Camera`]; [`render_in_view`]
//! draws a model placed in the world through a [`View`]. Both project the
//! vertices and hand over to [`draw_projected`], which culls back faces,
//! buckets faces by depth and draws them far to near:
//!
//! ```text
//! sx = cx + (x << 9) / z,  sy = cy + (y << 9) / z     z ≥ near
//! bucket = (depth_a + depth_b + depth_c) / 3 + extent_near
//! ```

use crate::light::{LitModel, kind};
use crate::raster::{self, State, TexSpace};
use crate::tables::{cos, hsl, sin};
use crate::texture::Textures;

/// A `0xRRGGBB` pixel buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Framebuffer {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// `width × height` pixels, row by row from the top.
    pub pixels: Vec<u32>,
}

impl Framebuffer {
    /// A black buffer.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            pixels: vec![0; width * height],
        }
    }

    /// Fill the whole buffer with one colour.
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

/// The original client's model-viewer camera: the model is rolled about z,
/// turned by `yaw` about y, moved by `(x, y, z)` (z is the distance), then
/// pitched about x. Angles are 2048 units per turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Camera {
    /// Turn about y, 2048 per turn.
    pub yaw: i32,
    /// Tilt about x, applied after the offset, 2048 per turn.
    pub pitch: i32,
    /// Roll about z, applied first, 2048 per turn.
    pub roll: i32,
    /// Sideways offset.
    pub x: i32,
    /// Vertical offset (y down).
    pub y: i32,
    /// Distance along the view axis, before pitch.
    pub z: i32,
}

impl Camera {
    /// An orbit camera around a model centred on the origin: the model sits
    /// at `distance` on the view axis whatever the pitch. (Pitch turns
    /// about the camera, not the model, so the offset has to tilt with it.)
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

/// The model's extent for the depth buckets, `(near, range)`, worked out
/// from its vertices as the original client does. `near` is the bucket
/// offset for depth 0 and `range` the bucket count. It's not the near
/// plane.
fn extent(vertices: &[[i32; 3]]) -> (i32, i32) {
    // up/down: furthest any vertex goes above/below y = 0 (y down, so up
    // is −y). xz: largest squared horizontal distance from the y axis.
    let (mut up, mut down, mut xz) = (0i32, 0i32, 0i64);
    for v in vertices {
        up = up.max(-v[1]);
        down = down.max(v[1]);
        xz = xz.max(i64::from(v[0]) * i64::from(v[0]) + i64::from(v[2]) * i64::from(v[2]));
    }
    // r: horizontal radius, rounded up. near and range-near are the
    // distances to the top and bottom rims of the bounding cylinder.
    let r = ((xz as f64).sqrt() + 0.99) as i64;
    let near = ((r as f64 * r as f64 + f64::from(up) * f64::from(up)).sqrt() + 0.99) as i32;
    let range =
        near + ((r as f64 * r as f64 + f64::from(down) * f64::from(down)).sqrt() + 0.99) as i32;
    (near, range)
}

/// A model's vertices projected to the screen.
pub struct Projected {
    /// Screen x per vertex (only meaningful where `ok`).
    pub sx: Vec<i32>,
    /// Screen y per vertex (only meaningful where `ok`).
    pub sy: Vec<i32>,
    /// Depth relative to the model's origin, for the depth buckets.
    pub depth: Vec<i32>,
    /// Camera-space positions (for texture mapping).
    pub camv: Vec<[i32; 3]>,
    /// Whether the vertex is at or past the near plane and got projected.
    pub ok: Vec<bool>,
}

impl Projected {
    fn with_capacity(n: usize) -> Self {
        Self {
            sx: vec![0; n],
            sy: vec![0; n],
            depth: vec![0; n],
            camv: vec![[0; 3]; n],
            ok: vec![true; n],
        }
    }

    /// Record vertex `i` at camera-space `c`, projecting it if it's at
    /// least `near` in front.
    fn set(&mut self, i: usize, c: [i32; 3], origin_depth: i32, centre: (i32, i32), near: i32) {
        self.depth[i] = c[2] - origin_depth;
        self.camv[i] = c;
        if c[2] >= near {
            // `<< 9` is the focal length of 512. Wraps like a Java int.
            self.sx[i] = centre.0.wrapping_add(c[0].wrapping_shl(9) / c[2]);
            self.sy[i] = centre.1.wrapping_add(c[1].wrapping_shl(9) / c[2]);
        } else {
            self.ok[i] = false;
        }
    }
}

/// Draw `model` with `vertices` (its rest pose or an animated pose) into
/// `fb` with the model-viewer camera. Faces that cross behind the camera
/// are skipped. (The original client's viewer never puts a model there,
/// so it never has to clip.)
pub fn render(
    fb: &mut Framebuffer,
    model: &LitModel,
    vertices: &[[i32; 3]],
    cam: &Camera,
    textures: &Textures,
) {
    let centre = (fb.width as i32 / 2, fb.height as i32 / 2);
    // 16.16 sine/cosine pairs: yaw, roll, pitch.
    let (sy, cyw) = (sin(cam.yaw), cos(cam.yaw));
    let (sr, cr) = (sin(cam.roll), cos(cam.roll));
    let (sp, cp) = (sin(cam.pitch), cos(cam.pitch));
    // Camera-space z of the model's origin: the pitched offset. Vertex
    // depths are taken relative to it.
    let origin_depth = (cam.y.wrapping_mul(sp).wrapping_add(cam.z.wrapping_mul(cp))) >> 16;
    let mut p = Projected::with_capacity(vertices.len());
    for (i, v) in vertices.iter().enumerate() {
        let [mut x, mut y, mut z] = *v;
        // Roll about z, then yaw about y (each skipped at angle 0, as the
        // original client skips them), then offset, then pitch about x.
        // Every `>> 16` drops the 16.16 fraction.
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
        p.set(i, [x, y, z], origin_depth, centre, 1);
    }
    let (near, range) = extent(vertices);
    draw_projected(fb, model, &p, near, range, textures);
}

/// A world camera, as the original client's scene renderer uses: the eye
/// position (cache coordinates, y down), yaw about y and pitch about x.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct View {
    /// Eye position in world cache coordinates (y down).
    pub eye: [i32; 3],
    /// Turn about y, 2048 per turn.
    pub yaw: i32,
    /// Tilt about x, 2048 per turn.
    pub pitch: i32,
}

/// The world view's near plane: vertices closer than this in camera-space
/// z count as behind the camera. It's where the original client clips scene
/// geometry.
pub const NEAR: i32 = 50;

impl View {
    /// Camera-space position of a world point: relative to the eye, turned
    /// by yaw, then by pitch, in the same 16.16 maths as [`render`].
    pub fn to_camera(&self, v: [i32; 3]) -> [i32; 3] {
        let (sy, cy) = (sin(self.yaw), cos(self.yaw));
        let (sp, cp) = (sin(self.pitch), cos(self.pitch));
        let (x, y, z) = (v[0] - self.eye[0], v[1] - self.eye[1], v[2] - self.eye[2]);
        let (x, z) = (
            (z.wrapping_mul(sy).wrapping_add(x.wrapping_mul(cy))) >> 16,
            (z.wrapping_mul(cy).wrapping_sub(x.wrapping_mul(sy))) >> 16,
        );
        let (y, z) = (
            (y.wrapping_mul(cp).wrapping_sub(z.wrapping_mul(sp))) >> 16,
            (y.wrapping_mul(sp).wrapping_add(z.wrapping_mul(cp))) >> 16,
        );
        [x, y, z]
    }
}

/// Draw a model placed in the world (`vertices` in world space) through a
/// world camera.
///
/// Returns `false`, having drawn nothing, when no vertex is in front of the
/// near plane ([`NEAR`]) or there are no vertices. Faces with any vertex
/// behind it are skipped.
pub fn render_in_view(
    fb: &mut Framebuffer,
    model: &LitModel,
    vertices: &[[i32; 3]],
    view: &View,
    textures: &Textures,
) -> bool {
    if vertices.is_empty() {
        return false;
    }
    let centre = (fb.width as i32 / 2, fb.height as i32 / 2);
    let n = vertices.len() as i64;
    let sum = vertices.iter().fold([0i64; 3], |a, v| {
        [
            a[0] + i64::from(v[0]),
            a[1] + i64::from(v[1]),
            a[2] + i64::from(v[2]),
        ]
    });
    // The vertices' integer mean stands in for the model's origin, both
    // for the depth reference and for the extent below.
    let mid = sum.map(|c| (c / n) as i32);
    let origin_depth = view.to_camera(mid)[2];
    let mut p = Projected::with_capacity(vertices.len());
    for (i, v) in vertices.iter().enumerate() {
        p.set(i, view.to_camera(*v), origin_depth, centre, NEAR);
    }
    if !p.ok.iter().any(|&o| o) {
        return false;
    }
    let local: Vec<[i32; 3]> = vertices
        .iter()
        .map(|v| [v[0] - mid[0], v[1] - mid[1], v[2] - mid[2]])
        .collect();
    let (near, range) = extent(&local);
    draw_projected(fb, model, &p, near, range, textures);
    true
}

/// Sort a projected model's visible faces and draw them.
///
/// `near` and `range` are the model's extent: the bucket offset for depth
/// 0 and the bucket count. Faces with an unprojected vertex, back faces,
/// zero-area faces and faces whose bucket falls outside the range are
/// dropped.
pub fn draw_projected(
    fb: &mut Framebuffer,
    model: &LitModel,
    p: &Projected,
    near: i32,
    range: i32,
    textures: &Textures,
) {
    let (cx, cy) = (fb.width as i32 / 2, fb.height as i32 / 2);
    let Projected {
        sx,
        sy: syv,
        depth,
        camv,
        ok,
    } = p;
    // Visible faces into depth buckets; index = depth + near, so higher is
    // farther. clip[f]: face f has a corner off the buffer horizontally.
    let mut buckets: Vec<Vec<usize>> = vec![Vec::new(); range.max(1) as usize];
    let mut clip = vec![false; model.faces.len()];
    for (f, face) in model.faces.iter().enumerate() {
        let [a, b, c] = face.map(usize::from);
        if !(ok[a] && ok[b] && ok[c]) {
            continue;
        }
        let (x0, x1, x2) = (sx[a], sx[b], sx[c]);
        // Screen-space cross product; > 0 means it faces us. It wraps like
        // a Java int, as the original does, for vertices just past the
        // near plane whose screen coordinates are huge.
        let cross = (x0.wrapping_sub(x1))
            .wrapping_mul(syv[c].wrapping_sub(syv[b]))
            .wrapping_sub((syv[a].wrapping_sub(syv[b])).wrapping_mul(x2.wrapping_sub(x1)));
        if cross <= 0 {
            continue;
        }
        let w = fb.width as i32 - 1;
        clip[f] = [x0, x1, x2].iter().any(|&x| x < 0 || x > w);
        // Average corner depth, shifted so the nearest possible face is 0.
        let key = (depth[a].wrapping_add(depth[b]).wrapping_add(depth[c])) / 3 + near;
        if let Some(bucket) = usize::try_from(key).ok().and_then(|k| buckets.get_mut(k)) {
            bucket.push(f);
        }
    }

    let draw = |f: usize, fb: &mut Framebuffer| {
        let [a, b, c] = model.faces[f].map(usize::from);
        let ys = [syv[a], syv[b], syv[c]];
        let xs = [sx[a], sx[b], sx[c]];
        // Lit colours, shades or RGB, depending on the face kind.
        let col = model.colours[f];
        let st = State {
            clip: clip[f],
            alpha: i32::from(model.alphas[f]),
        };
        match model.kinds[f] {
            0 => raster::gouraud(fb, st, ys, xs, col),
            1 => raster::flat(fb, st, ys, xs, hsl(col[0])),
            kind::FLAT_RGB => raster::flat(fb, st, ys, xs, col[0] as u32),
            // Textured: 2 smooth, 3 flat.
            kind => {
                // A texture triangle index out of range: not drawn at all.
                let Some(&[p, m, nn]) = model.textured_faces.get(model.texture_triple[f]) else {
                    return;
                };
                let Some(page) = textures.page(model.base[f]) else {
                    // No texture loaded: flat mid grey stands in.
                    raster::flat(fb, st, ys, xs, 0x808080);
                    return;
                };
                // A flat textured face has one shade; use it at every corner.
                let shades = if kind == 3 { [col[0]; 3] } else { col };
                let tex = TexSpace {
                    p: camv[usize::from(p)],
                    m: camv[usize::from(m)],
                    n: camv[usize::from(nn)],
                    centre: (cx, cy),
                };
                // The textured filler ignores transparency.
                let st = State { alpha: 0, ..st };
                raster::textured(fb, st, ys, xs, shades, tex, page);
            }
        }
    };

    // No priorities: just buckets, far to near.
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
    // average depth of groups 1–2, 3–4 and 6/8 respectively. Yes, 6 and 8,
    // not 6 and 7.
    // groups[p]: faces of priority p, far to near. sums[p]: their bucket
    // indices added up (groups 0–9). special_depth[0/1]: bucket of each
    // face in groups 10/11.
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
    // Mean bucket over groups a and b together, 0 when both are empty.
    let avg = |a: usize, b: usize| {
        let n = (groups[a].len() + groups[b].len()) as i64;
        if n > 0 {
            ((sums[a] + sums[b]) / n) as i32
        } else {
            0
        }
    };
    let (d12, d34, d68) = (avg(1, 2), avg(3, 4), avg(6, 8));
    // Groups 10 then 11 form one queue. (list, idx) is the next queued
    // face; next_depth is its bucket, or −1000 once the queue is empty.
    let (mut list, mut idx) = (if groups[10].is_empty() { 11 } else { 10 }, 0usize);
    let depth_of =
        |list: usize, idx: usize| special_depth[list - 10].get(idx).copied().unwrap_or(-1000);
    let mut next_depth = depth_of(list, idx);
    // Draw the next queued face and advance, moving on to group 11 when
    // group 10 runs out.
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
    // Whatever's left in the queue goes last.
    while next_depth != -1000 {
        draw_special(fb, &mut list, &mut idx, &mut next_depth);
    }
}
