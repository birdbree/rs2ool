//! 317 HSL colours, and the helpers a palette is built with.
//!
//! A 317 face colour is 16-bit HSL:
//!
//! ```text
//! bits 15–10   hue          (c >> 10) & 63
//! bits 9–7     saturation   (c >> 7) & 7
//! bits 6–0     lightness    c & 127
//! ```
//!
//! [`hsl_to_rgb`] gives the same RGB the original client puts in its colour
//! table at the default brightness: a standard HSL-to-RGB conversion with
//! hue and saturation nudged up by half a step, then a gamma curve. The
//! original client also adds a small random jitter to the brightness at
//! startup; that's left out here, so you get the same colours every run.
//!
//! The rest is for picking a palette: [`rgb_distance`] and [`nearest`] for
//! matching colours, and a deterministic weighted [`kmeans`] for reducing
//! many colours to a few.

/// The original client's default brightness, used as the gamma exponent.
pub const DEFAULT_BRIGHTNESS: f64 = 0.8;

// One channel of the standard HSL-to-RGB piecewise ramp: `t` is the
// channel's hue position in 0..1, `p` and `q` the low and high levels.
fn channel(t: f64, p: f64, q: f64) -> f64 {
    if 6.0 * t < 1.0 {
        p + (q - p) * 6.0 * t
    } else if 2.0 * t < 1.0 {
        q
    } else if 3.0 * t < 2.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    }
}

/// Apply a brightness (gamma) curve to a packed 24-bit colour, as the
/// original client does when it builds its colour and texture tables.
///
/// Each channel becomes `trunc((c / 256) ^ brightness · 256)`. With the
/// default 0.8 that lifts the darker values.
pub fn gamma(rgb: u32, brightness: f64) -> u32 {
    // Per channel: scale to 0..1, raise to the exponent, scale back, truncate.
    let f = |c: u32| ((f64::from(c) / 256.0).powf(brightness) * 256.0) as u32;
    (f(rgb >> 16) << 16) + (f((rgb >> 8) & 0xff) << 8) + f(rgb & 0xff)
}

/// 24-bit RGB for a 317 HSL colour at a given brightness, as the original
/// client draws it.
///
/// Never returns 0: pure black comes out as 1, since 0 means transparent.
pub fn hsl_to_rgb_with(hsl: u16, brightness: f64) -> u32 {
    // Hue and saturation each get half a step added (1/128 and 1/16), which
    // is why saturation 0 still carries a faint tint.
    let hue = f64::from((hsl >> 10) & 63) / 64.0 + 0.0078125;
    let sat = f64::from((hsl >> 7) & 7) / 8.0 + 0.0625;
    let lum = f64::from(hsl & 127) / 128.0;
    // q and p are the high and low levels of the HSL ramp.
    let q = if lum < 0.5 {
        lum * (1.0 + sat)
    } else {
        lum + sat - lum * sat
    };
    let p = 2.0 * lum - q;
    // Red and blue sit a third of a turn either side of green, wrapped to 0..1.
    let mut r = hue + 1.0 / 3.0;
    if r > 1.0 {
        r -= 1.0;
    }
    let mut b = hue - 1.0 / 3.0;
    if b < 0.0 {
        b += 1.0;
    }
    let (r, g, b) = (channel(r, p, q), channel(hue, p, q), channel(b, p, q));
    // Truncate each channel to 0..256 and pack as 0xRRGGBB.
    let rgb = (((r * 256.0) as u32) << 16) + (((g * 256.0) as u32) << 8) + (b * 256.0) as u32;
    let rgb = gamma(rgb, brightness) & 0xff_ffff;
    // The original client reserves 0 for transparency.
    if rgb == 0 { 1 } else { rgb }
}

/// 24-bit RGB for a 317 HSL colour at the default brightness.
pub fn hsl_to_rgb(hsl: u16) -> u32 {
    hsl_to_rgb_with(hsl, DEFAULT_BRIGHTNESS)
}

