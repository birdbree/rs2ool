//! A sprite renders crisp at 4× (nearest filtering, each texel exactly four
//! physical pixels), a font lays out 256 glyphs, and an interface nests its
//! children with their sprites.

#![cfg(feature = "explorer")]

use std::path::PathBuf;

use bevy_egui::egui;
use rs2cache::types::record::{Record, RecordType};
use rs2ool::explorer::renderable::Renderable;
use rs2ool::explorer::selection::{Sel, load};
use rs2ool::explorer::sources::{Source, open_cache};
use rs2ool::explorer::viewport2d::{
    GLYPH_GRID, TEXTURE_OPTIONS, UiKind, glyph_sheet, image_rect, sheet_image,
};

fn cache() -> Option<Source> {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return None;
    };
    Some(open_cache(&dir).unwrap())
}

#[test]
fn sprite_is_crisp_at_4x() {
    let Some(src) = cache() else { return };
    let Source::Cache { records, .. } = &src else {
        unreachable!()
    };
    // The first media sprite sheet whose first sub-sprite is fully opaque.
    let (id, want) = records
        .ids(RecordType::Sprite)
        .unwrap()
        .into_iter()
        .filter(|&id| {
            records
                .sprite_source(id)
                .is_ok_and(|(n, _)| n == rs2cache::archive::MEDIA)
        })
        .find_map(|id| {
            let Ok(Record::Sprite(sheet)) = records.decode(RecordType::Sprite, id) else {
                return None;
            };
            let s = sheet.sprites.first()?;
            let img = sheet_image(&sheet, 0, false)?;
            (s.width > 0 && img.pixels.chunks(4).all(|p| p[3] == 255))
                .then_some((id, (usize::from(s.width), usize::from(s.height))))
        })
        .expect("a media sprite with an opaque sub-sprite");
    let (_, r) = load(&src, &Sel::Record(RecordType::Sprite, id)).unwrap();
    let Some(Renderable::Sheet { sheet, untrimmed }) = r else {
        panic!("a sprite sheet")
    };
    assert!(!untrimmed);
    let img = sheet_image(&sheet, 0, untrimmed).unwrap();
    assert_eq!((img.width, img.height), want, "the sub-sprite's own size");
    assert!(
        img.pixels.chunks(4).all(|p| p[3] == 255),
        "the sprite is opaque"
    );
    for ppp in [1.0, 2.0] {
        let at = image_rect(
            egui::pos2(0.0, 0.0),
            egui::vec2(0.0, 0.0),
            (img.width, img.height),
            4,
            ppp,
        );
        assert_eq!(
            at.width() * ppp,
            4.0 * img.width as f32,
            "4 physical pixels per texel"
        );
        assert_eq!(at.height() * ppp, 4.0 * img.height as f32);
    }
    assert_eq!(TEXTURE_OPTIONS.magnification, egui::TextureFilter::Nearest);
    assert_eq!(TEXTURE_OPTIONS.minification, egui::TextureFilter::Nearest);
}

#[test]
fn p12_full_shows_256_glyphs() {
    let Some(src) = cache() else { return };
    let (_, r) = load(&src, &Sel::Record(RecordType::Font, 1)).unwrap();
    let Some(Renderable::Font(font)) = r else {
        panic!("a font")
    };
    assert_eq!(font.name, "p12_full");
    assert_eq!(font.glyphs.sprites.len(), GLYPH_GRID * GLYPH_GRID);
    let (img, (cw, ch)) = glyph_sheet(&font);
    assert_eq!((img.width, img.height), (cw * GLYPH_GRID, ch * GLYPH_GRID));
    // The printable ASCII glyphs all have ink in their cells.
    let inked = |i: usize| {
        let (ox, oy) = ((i % GLYPH_GRID) * cw, (i / GLYPH_GRID) * ch);
        (0..ch).any(|y| (0..cw).any(|x| img.pixels[((ox + x) + (oy + y) * img.width) * 4 + 3] != 0))
    };
    let blank: Vec<usize> = (33..127).filter(|&i| !inked(i)).collect();
    assert!(blank.is_empty(), "glyphs without ink: {blank:?}");
}

#[test]
fn interfaces_nest_their_children_with_sprites() {
    let Some(src) = cache() else { return };
    // Any layer with sprite children will do, so take the first few.
    let Source::Cache { records, .. } = &src else {
        unreachable!()
    };
    let ids = records.ids(RecordType::Interface).unwrap();
    let mut checked = 0;
    for id in ids {
        let (_, r) = load(&src, &Sel::Record(RecordType::Interface, id)).unwrap();
        let Some(Renderable::Interface(scene)) = r else {
            panic!("interface {id} is not drawn in 2D")
        };
        let sprites: Vec<_> = scene
            .items
            .iter()
            .filter_map(|i| match &i.kind {
                UiKind::Sprite(r) => Some(r),
                _ => None,
            })
            .collect();
        if scene.items.len() < 5 || sprites.is_empty() {
            continue;
        }
        assert!(
            sprites.iter().any(|r| scene.sheets.contains_key(&r.name)),
            "interface {id}: its sprites resolve in the media archive"
        );
        assert!(
            scene.items.iter().skip(1).any(|i| i.x != 0 || i.y != 0),
            "interface {id}: children are offset"
        );
        checked += 1;
        if checked == 3 {
            break;
        }
    }
    assert_eq!(checked, 3, "found layered interfaces with sprites");
}
