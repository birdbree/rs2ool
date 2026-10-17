//! The three triangle fillers. Each walks the triangle's rows the way the
//! original client does (edge slopes in 16.16 fixed point from the topmost
//! vertex, rows clipped to the buffer, the long edge left or right by
//! slope) and fills each row with the matching span routine.
//!
//! Spans are half-open, `[left, right)`, and the bottom vertex's row isn't
//! drawn. All the maths wraps like 32-bit Java ints, because vertices just
//! past the near plane project to enormous screen coordinates and the
//! original wraps on them too.
//!
//! ```text
//! edge x            16.16
//! gouraud colour    walked << 15, spanned with 8 fraction bits
//! texture shade     walked << 16, spanned with 8 then 17 fraction bits
//! texture u, v      texels × 128 (7 fraction bits)
//! ```

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

/// Blend `src` over `dst` as the original client does:
/// `(src·(256−a) + dst·a) >> 8` per channel, `a` being 317 transparency.
pub fn blend(src: u32, dst: u32, alpha: i32) -> u32 {
    // Red and blue go together under 0xff00ff, green alone under 0xff00,
    // so one multiply handles two channels without them overlapping.
    let (a, inv) = (alpha as u32, (256 - alpha) as u32);
    let s = ((((src & 0xff00ff) * inv) >> 8) & 0xff00ff) + ((((src & 0xff00) * inv) >> 8) & 0xff00);
    s + ((((dst & 0xff00ff) * a) >> 8) & 0xff00ff) + ((((dst & 0xff00) * a) >> 8) & 0xff00)
}

/// One edge being walked: x in 16.16 and an attribute in fixed point.
#[derive(Debug, Clone, Copy)]
struct Edge {
    /// Current x, 16.16.
    x: i32,
    /// x step per row.
    dx: i32,
    /// Current attribute (colour or shade), fixed point.
    a: i32,
    /// Attribute step per row.
    da: i32,
}

