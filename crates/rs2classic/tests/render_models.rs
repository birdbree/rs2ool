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
    let ids = records.ids(RecordType::Texture).unwrap();
    let count = ids.iter().max().map_or(0, |&m| m + 1);
    let sheets: Vec<_> = (0..count)
        .map(|t| match records.decode(RecordType::Texture, t) {
            Ok(Record::Texture(s)) => Some(s),
            _ => None,
        })
        .collect();
    Textures::from_sheets(&sheets, contentpack::palette::DEFAULT_BRIGHTNESS)
}

/// Per-axis extent of a set of vertices.
fn extent(v: &[[i32; 3]]) -> [i32; 3] {
    let (mut lo, mut hi) = ([i32::MAX; 3], [i32::MIN; 3]);
    for p in v {
        for a in 0..3 {
            lo[a] = lo[a].min(p[a]);
            hi[a] = hi[a].max(p[a]);
        }
    }
    [hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]]
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

/// A camera distance that frames a centred model in a `size`-pixel frame:
/// its bounding sphere projects (focal length 512) to about three
/// quarters of the frame.
fn framing(v: &[[i32; 3]], size: i32) -> i32 {
    let r = v
        .iter()
        .map(|p| {
            let [x, y, z] = p.map(f64::from);
            (x * x + y * y + z * z).sqrt()
        })
        .fold(0.0, f64::max);
    ((512.0 * r) / (f64::from(size) * 3.0 / 8.0)).ceil() as i32
}

/// The columns and rows of `fb` holding a pixel other than `bg`.
fn drawn_box(fb: &Framebuffer, bg: u32) -> (usize, usize) {
    let (w, h) = (fb.width, fb.height);
    let rows = (0..h)
        .filter(|y| fb.pixels[y * w..(y + 1) * w].iter().any(|&p| p != bg))
        .count();
    let cols = (0..w)
        .filter(|x| (0..h).any(|y| fb.pixels[y * w + x] != bg))
        .count();
    (cols, rows)
}

#[test]
fn renders_an_npc_and_a_textured_model() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = Cache::open(&dir).unwrap();
    let records = Records::new(&cache);
    let tex = textures(&records);
    let index = cache.frame_index().unwrap();

    // The first NPC with several body models whose merged mesh is taller
    // than it is wide or deep: a standing figure.
    let (id, npc) = records
        .ids(RecordType::Npc)
        .unwrap()
        .into_iter()
        .find_map(|id| {
            let Ok(Record::Npc(d)) = records.decode(RecordType::Npc, id) else {
                return None;
            };
            if d.models.len() < 2 {
                return None;
            }
            let c = rs2convert::creature::npc(&records, &index, id, &[]).ok()?;
            let [x, y, z] = extent(&c.model.vertices);
            (!c.model.faces.is_empty() && y > x.max(z)).then_some((id, c))
        })
        .expect("no NPC with two or more models that is taller than wide");
    let lit = light(&npc.model, &NPC);
    let verts = centred(&npc.model.vertices);
    let size = 256;
    let bg = 0x101010;
    let mut fb = Framebuffer::new(size, size);
    fb.clear(bg);
    let distance = framing(&verts, size as i32);
    render(
        &mut fb,
        &lit,
        &verts,
        &Camera::orbit(1800, 60, distance),
        &tex,
    );
    ppm("npc", &fb);
    let drawn: Vec<u32> = fb.pixels.iter().copied().filter(|&p| p != bg).collect();
    let (cols, rows) = drawn_box(&fb, bg);
    eprintln!(
        "npc {id}: {} pixels drawn in {cols}x{rows} at distance {distance}",
        drawn.len()
    );
    // The figure fills a fair part of the frame and of its own box.
    assert!(
        rows > size / 2 && drawn.len() > cols * rows / 4,
        "only {} pixels drawn in {cols}x{rows}",
        drawn.len()
    );
    let table: BTreeSet<u32> = (0..=u16::MAX)
        .map(contentpack::palette::hsl_to_rgb)
        .collect();
    assert!(
        drawn.iter().all(|p| table.contains(p)),
        "smooth/flat pixels come from the HSL table"
    );
    // The figure is drawn taller than wide.
    assert!(rows > cols, "{rows} rows vs {cols} cols");

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
        .expect("no model with more than 20 textured faces");
    let lit = light(&tm, &OBJECT);
    let verts = centred(&tm.vertices);
    let mut fb = Framebuffer::new(size, size);
    let distance = framing(&verts, size as i32);
    render(
        &mut fb,
        &lit,
        &verts,
        &Camera::orbit(300, 200, distance),
        &tex,
    );
    ppm("textured", &fb);
    let drawn = fb.pixels.iter().filter(|&&p| p != 0).count();
    let (cols, rows) = drawn_box(&fb, 0);
    eprintln!("textured model {tid}: {drawn} pixels drawn in {cols}x{rows}");
    assert!(
        cols.max(rows) > size / 2 && drawn > cols * rows / 4,
        "only {drawn} pixels drawn in {cols}x{rows}"
    );
}
