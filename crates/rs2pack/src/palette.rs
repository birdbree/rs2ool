//! 317 HSL colours and the client palette.
//!
//! A 317 face colour is 16-bit HSL: `hue = (c >> 10) & 63`,
//! `sat = (c >> 7) & 7`, `lum = c & 127`. [`hsl_to_rgb`] ports the 317
//! client's table builder (`Texture.method372`) at its default brightness,
//! without the small random jitter the client adds at startup.

/// The client's default brightness (the gamma exponent).
pub const DEFAULT_BRIGHTNESS: f64 = 0.8;

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

fn gamma(rgb: u32, brightness: f64) -> u32 {
    let f = |c: u32| ((f64::from(c) / 256.0).powf(brightness) * 256.0) as u32;
    (f(rgb >> 16) << 16) + (f((rgb >> 8) & 0xff) << 8) + f(rgb & 0xff)
}

/// 24-bit RGB for a 317 HSL colour, as the client draws it.
pub fn hsl_to_rgb_with(hsl: u16, brightness: f64) -> u32 {
    let hue = f64::from((hsl >> 10) & 63) / 64.0 + 0.0078125;
    let sat = f64::from((hsl >> 7) & 7) / 8.0 + 0.0625;
    let lum = f64::from(hsl & 127) / 128.0;
    let q = if lum < 0.5 {
        lum * (1.0 + sat)
    } else {
        lum + sat - lum * sat
    };
    let p = 2.0 * lum - q;
    let mut r = hue + 1.0 / 3.0;
    if r > 1.0 {
        r -= 1.0;
    }
    let mut b = hue - 1.0 / 3.0;
    if b < 0.0 {
        b += 1.0;
    }
    let (r, g, b) = (channel(r, p, q), channel(hue, p, q), channel(b, p, q));
    let rgb = (((r * 256.0) as u32) << 16) + (((g * 256.0) as u32) << 8) + (b * 256.0) as u32;
    let rgb = gamma(rgb, brightness) & 0xff_ffff;
    // The client reserves 0 for transparency.
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
