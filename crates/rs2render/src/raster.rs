//! The three triangle fillers. Each walks the triangle's rows as the client
//! does — edge slopes in 16.16 fixed point from the topmost vertex, rows
//! clipped to the buffer, the long edge left or right by slope — and fills
//! each row with the client's span routine.

use crate::render::Framebuffer;
use crate::tables::{hsl, recip};
use crate::texture::TexturePage;

/// Per-face state the span routines read.
#[derive(Debug, Clone, Copy)]
pub struct State {
    /// Whether a vertex lies outside the buffer horizontally, so spans must
    /// clamp.
    pub clip: bool,
    /// 317 transparency (0 opaque, 255 invisible).
    pub alpha: i32,
}

/// Blend `src` over `dst` as the client does: `src·(256−a) + dst·a`.
pub fn blend(src: u32, dst: u32, alpha: i32) -> u32 {
    let (a, inv) = (alpha as u32, (256 - alpha) as u32);
    let s = ((((src & 0xff00ff) * inv) >> 8) & 0xff00ff) + ((((src & 0xff00) * inv) >> 8) & 0xff00);
    s + ((((dst & 0xff00ff) * a) >> 8) & 0xff00ff) + ((((dst & 0xff00) * a) >> 8) & 0xff00)
}

/// One edge being walked: x in 16.16 and an attribute in fixed point.
#[derive(Debug, Clone, Copy)]
struct Edge {
    x: i32,
    dx: i32,
    a: i32,
    da: i32,
}

impl Edge {
    fn step(&mut self) {
        self.x = self.x.wrapping_add(self.dx);
        self.a = self.a.wrapping_add(self.da);
    }
}

/// Walk a triangle's rows. `attr` values are shifted left by `shift` into
/// fixed point; `span` gets `(y, left x, right x, left attr, right attr)`
/// with x already integral and attrs still fixed point.
fn walk(
    height: i32,
    y: [i32; 3],
    x: [i32; 3],
    attr: [i32; 3],
    shift: u32,
    mut span: impl FnMut(i32, i32, i32, i32, i32),
) {
    let slope = |i: usize, j: usize| -> (i32, i32) {
        if y[j] == y[i] {
            (0, 0)
        } else {
            let dy = y[j] - y[i];
            (
                (x[j] - x[i]).wrapping_shl(16) / dy,
                (attr[j] - attr[i]).wrapping_shl(shift) / dy,
            )
        }
    };
    let t = if y[0] <= y[1] && y[0] <= y[2] {
        0
    } else if y[1] <= y[2] {
        1
    } else {
        2
    };
    let (n, p) = ((t + 1) % 3, (t + 2) % 3);
    let (s_tn, s_pt, s_np) = (slope(t, n), slope(p, t), slope(n, p));
    let mut yt = y[t];
    if yt >= height {
        return;
    }
    let (mut yn, mut yp) = (y[n].min(height), y[p].min(height));
    let start = |v: usize, s: (i32, i32)| Edge {
        x: x[v].wrapping_shl(16),
        dx: s.0,
        a: attr[v].wrapping_shl(shift),
        da: s.1,
    };
    let clip_to_top = |e: &mut Edge, from: i32| {
        e.x = e.x.wrapping_sub(e.dx.wrapping_mul(from));
        e.a = e.a.wrapping_sub(e.da.wrapping_mul(from));
    };
    let mut prev = start(t, s_pt);
    let mut next = start(t, s_tn);
    if yt < 0 {
        clip_to_top(&mut prev, yt);
        clip_to_top(&mut next, yt);
        yt = 0;
    }
    let mut row = |e1: &Edge, e2: &Edge, yy: i32| span(yy, e1.x >> 16, e2.x >> 16, e1.a, e2.a);
    if y[n] < y[p] {
        // The next edge ends first; the third edge runs from n to p.
        let mut third = start(n, s_np);
        if yn < 0 {
            clip_to_top(&mut third, yn);
            yn = 0;
        }
        let prev_left = (yt != yn && s_pt.0 < s_tn.0) || (yt == yn && s_pt.0 > s_np.0);
        let mut yy = yt;
        for _ in 0..(yn - yt).max(0) {
            if prev_left {
                row(&prev, &next, yy)
            } else {
                row(&next, &prev, yy)
            }
            prev.step();
            next.step();
            yy += 1;
        }
        for _ in 0..(yp - yn).max(0) {
            if prev_left {
                row(&prev, &third, yy)
            } else {
                row(&third, &prev, yy)
            }
            prev.step();
            third.step();
            yy += 1;
        }
    } else {
        // The previous edge ends first; the third edge runs from p to n.
        let mut third = start(p, s_np);
        if yp < 0 {
            clip_to_top(&mut third, yp);
            yp = 0;
        }
        let prev_left = (yt != yp && s_pt.0 < s_tn.0) || (yt == yp && s_np.0 > s_tn.0);
        let mut yy = yt;
        for _ in 0..(yp - yt).max(0) {
            if prev_left {
                row(&prev, &next, yy)
            } else {
                row(&next, &prev, yy)
            }
            prev.step();
            next.step();
            yy += 1;
        }
        for _ in 0..(yn - yp).max(0) {
            if prev_left {
                row(&third, &next, yy)
            } else {
                row(&next, &third, yy)
            }
            third.step();
            next.step();
            yy += 1;
        }
    }
}

