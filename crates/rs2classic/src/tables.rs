//! Lookup tables shared by the renderer, built once on first use.
//!
//! ```text
//! sin[k]   = trunc(65536 · sin(k · 0.0030679615))   k = 0 … 2047
//! cos[k]   = trunc(65536 · cos(k · 0.0030679615))
//! recip[k] = 32768 / k, recip[0] = 0
//! hsl[c]   = 24-bit RGB for 16-bit HSL colour c, at the default brightness
//! ```

use std::sync::OnceLock;

struct Tables {
    /// 16.16 sine per 1/2048 turn.
    sin: [i32; 2048],
    /// 16.16 cosine per 1/2048 turn.
    cos: [i32; 2048],
    /// `32768 / k`, so `x * recip[k] >> 15` divides by `k`.
    recip: [i32; 2048],
    /// All 65536 HSL colours as `0xRRGGBB`.
    hsl: Vec<u32>,
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        let mut sin = [0; 2048];
        let mut cos = [0; 2048];
        let mut recip = [0; 2048];
        for k in 0..2048 {
            // About 2π / 2048: the original client's constant, kept so the
            // truncated values match.
            let a = k as f64 * 0.003_067_961_5;
            sin[k] = (65536.0 * a.sin()) as i32;
            cos[k] = (65536.0 * a.cos()) as i32;
            if k > 0 {
                recip[k] = 32768 / k as i32;
            }
        }
        let hsl = (0..=u16::MAX)
            .map(contentpack::palette::hsl_to_rgb)
            .collect();
        Tables {
            sin,
            cos,
            recip,
            hsl,
        }
    })
}

/// The original client's 16.16 sine for a 2048-unit angle (65536 = 1.0).
/// Angles wrap, negative ones included.
pub fn sin(k: i32) -> i32 {
    tables().sin[(k & 2047) as usize]
}

/// The original client's 16.16 cosine for a 2048-unit angle. Angles wrap.
pub fn cos(k: i32) -> i32 {
    tables().cos[(k & 2047) as usize]
}

/// `32768 / n` (0 for 0), the original client's span reciprocal table.
/// It stands in for a division when stepping along a span. `n` is clamped
/// to 0–2047.
pub fn recip(n: i32) -> i32 {
    tables().recip[n.clamp(0, 2047) as usize]
}

/// `0xRRGGBB` for a 16-bit HSL colour at the default brightness. Only the
/// low 16 bits of `c` count.
pub fn hsl(c: i32) -> u32 {
    tables().hsl[(c & 0xffff) as usize]
}
