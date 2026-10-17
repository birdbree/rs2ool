//! Renders real models with the classic renderer: a smooth-shaded model
//! with priorities and a textured model. Checks coverage and that every
//! drawn pixel comes from the HSL table or a texture, and writes PPM frames
//! under the target temp directory for a look.

use std::collections::BTreeSet;
use std::path::PathBuf;

use rs2cache::types::record::{Record, RecordType};
use rs2cache::{Cache, Records};
use rs2classic::light::{NPC, OBJECT};
use rs2classic::{Camera, Framebuffer, Textures, light, render};

fn ppm(name: &str, fb: &Framebuffer) {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("rs2classic");
    std::fs::create_dir_all(&dir).unwrap();
    let mut out = format!("P6\n{} {}\n255\n", fb.width, fb.height).into_bytes();
    for &p in &fb.pixels {
        out.extend([(p >> 16) as u8, (p >> 8) as u8, p as u8]);
    }
    std::fs::write(dir.join(format!("{name}.ppm")), out).unwrap();
}

fn textures(records: &Records) -> Textures {
    let sheets: Vec<_> = (0..50)
        .map(|t| match records.decode(RecordType::Texture, t) {
            Ok(Record::Texture(s)) => Some(s),
            _ => None,
        })
        .collect();
    Textures::from_sheets(&sheets, contentpack::palette::DEFAULT_BRIGHTNESS)
}

/// Centre a model on the origin (the camera looks at it).
fn centred(v: &[[i32; 3]]) -> Vec<[i32; 3]> {
    let (mut lo, mut hi) = ([i32::MAX; 3], [i32::MIN; 3]);
    for p in v {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    let c = [
        (lo[0] + hi[0]) / 2,
        (lo[1] + hi[1]) / 2,
        (lo[2] + hi[2]) / 2,
    ];
    v.iter()
        .map(|p| [p[0] - c[0], p[1] - c[1], p[2] - c[2]])
        .collect()
}

#[test]
fn renders_man_and_a_textured_model() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = Cache::open(&dir).unwrap();
    let records = Records::new(&cache);
    let tex = textures(&records);
    let index = cache.frame_index().unwrap();
    let man = rs2convert::creature::npc(&records, &index, 1, &[]).unwrap();
    let lit = light(&man.model, &NPC);
    let verts = centred(&man.model.vertices);
    let mut fb = Framebuffer::new(256, 256);
    fb.clear(0x101010);
    render(&mut fb, &lit, &verts, &Camera::orbit(1800, 60, 550), &tex);
    ppm("man", &fb);
    let drawn: Vec<u32> = fb
        .pixels
        .iter()
        .copied()
        .filter(|&p| p != 0x101010)
        .collect();
    assert!(drawn.len() > 3000, "only {} pixels drawn", drawn.len());
    let table: BTreeSet<u32> = (0..=u16::MAX)
        .map(contentpack::palette::hsl_to_rgb)
        .collect();
    assert!(
        drawn.iter().all(|p| table.contains(p)),
        "smooth/flat pixels come from the HSL table"
    );
    // The figure is taller than wide and centred.
    let rows: Vec<usize> = (0..256)
        .filter(|y| {
            fb.pixels[y * 256..(y + 1) * 256]
                .iter()
                .any(|&p| p != 0x101010)
        })
        .collect();
    let cols: Vec<usize> = (0..256)
        .filter(|x| (0..256).any(|y| fb.pixels[y * 256 + x] != 0x101010))
        .collect();
    assert!(
        rows.len() > cols.len(),
        "{} rows vs {} cols",
        rows.len(),
        cols.len()
    );

    // The first model with textured faces.
    let (tid, tm) = (0..cache.file_count(1).unwrap() as u32)
        .find_map(|id| {
            let m = cache.model(id).ok()?;
            (m.face_types
                .as_ref()?
                .iter()
                .filter(|t| *t & 2 != 0)
                .count()
                > 20)
                .then_some((id, m))
        })
        .unwrap();
    let lit = light(&tm, &OBJECT);
    let verts = centred(&tm.vertices);
    let mut fb = Framebuffer::new(256, 256);
    render(&mut fb, &lit, &verts, &Camera::orbit(300, 200, 900), &tex);
    ppm("textured", &fb);
    let drawn = fb.pixels.iter().filter(|&&p| p != 0).count();
    eprintln!("textured model {tid}: {drawn} pixels drawn");
    assert!(drawn > 1000);
}