/// `[r, g, b]` bytes of a packed 24-bit colour.
pub fn split(rgb: u32) -> [u8; 3] {
    [(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greys_and_extremes() {
        // Saturation 0 is still offset by 1/16, so greys carry a tint of
        // their hue; lum 0 is black (mapped to 1), lum 127 near white.
        assert_eq!(hsl_to_rgb(0), 1);
        let [r, g, b] = split(hsl_to_rgb(127));
        assert!(r > 240 && g > 240 && b > 240, "{r} {g} {b}");
        // Pure-ish red: hue 0, full saturation, mid luminance.
        let [r, g, b] = split(hsl_to_rgb(7 << 7 | 64));
        assert!(r > 200 && g < 80 && b < 80, "{r} {g} {b}");
    }

    #[test]
    fn every_colour_is_24_bit_and_nonzero() {
        for c in 0..=u16::MAX {
            let rgb = hsl_to_rgb(c);
            assert!(rgb != 0 && rgb <= 0xff_ffff);
        }
    }
}

/// Squared Euclidean distance between two colours in RGB.
pub fn rgb_distance2(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum()
}

/// Euclidean distance between two colours in RGB units.
pub fn rgb_distance(a: [u8; 3], b: [u8; 3]) -> f64 {
    rgb_distance2(a.map(f64::from), b.map(f64::from)).sqrt()
}

/// Index of the entry of `palette` nearest `c` in RGB.
pub fn nearest(palette: &[[f64; 3]], c: [f64; 3]) -> usize {
    let mut best = (0, f64::INFINITY);
    for (i, p) in palette.iter().enumerate() {
        let d = rgb_distance2(c, *p);
        if d < best.1 {
            best = (i, d);
        }
    }
    best.0
}

/// Weighted k-means in RGB.
///
/// `points` are `(colour, weight)`; the result has at most `k` entries, or
/// just the points themselves when there are no more than `k`. It's
/// deterministic: the first centre is the heaviest point, each next one is
/// the point with the greatest `distance² × √weight` to the centres so far
/// (ties go to the earliest point), then Lloyd iterations run until the
/// assignments stop changing or `iterations` runs out. A centre that ends
/// up with no points stays put. Centres come back unrounded.
pub fn kmeans(points: &[([f64; 3], f64)], k: usize, iterations: usize) -> Vec<[f64; 3]> {
    if points.len() <= k {
        return points.iter().map(|p| p.0).collect();
    }
    // Heaviest point; on a tie, `b.cmp(&a)` makes the earlier index win.
    let heaviest = (0..points.len())
        .max_by(|&a, &b| points[a].1.total_cmp(&points[b].1).then(b.cmp(&a)))
        .unwrap_or(0);
    let mut centres = vec![points[heaviest].0];
    // Squared distance from each point to its nearest centre so far.
    let mut nearest_d: Vec<f64> = points
        .iter()
        .map(|p| rgb_distance2(p.0, centres[0]))
        .collect();
    while centres.len() < k {
        // (index, score) of the best candidate; strict `>` keeps the earliest.
        let mut next = (0, -1.0);
        for (i, (d, p)) in nearest_d.iter().zip(points).enumerate() {
            let score = d * p.1.sqrt();
            if score > next.1 {
                next = (i, score);
            }
        }
        let c = points[next.0].0;
        centres.push(c);
        // Only the new centre can bring a point closer.
        for (d, p) in nearest_d.iter_mut().zip(points) {
            *d = d.min(rgb_distance2(p.0, c));
        }
    }
    // Centre index per point. Starts out invalid so round one never matches.
    let mut assign = vec![usize::MAX; points.len()];
    for _ in 0..iterations {
        let fresh: Vec<usize> = points.iter().map(|p| nearest(&centres, p.0)).collect();
        if fresh == assign {
            break;
        }
        assign = fresh;
        // Per centre: weighted r, g, b sums in 0..3, total weight in 3.
        let mut sums = vec![[0.0f64; 4]; centres.len()];
        for (p, &a) in points.iter().zip(&assign) {
            let s = &mut sums[a];
            for (sum, v) in s.iter_mut().zip(p.0) {
                *sum += v * p.1;
            }
            s[3] += p.1;
        }
        // Move each centre to the weighted mean of its points.
        for (c, s) in centres.iter_mut().zip(sums) {
            if s[3] > 0.0 {
                *c = [s[0] / s[3], s[1] / s[3], s[2] / s[3]];
            }
        }
    }
    centres
}

#[cfg(test)]
mod quantise_tests {
    use super::*;

    #[test]
    fn kmeans_is_deterministic_and_separates_clusters() {
        let mut pts = Vec::new();
        for i in 0..50 {
            pts.push(([200.0 + f64::from(i % 5), 10.0, 10.0], 1.0));
            pts.push(([10.0, 10.0, 200.0 + f64::from(i % 5)], 2.0));
        }
        let a = kmeans(&pts, 2, 20);
        assert_eq!(a, kmeans(&pts, 2, 20));
        assert!(
            a.iter().any(|c| c[0] > 190.0) && a.iter().any(|c| c[2] > 190.0),
            "{a:?}"
        );
        assert_eq!(
            nearest(&a, [190.0, 0.0, 0.0]),
            a.iter().position(|c| c[0] > 190.0).unwrap()
        );
    }
}