/// A smooth-shaded triangle. `colours` are lit 16-bit HSL values,
/// interpolated and looked up per 4-pixel run.
pub fn gouraud(fb: &mut Framebuffer, st: State, y: [i32; 3], x: [i32; 3], colours: [i32; 3]) {
    let (w, h) = (fb.width as i32, fb.height as i32);
    walk(h, y, x, colours, 15, |yy, mut l, mut r, ca, cb| {
        let (mut c, end) = (ca >> 7, cb >> 7);
        let row = (yy * w) as usize;
        let step;
        if st.clip {
            let s = if r - l > 3 { (end - c) / (r - l) } else { 0 };
            if r > w - 1 {
                r = w - 1;
            }
            if l < 0 {
                c = c.wrapping_sub(l.wrapping_mul(s));
                l = 0;
            }
            if l >= r {
                return;
            }
            step = s << 2;
        } else {
            if l >= r {
                return;
            }
            let runs = (r - l) >> 2;
            step = if runs > 0 {
                (end - c).wrapping_mul(recip(runs)) >> 15
            } else {
                0
            };
        }
        let px = &mut fb.pixels[row..row + w as usize];
        let mut i = l as isize;
        let put = |px: &mut [u32], i: isize, colour: u32| {
            if let Some(d) = usize::try_from(i).ok().and_then(|i| px.get_mut(i)) {
                *d = if st.alpha == 0 {
                    colour
                } else {
                    blend(colour, *d, st.alpha)
                };
            }
        };
        for _ in 0..((r - l) >> 2) {
            let colour = hsl(c >> 8);
            c = c.wrapping_add(step);
            for _ in 0..4 {
                put(px, i, colour);
                i += 1;
            }
        }
        let rem = (r - l) & 3;
        if rem > 0 {
            let colour = hsl(c >> 8);
            for _ in 0..rem {
                put(px, i, colour);
                i += 1;
            }
        }
    });
}

/// A flat triangle in one RGB colour.
pub fn flat(fb: &mut Framebuffer, st: State, y: [i32; 3], x: [i32; 3], rgb: u32) {
    let (w, h) = (fb.width as i32, fb.height as i32);
    walk(h, y, x, [0; 3], 0, |yy, mut l, mut r, _, _| {
        if st.clip {
            r = r.min(w - 1);
            l = l.max(0);
        }
        if l >= r {
            return;
        }
        let row = (yy * w) as usize;
        // The client only leaves clipping off when every vertex is on
        // screen; clamp anyway so bad input cannot index out of the row.
        let (l, r) = (l.max(0) as usize, r.clamp(0, w) as usize);
        if l >= r {
            return;
        }
        for d in &mut fb.pixels[row + l..row + r] {
            *d = if st.alpha == 0 {
                rgb
            } else {
                blend(rgb, *d, st.alpha)
            };
        }
    });
}