impl Edge {
    /// Move down one row.
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
    // Per-row (x, attr) steps along the edge from vertex i to j; flat edges
    // get 0.
    let slope = |i: usize, j: usize| -> (i32, i32) {
        if y[j] == y[i] {
            (0, 0)
        } else {
            let dy = y[j].wrapping_sub(y[i]);
            (
                x[j].wrapping_sub(x[i]).wrapping_shl(16).wrapping_div(dy),
                attr[j]
                    .wrapping_sub(attr[i])
                    .wrapping_shl(shift)
                    .wrapping_div(dy),
            )
        }
    };
    // t: the top vertex (smallest y, earlier one on a tie).
    let t = if y[0] <= y[1] && y[0] <= y[2] {
        0
    } else if y[1] <= y[2] {
        1
    } else {
        2
    };
    // n and p: the vertices after and before t, in face order. "next" is
    // the edge t→n, "prev" the edge t→p, and "third" joins n and p.
    let (n, p) = ((t + 1) % 3, (t + 2) % 3);
    let (s_tn, s_pt, s_np) = (slope(t, n), slope(p, t), slope(n, p));
    let mut yt = y[t];
    if yt >= height {
        return;
    }
    // Where the two lower vertices sit, cut off at the bottom of the buffer.
    let (mut yn, mut yp) = (y[n].min(height), y[p].min(height));
    // An edge starting at vertex v with slope s, in fixed point.
    let start = |v: usize, s: (i32, i32)| Edge {
        x: x[v].wrapping_shl(16),
        dx: s.0,
        a: attr[v].wrapping_shl(shift),
        da: s.1,
    };
    // Fast-forward an edge that starts at row `from` < 0 to row 0.
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
    // Emit one row from left edge e1 to right edge e2; `>> 16` takes the
    // integer part of x.
    let mut row = |e1: &Edge, e2: &Edge, yy: i32| span(yy, e1.x >> 16, e2.x >> 16, e1.a, e2.a);
    if y[n] < y[p] {
        // The next edge ends first; the third edge runs from n to p.
        let mut third = start(n, s_np);
        if yn < 0 {
            clip_to_top(&mut third, yn);
            yn = 0;
        }
        // Whether prev is the left edge: compare slopes at the top, or, with
        // a flat top, prev against the third edge.
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
        // Same test, mirrored: a flat top compares the third edge with next.
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
        // Walked with 15 fraction bits; keep 8 for the span.
        let (mut c, end) = (ca >> 7, cb >> 7);
        let row = (yy * w) as usize;
        // Colour step per 4-pixel run.
        let step = if st.clip {
            // s: colour step per pixel, by real division. Too-short spans
            // don't step at all.
            let s = if r.wrapping_sub(l) > 3 {
                end.wrapping_sub(c).wrapping_div(r.wrapping_sub(l))
            } else {
                0
            };
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
            s << 2
        } else {
            if l >= r {
                return;
            }
            // Divide by the run count through the reciprocal table:
            // x · (32768 / runs) >> 15 is x / runs.
            let runs = (r - l) >> 2;
            if runs > 0 {
                (end - c).wrapping_mul(recip(runs)) >> 15
            } else {
                0
            }
        };
        let px = &mut fb.pixels[row..row + w as usize];
        let mut i = l as isize;
        // Write one pixel, blended if the face is transparent. Out-of-row
        // indices are dropped rather than panicking.
        let put = |px: &mut [u32], i: isize, colour: u32| {
            if let Some(d) = usize::try_from(i).ok().and_then(|i| px.get_mut(i)) {
                *d = if st.alpha == 0 {
                    colour
                } else {
                    blend(colour, *d, st.alpha)
                };
            }
        };
        // One colour lookup per 4 pixels; `c >> 8` drops the fraction.
        for _ in 0..((r - l) >> 2) {
            let colour = hsl(c >> 8);
            c = c.wrapping_add(step);
            for _ in 0..4 {
                put(px, i, colour);
                i += 1;
            }
        }
        // The last 0–3 pixels share one more colour.
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
        // The original client only leaves clipping off when every vertex is
        // on screen. Clamp anyway, so bad input can't index past the row.
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
    /// Camera-space P: texture coordinate (0, 0).
    pub p: [i32; 3],
    /// Camera-space M: the far end of the u axis.
    pub m: [i32; 3],
    /// Camera-space N: the far end of the v axis.
    pub n: [i32; 3],
    /// The projection centre (half the buffer size).
    pub centre: (i32, i32),
}

/// A perspective-textured triangle with per-corner shades (0–127).
///
/// The texture triangle sets up three functions linear in screen position,
/// U, V and D, and each pixel's texture coordinate is `U / D`, `V / D`
/// (scaled so u runs 0 at P to 128 texels at M, and v 0 at P to 128 at N).
/// Transparency is ignored; texel 0 is see-through unless the page is
/// opaque.
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
    // M' = (mx, my, mz) = P − M, but N' = (nx, ny, nz) = N − P. Mind the
    // signs: the cross products below expect exactly that.
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
    // U = N' × P, V = M' × P, D = N' × M'. Their z, x and y parts are the
    // value at the projection centre (`0`), the change per 8 pixels across
    // (`_dx`) and the change per row (`_dy`). The shifts are the original
    // client's fixed-point scales.
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
        // U, V and D at this row, at the centre column.
        let dy = yy - cy;
        let u = u0.wrapping_add(u_dy.wrapping_mul(dy));
        let v = v0.wrapping_add(v_dy.wrapping_mul(dy));
        let d = d0.wrapping_add(d_dy.wrapping_mul(dy));
        let row = (yy * w) as usize;
        let span = Span {
            l,
            r,
            // Walked with 16 fraction bits; keep 8.
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
    /// First column.
    l: i32,
    /// One past the last column.
    r: i32,
    /// Shade at `l`, 8 fraction bits.
    shade: i32,
    /// Shade at `r`, 8 fraction bits.
    shade_end: i32,
    /// (U, V, D) at the row's centre column.
    num: (i32, i32, i32),
    /// Their change per 8 pixels.
    grad: (i32, i32, i32),
}

/// One textured row: texture coordinates are divided out every 8 pixels
/// and interpolated linearly between; the shade selects one of four
/// darkened copies plus a further right shift.
///
/// ```text
/// u, v   = U / (D >> 14), V / (D >> 14)        texels × 128
/// copy   = (shade >> 4) & 3                    which darkened page copy
/// texel  = page[copy·16384 + (v & 0x3f80) + (u >> 7)] >> (shade >> 6)
/// ```
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
    // runs: whole 8-pixel runs. step: shade change per run, in the
    // 17-fraction-bit form `shade` takes below.
    let (runs, step);
    if st.clip {
        // Per-pixel shade step by real division; × 8 per run and << 9 for
        // the format gives `<< 12`.
        let s = shade_end
            .wrapping_sub(shade)
            .wrapping_div(r.wrapping_sub(l));
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
        // Δ · (32768 / runs) >> 6 is Δ / runs << 9: per run, already in
        // the 17-fraction-bit format.
        step = (shade_end - shade).wrapping_mul(recip(runs)) >> 6;
    } else {
        runs = 0;
        step = 0;
    }
    // 8 → 17 fraction bits. Now bits 21–22 hold (shade >> 4) & 3, the page
    // copy, and bits 23+ hold shade >> 6, the extra darkening shift.
    shade = shade.wrapping_shl(9);
    // Move U, V, D from the centre column to column l (grads are per 8
    // pixels, hence `>> 3`).
    let off = l - cx;
    un = un.wrapping_add((du >> 3).wrapping_mul(off));
    vn = vn.wrapping_add((dv >> 3).wrapping_mul(off));
    dn = dn.wrapping_add((dd >> 3).wrapping_mul(off));
    // (tu, tv): texture coordinate now, texels × 128. A zero denominator
    // leaves it at 0.
    let (mut tu, mut tv) = (0i32, 0i32);
    let z = dn >> 14;
    if z != 0 {
        tu = un.wrapping_div(z).clamp(0, 16256);
        tv = vn.wrapping_div(z);
    }
    // (nu, nv): the coordinate 8 pixels on, the end of the first run.
    un = un.wrapping_add(du);
    vn = vn.wrapping_add(dv);
    dn = dn.wrapping_add(dd);
    let (mut nu, mut nv) = (0i32, 0i32);
    let z = dn >> 14;
    if z != 0 {
        nu = un.wrapping_div(z).clamp(7, 16256);
        nv = vn.wrapping_div(z);
    }
    // (su, sv): per-pixel step across the run.
    let mut su = (nu - tu) >> 3;
    let mut sv = nv.wrapping_sub(tv) >> 3;
    // Fold the page copy into tu: after `>> 7` in the lookup, 0x600000
    // becomes copy · 16384, the copy's offset in the page.
    tu = tu.wrapping_add(shade & 0x600000);
    // dark: how far to shift the texel right on top of that.
    let mut dark = (shade >> 23) as u32;
    let mut i = l as isize;
    let tex = &page.texels;
    // Sample and write one pixel. `tv & 0x3f80` is the texel row × 128 and
    // `tu >> 7` the column plus copy offset. Texel 0 is skipped unless the
    // page is opaque.
    let put = |row: &mut [u32], i: isize, tu: i32, tv: i32, dark: u32| {
        let t = tex[(((tv & 0x3f80) + (tu >> 7)) & 0xffff) as usize] >> dark.min(31);
        if (page.opaque || t != 0)
            && let Some(d) = usize::try_from(i).ok().and_then(|i| row.get_mut(i))
        {
            *d = t;
        }
    };
    for _ in 0..runs {
        // Eight pixels stepping linearly; the last one doesn't step,
        // because tu/tv snap to the exact next coordinate below.
        for k in 0..8 {
            put(row, i, tu, tv, dark);
            i += 1;
            if k < 7 {
                tu = tu.wrapping_add(su);
                tv = tv.wrapping_add(sv);
            }
        }
        // Snap to the run's exact end, then divide out the next one.
        tu = nu;
        tv = nv;
        un = un.wrapping_add(du);
        vn = vn.wrapping_add(dv);
        dn = dn.wrapping_add(dd);
        let z = dn >> 14;
        if z != 0 {
            nu = un.wrapping_div(z).clamp(7, 16256);
            nv = vn.wrapping_div(z);
        }
        su = (nu - tu) >> 3;
        sv = nv.wrapping_sub(tv) >> 3;
        // The shade only changes between runs.
        shade = shade.wrapping_add(step);
        tu = tu.wrapping_add(shade & 0x600000);
        dark = (shade >> 23) as u32;
    }
    // The last 0–7 pixels keep stepping towards the next divided-out
    // coordinate.
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
    fn extreme_coordinates_wrap_instead_of_panicking() {
        // Vertices just past the near plane project far off screen; the
        // client's int arithmetic wraps there and so must ours.
        let clip = State {
            clip: true,
            alpha: 0,
        };
        let big = [i32::MIN / 2, i32::MAX / 2, 7];
        let ys = [-(1 << 30), 5, 1 << 30];
        let mut f = fb();
        flat(&mut f, clip, ys, big, 0xffffff);
        gouraud(&mut f, clip, ys, big, [0, 60000, 127]);
        let page = TexturePage {
            texels: vec![0x808080; 65536],
            opaque: true,
        };
        let tex = TexSpace {
            p: [i32::MIN, 0, 50],
            m: [i32::MAX, 1, 50],
            n: [0, i32::MAX, 51],
            centre: (16, 16),
        };
        textured(&mut f, clip, ys, big, [0, 64, 127], tex, &page);
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
