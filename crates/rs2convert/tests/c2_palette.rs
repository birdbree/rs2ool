//! The 1024-entry palette quantises the model colours with a mean error
//! under 6 and a max under 24 RGB units.

mod common;

use rs2cache::Records;
use rs2convert::palette::{Palette, corpus};

#[test]
fn palette_error_is_within_bounds() {
    let Some(cache) = common::cache() else { return };
    let records = Records::new(&cache);
    let corpus = corpus(&records).unwrap();
    let mut p = Palette::build(&corpus);
    p.prime(&corpus);
    let e = p.error(&corpus);
    eprintln!("{} entries; {e:?}", p.rgb.len());
    assert_eq!(p.rgb.len(), 1024);
    assert!(e.mean < 6.0, "mean error {}", e.mean);
    assert!(e.max < 24.0, "max error {} (hsl {})", e.max, e.worst_hsl);
}