/// The camera-space texture triangle `P, M, N` of a textured face.
#[derive(Debug, Clone, Copy)]
pub struct TexSpace {
    pub p: [i32; 3],
    pub m: [i32; 3],
    pub n: [i32; 3],
    /// The projection centre (half the buffer size).
    pub centre: (i32, i32),
}

/// A perspective-textured triangle with per-corner shades (0–127).
pub fn textured(
    fb: &mut Framebuffer,
    st: State,
    y: [i32; 3],
    x: [i32; 3],
    shades: [i32; 3],
    tex: TexSpace,
    page: &TexturePage,
) {
    let (w, h) = (fb.width as i32, fb.height as i32);
    let [px, py, pz] = tex.p;
    let (mx, my, mz) = (
        px.wrapping_sub(tex.m[0]),
        py.wrapping_sub(tex.m[1]),
        pz.wrapping_sub(tex.m[2]),
    );
    let (nx, ny, nz) = (
        tex.n[0].wrapping_sub(px),
        tex.n[1].wrapping_sub(py),
        tex.n[2].wrapping_sub(pz),
    );
    let m = |a: i32, b: i32| a.wrapping_mul(b);
    let u0 = m(nx, py).wrapping_sub(m(ny, px)).wrapping_shl(14);
    let u_dx = m(ny, pz).wrapping_sub(m(nz, py)).wrapping_shl(8);
    let u_dy = m(nz, px).wrapping_sub(m(nx, pz)).wrapping_shl(5);
    let v0 = m(mx, py).wrapping_sub(m(my, px)).wrapping_shl(14);
    let v_dx = m(my, pz).wrapping_sub(m(mz, py)).wrapping_shl(8);
    let v_dy = m(mz, px).wrapping_sub(m(mx, pz)).wrapping_shl(5);
    let d0 = m(my, nx).wrapping_sub(m(mx, ny)).wrapping_shl(14);
    let d_dx = m(mz, ny).wrapping_sub(m(my, nz)).wrapping_shl(8);
    let d_dy = m(mx, nz).wrapping_sub(m(mz, nx)).wrapping_shl(5);
    let (cx, cy) = tex.centre;
    walk(h, y, x, shades, 16, |yy, l, r, sa, sb| {
        let dy = yy - cy;
        let u = u0.wrapping_add(u_dy.wrapping_mul(dy));
        let v = v0.wrapping_add(v_dy.wrapping_mul(dy));
        let d = d0.wrapping_add(d_dy.wrapping_mul(dy));
        let row = (yy * w) as usize;
        let span = Span {
            l,
            r,
            shade: sa >> 8,
            shade_end: sb >> 8,
            num: (u, v, d),
            grad: (u_dx, v_dx, d_dx),
        };
        span_textured(&mut fb.pixels[row..row + w as usize], st, w, cx, page, span);
    });
}

/// One textured row's inputs: its x range, shades at both ends, and the
/// texture numerators/denominator at the row with their x gradients.
struct Span {
    l: i32,
    r: i32,
    shade: i32,
    shade_end: i32,
    num: (i32, i32, i32),
    grad: (i32, i32, i32),
}

