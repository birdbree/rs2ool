//! Lookup tables shared by the renderer.

use std::sync::OnceLock;

struct Tables {
    sin: [i32; 2048],
    cos: [i32; 2048],
    recip: [i32; 2048],
    hsl: Vec<u32>,
}

fn tables() -> &'static Tables {
    static T: OnceLock<Tables> = OnceLock::new();
    T.get_or_init(|| {
        let mut sin = [0; 2048];
        let mut cos = [0; 2048];
        let mut recip = [0; 2048];
        for k in 0..2048 {
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

/// The client's 16.16 sine for a 2048-unit angle.
pub fn sin(k: i32) -> i32 {
    tables().sin[(k & 2047) as usize]
}

pub fn cos(k: i32) -> i32 {
    tables().cos[(k & 2047) as usize]
}

/// `32768 / n` (0 for 0), the client's span reciprocal table.
pub fn recip(n: i32) -> i32 {
    tables().recip[n.clamp(0, 2047) as usize]
}

/// RGB for a 16-bit HSL colour at the default brightness.
pub fn hsl(c: i32) -> u32 {
    tables().hsl[(c & 0xffff) as usize]
}