/// One textured row: texture coordinates are divided out every 8 pixels
/// and interpolated linearly between; the shade selects one of four
/// darkened copies plus a further right shift.
fn span_textured(row: &mut [u32], st: State, w: i32, cx: i32, page: &TexturePage, s: Span) {
    let Span {
        mut l,
        mut r,
        mut shade,
        shade_end,
        num: (mut un, mut vn, mut dn),
        grad: (du, dv, dd),
    } = s;
    if l >= r {
        return;
    }
    let (runs, step);
    if st.clip {
        let s = (shade_end - shade) / (r - l);
        if r > w - 1 {
            r = w - 1;
        }
        if l < 0 {
            shade = shade.wrapping_sub(l.wrapping_mul(s));
            l = 0;
        }
        if l >= r {
            return;
        }
        runs = (r - l) >> 3;
        step = s << 12;
    } else if r - l > 7 {
        runs = (r - l) >> 3;
        step = (shade_end - shade).wrapping_mul(recip(runs)) >> 6;
    } else {
        runs = 0;
        step = 0;
    }
    shade = shade.wrapping_shl(9);
    let off = l - cx;
    un = un.wrapping_add((du >> 3).wrapping_mul(off));
    vn = vn.wrapping_add((dv >> 3).wrapping_mul(off));
    dn = dn.wrapping_add((dd >> 3).wrapping_mul(off));
    let (mut tu, mut tv) = (0i32, 0i32);
    let z = dn >> 14;
    if z != 0 {
        tu = (un / z).clamp(0, 16256);
        tv = vn / z;
    }
    un = un.wrapping_add(du);
    vn = vn.wrapping_add(dv);
    dn = dn.wrapping_add(dd);
    let (mut nu, mut nv) = (0i32, 0i32);
    let z = dn >> 14;
    if z != 0 {
        nu = (un / z).clamp(7, 16256);
        nv = vn / z;
    }
    let mut su = (nu - tu) >> 3;
    let mut sv = (nv - tv) >> 3;
    tu = tu.wrapping_add(shade & 0x600000);
    let mut dark = (shade >> 23) as u32;
    let mut i = l as isize;
    let tex = &page.texels;
    let put = |row: &mut [u32], i: isize, tu: i32, tv: i32, dark: u32| {
        let t = tex[(((tv & 0x3f80) + (tu >> 7)) & 0xffff) as usize] >> dark.min(31);
        if (page.opaque || t != 0)
            && let Some(d) = usize::try_from(i).ok().and_then(|i| row.get_mut(i))
        {
            *d = t;
        }
    };
    for _ in 0..runs {
        for k in 0..8 {
            put(row, i, tu, tv, dark);
            i += 1;
            if k < 7 {
                tu = tu.wrapping_add(su);
                tv = tv.wrapping_add(sv);
            }
        }
        tu = nu;
        tv = nv;
        un = un.wrapping_add(du);
        vn = vn.wrapping_add(dv);
        dn = dn.wrapping_add(dd);
        let z = dn >> 14;
        if z != 0 {
            nu = (un / z).clamp(7, 16256);
            nv = vn / z;
        }
        su = (nu - tu) >> 3;
        sv = (nv - tv) >> 3;
        shade = shade.wrapping_add(step);
        tu = tu.wrapping_add(shade & 0x600000);
        dark = (shade >> 23) as u32;
    }
    for _ in 0..((r - l) & 7) {
        put(row, i, tu, tv, dark);
        i += 1;
        tu = tu.wrapping_add(su);
        tv = tv.wrapping_add(sv);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fb() -> Framebuffer {
        Framebuffer::new(32, 32)
    }

    const OPAQUE: State = State {
        clip: false,
        alpha: 0,
    };

    #[test]
    fn flat_triangle_covers_its_interior_and_spans_are_half_open() {
        let mut f = fb();
        flat(&mut f, OPAQUE, [0, 0, 10], [0, 10, 0], 0xff0000);
        // Row 0 spans x 0..10 exclusive of the right edge.
        assert!(f.pixels[..10].iter().all(|&p| p == 0xff0000));
        assert_eq!(f.pixels[10], 0);
        // Rows 10 and below are untouched (the bottom vertex row is exclusive).
        assert!(f.pixels[10 * 32..].iter().all(|&p| p == 0));
        let filled = f.pixels.iter().filter(|&&p| p != 0).count();
        assert!((45..=55).contains(&filled), "{filled}");
    }

    #[test]
    fn vertex_order_does_not_change_coverage() {
        let tri = ([3, 20, 11], [5, 14, 27]);
        let mut reference = None;
        for perm in [
            [0, 1, 2],
            [1, 2, 0],
            [2, 0, 1],
            [0, 2, 1],
            [2, 1, 0],
            [1, 0, 2],
        ] {
            let mut f = fb();
            flat(
                &mut f,
                OPAQUE,
                perm.map(|i| tri.0[i]),
                perm.map(|i| tri.1[i]),
                7,
            );
            let count = f.pixels.iter().filter(|&&p| p == 7).count();
            assert_eq!(*reference.get_or_insert(count), count, "{perm:?}");
        }
    }

    #[test]
    fn rows_outside_the_buffer_are_clipped() {
        let mut f = fb();
        flat(
            &mut f,
            State {
                clip: true,
                alpha: 0,
            },
            [-20, -20, 60],
            [-20, 60, 10],
            9,
        );
        assert!(f.pixels.contains(&9));
        let mut f = fb();
        flat(&mut f, OPAQUE, [40, 40, 50], [0, 10, 0], 9);
        assert!(
            f.pixels.iter().all(|&p| p == 0),
            "a triangle below the buffer draws nothing"
        );
    }

    #[test]
    fn gouraud_interpolates_lightness_left_to_right() {
        let mut f = fb();
        let (dark, light) = (64 << 10 | 3 << 7 | 20, 64 << 10 | 3 << 7 | 110);
        gouraud(&mut f, OPAQUE, [0, 0, 31], [0, 31, 0], [dark, light, dark]);
        let lum = |p: u32| (p >> 16) + ((p >> 8) & 0xff) + (p & 0xff);
        assert!(
            lum(f.pixels[28]) > lum(f.pixels[2]),
            "brighter towards the lighter corner"
        );
        // 4-pixel runs share a colour.
        assert_eq!(f.pixels[0], f.pixels[3]);
    }

    #[test]
    fn blending_matches_the_client_formula() {
        assert_eq!(blend(0xff0000, 0x0000ff, 0), 0xff0000);
        assert_eq!(blend(0xff0000, 0x0000ff, 128), 0x7f007f);
        let mut f = fb();
        f.pixels.fill(0x0000ff);
        flat(
            &mut f,
            State {
                clip: false,
                alpha: 128,
            },
            [0, 0, 10],
            [0, 10, 0],
            0xff0000,
        );
        assert_eq!(f.pixels[1], 0x7f007f);
    }

    #[test]
    fn textured_triangle_samples_the_texture() {
        // A texture whose left half is red and right half blue.
        let mut texels = vec![0u32; 65536];
        for y in 0..128 {
            for x in 0..128 {
                texels[x + (y << 7)] = if x < 64 { 0xf80000 } else { 0x0000f8 };
            }
        }
        let page = TexturePage {
            texels,
            opaque: true,
        };
        let mut f = Framebuffer::new(64, 64);
        // Texture space spans the screen-facing square at depth 500.
        let tex = TexSpace {
            p: [-40, -40, 500],
            m: [40, -40, 500],
            n: [-40, 40, 500],
            centre: (32, 32),
        };
        let (cx, cy) = (32, 32);
        let proj = |v: [i32; 3]| (cx + (v[0] << 9) / v[2], cy + (v[1] << 9) / v[2]);
        let (a, b, c) = (proj(tex.p), proj(tex.m), proj(tex.n));
        textured(
            &mut f,
            OPAQUE,
            [a.1, b.1, c.1],
            [a.0, b.0, c.0],
            [0, 0, 0],
            tex,
            &page,
        );
        let reds = f.pixels.iter().filter(|&&p| p == 0xf80000).count();
        let blues = f.pixels.iter().filter(|&&p| p == 0x0000f8).count();
        assert!(reds > 50 && blues > 50, "red {reds}, blue {blues}");
    }
}
