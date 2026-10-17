//! The 2D viewport: textures, sprites, fonts and interfaces, painted with
//! egui over the viewport rect.
//!
//! Images are nearest-filtered and drawn at an integer zoom in physical
//! pixels, so one texel covers exactly `zoom × zoom` screen pixels; a
//! checkerboard shows behind transparency. egui works in logical points,
//! so most of the maths here is dividing physical pixels by the window's
//! scale factor (`ppp`, pixels per point) and snapping back to whole
//! pixels.
//!
//! The 3D viewport stays where it is underneath; this just paints over it
//! whenever the selection [`is_2d`](Renderable::is_2d).

use std::collections::{BTreeMap, BTreeSet};

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use rs2cache::Records;
use rs2cache::types::interface::{Content, Interface, SpriteRef};
use rs2cache::types::record::{Record, RecordType};
use rs2cache::types::sprite::{Font, SpriteSheet};

use crate::explorer::renderable::Renderable;
use crate::explorer::selection::Loaded;
use crate::explorer::ui::ViewportRect;

/// Zoom steps offered by the controls.
pub const ZOOMS: [u32; 5] = [1, 2, 3, 4, 8];

/// Glyphs per row and column of a font's glyph sheet.
pub const GLYPH_GRID: usize = 16;

/// An image ready to upload: RGBA8, row-major.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// `width × height × 4` bytes, not premultiplied.
    pub pixels: Vec<u8>,
}

impl Rgba {
    /// As an egui image, ready to upload.
    fn color_image(&self) -> egui::ColorImage {
        egui::ColorImage::from_rgba_unmultiplied([self.width, self.height], &self.pixels)
    }
}

/// Sub-sprite `i` of a sheet: textures on their untrimmed canvas (as the
/// original client uses them), other sprites trimmed. `None` past the end.
pub fn sheet_image(sheet: &SpriteSheet, i: usize, untrimmed: bool) -> Option<Rgba> {
    let s = if untrimmed {
        sheet.untrimmed(i)?
    } else {
        sheet.sprites.get(i)?.clone()
    };
    // A one-sprite sheet sized to the sprite, so `rgba(0)` gives just it.
    let one = SpriteSheet {
        max_width: s.width,
        max_height: s.height,
        palette: sheet.palette.clone(),
        sprites: vec![s],
    };
    let pixels = one.rgba(0)?;
    Some(Rgba {
        width: usize::from(one.max_width),
        height: usize::from(one.max_height),
        pixels,
    })
}

/// A font's 256 glyphs laid out 16 × 16, each in a cell the size of the
/// largest glyph with a one-pixel gutter. Returns the sheet and the cell
/// size.
///
/// Unlike the dump's glyph sheet, each glyph sits at its cell's top-left,
/// ignoring its draw offsets; the metrics panel lists those instead.
pub fn glyph_sheet(font: &Font) -> (Rgba, (usize, usize)) {
    let g = &font.glyphs;
    let cw = g
        .sprites
        .iter()
        .map(|s| usize::from(s.width))
        .max()
        .unwrap_or(1)
        + 1;
    let ch = g
        .sprites
        .iter()
        .map(|s| usize::from(s.height))
        .max()
        .unwrap_or(1)
        + 1;
    let (w, h) = (cw * GLYPH_GRID, ch * GLYPH_GRID);
    let mut pixels = vec![0u8; w * h * 4];
    for (i, s) in g.sprites.iter().enumerate().take(GLYPH_GRID * GLYPH_GRID) {
        // Glyph `i`'s cell: column `i % 16`, row `i / 16`.
        let (ox, oy) = ((i % GLYPH_GRID) * cw, (i / GLYPH_GRID) * ch);
        let sw = usize::from(s.width);
        for (k, &p) in s.pixels.iter().enumerate() {
            // Palette index 0 is transparent.
            if p == 0 {
                continue;
            }
            // Pixels run row by row.
            let (x, y) = (ox + k % sw, oy + k / sw);
            let c = g.palette.get(usize::from(p)).copied().unwrap_or(0xffffff);
            let at = (x + y * w) * 4;
            pixels[at..at + 4].copy_from_slice(&[(c >> 16) as u8, (c >> 8) as u8, c as u8, 255]);
        }
    }
    (
        Rgba {
            width: w,
            height: h,
            pixels,
        },
        (cw, ch),
    )
}

/// One drawn interface component, positioned relative to the root.
#[derive(Debug, Clone, PartialEq)]
pub struct UiItem {
    /// The component's interface id.
    pub id: u16,
    /// Left edge, in interface pixels from the root's top-left.
    pub x: i32,
    /// Top edge, in interface pixels from the root's top-left.
    pub y: i32,
    /// Width in interface pixels.
    pub width: u16,
    /// Height in interface pixels.
    pub height: u16,
    /// What to draw.
    pub kind: UiKind,
}

/// What an interface component draws as.
#[derive(Debug, Clone, PartialEq)]
pub enum UiKind {
    /// A container, drawn as an outline.
    Layer,
    /// A rectangle.
    Rect {
        /// `0xRRGGBB`.
        rgb: u32,
        /// Filled, or just outlined.
        filled: bool,
        /// 255 opaque.
        opacity: u8,
    },
    /// A media-archive sprite, by name and index.
    Sprite(SpriteRef),
    /// A line of text, drawn in a stand-in font.
    Text {
        /// The text as stored.
        text: String,
        /// `0xRRGGBB`.
        rgb: u32,
        /// Centred on the component rather than left-aligned.
        centered: bool,
        /// With a black drop shadow one pixel down and right.
        shadow: bool,
    },
    /// An inventory, model or other component, drawn as a labelled box.
    Placeholder(&'static str),
}

/// An interface laid out as the original client nests it: each layer's
/// children at their offsets, with the media sprites they name.
#[derive(Debug, Clone, Default)]
pub struct InterfaceScene {
    /// The root's width, in interface pixels.
    pub width: u16,
    /// The root's height, in interface pixels.
    pub height: u16,
    /// Every component, parents before children, so later ones draw on
    /// top.
    pub items: Vec<UiItem>,
    /// The media sprite sheets the sprites name, by name. A name that
    /// won't load is just missing, and its sprites aren't drawn.
    pub sheets: BTreeMap<String, SpriteSheet>,
}

/// Lays out interface `root` and its descendants. Children that fail to
/// decode, and cycles, are skipped, and it stops after 4096 components.
pub fn interface_scene(records: &Records, root: &Interface) -> InterfaceScene {
    let mut scene = InterfaceScene {
        width: root.width,
        height: root.height,
        ..Default::default()
    };
    let mut seen = BTreeSet::new();
    place(records, root, 0, 0, &mut scene, &mut seen);
    // Load each sheet once, however many components use it.
    let names: BTreeSet<String> = scene
        .items
        .iter()
        .filter_map(|i| match &i.kind {
            UiKind::Sprite(r) => Some(r.name.clone()),
            _ => None,
        })
        .collect();
    for n in names {
        if let Ok(sheet) = records.media_sprite(&n) {
            scene.sheets.insert(n, sheet);
        }
    }
    scene
}

/// Adds component `c` at `(x, y)`, then its children (for a layer) at
/// their offsets from it, depth first.
fn place(
    records: &Records,
    c: &Interface,
    x: i32,
    y: i32,
    scene: &mut InterfaceScene,
    seen: &mut BTreeSet<u16>,
) {
    // Each component once (that's what breaks cycles), and a hard cap.
    if !seen.insert(c.id) || seen.len() > 4096 {
        return;
    }
    // Colours are stored as i32; keep the low 24 bits.
    let rgb = |v: i32| (v as u32) & 0xff_ffff;
    let kind = match &c.content {
        Content::Layer { .. } => UiKind::Layer,
        Content::Rectangle { filled, colour, .. } => UiKind::Rect {
            rgb: rgb(*colour),
            filled: *filled,
            // The cache stores transparency; flip it to opacity.
            opacity: 255 - c.alpha,
        },
        Content::Sprite { sprite, .. } => match sprite {
            Some(r) => UiKind::Sprite(r.clone()),
            None => UiKind::Placeholder("sprite"),
        },
        Content::Text {
            style,
            text,
            colour,
            ..
        } => UiKind::Text {
            text: text.clone(),
            rgb: rgb(*colour),
            centered: style.centered,
            shadow: style.shadow,
        },
        Content::Inventory { .. } => UiKind::Placeholder("inventory"),
        Content::Model { .. } => UiKind::Placeholder("model"),
        Content::ItemText { .. } => UiKind::Placeholder("item text"),
        Content::Tooltip { .. } => UiKind::Placeholder("tooltip"),
        Content::Unused { .. } | Content::Other => UiKind::Placeholder("component"),
    };
    scene.items.push(UiItem {
        id: c.id,
        x,
        y,
        width: c.width,
        height: c.height,
        kind,
    });
    if let Content::Layer { children, .. } = &c.content {
        for ch in children {
            if let Ok(Record::Interface(child)) =
                records.decode(RecordType::Interface, u32::from(ch.id))
            {
                place(
                    records,
                    &child,
                    x + i32::from(ch.x),
                    y + i32::from(ch.y),
                    scene,
                    seen,
                );
            }
        }
    }
}

/// Where an image `w × h` texels lands at `zoom`: its top-left in logical
/// points and its size in logical points, such that each texel covers
/// `zoom` physical pixels. `origin` is the viewport's top-left in points
/// and `pan` the image's offset from it.
pub fn image_rect(
    origin: egui::Pos2,
    pan: egui::Vec2,
    (w, h): (usize, usize),
    zoom: u32,
    pixels_per_point: f32,
) -> egui::Rect {
    // Physical size is a whole number of pixels; convert it to points.
    let size = egui::vec2(
        (w as u32 * zoom) as f32 / pixels_per_point,
        (h as u32 * zoom) as f32 / pixels_per_point,
    );
    // Snap to whole physical pixels so texels stay square and crisp.
    let snap = |v: f32| (v * pixels_per_point).round() / pixels_per_point;
    let min = origin + pan;
    egui::Rect::from_min_size(egui::pos2(snap(min.x), snap(min.y)), size)
}

/// The texture filtering for every 2D image: nearest, so zoomed texels
/// stay crisp.
pub const TEXTURE_OPTIONS: egui::TextureOptions = egui::TextureOptions::NEAREST;

/// Whether the loaded selection is drawn by the 2D viewport.
pub fn active(loaded: Res<Loaded>) -> bool {
    loaded.renderable.as_deref().is_some_and(Renderable::is_2d)
}

/// The 2D viewport's state.
#[derive(Resource)]
pub struct View2d {
    /// Physical pixels per texel, one of [`ZOOMS`].
    pub zoom: u32,
    /// Offset of the image from the viewport's top-left corner, in points.
    pub pan: egui::Vec2,
    /// Sub-sprite shown for a sheet.
    pub page: usize,
    /// The uploaded image and what it shows.
    texture: Option<(ImageKey, egui::TextureHandle)>,
    /// Interface sprites, by `(name, index)`.
    sprites: BTreeMap<(String, u32), egui::TextureHandle>,
    /// Address of the renderable the state belongs to, to spot a new
    /// selection.
    for_renderable: usize,
}

impl Default for View2d {
    fn default() -> Self {
        Self {
            zoom: 4,
            pan: egui::vec2(16.0, 48.0),
            page: 0,
            texture: None,
            sprites: BTreeMap::new(),
            for_renderable: 0,
        }
    }
}

/// What the uploaded texture shows: a renderable (by address) and its
/// sub-sprite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ImageKey {
    renderable: usize,
    page: usize,
}

/// Paints a checkerboard of `cell`-point squares filling `rect`.
fn checkerboard(p: &egui::Painter, rect: egui::Rect, cell: f32) {
    let (a, b) = (egui::Color32::from_gray(60), egui::Color32::from_gray(90));
    // Dark everywhere first, then the light squares on top.
    p.rect_filled(rect, 0.0, a);
    let (nx, ny) = (
        (rect.width() / cell).ceil() as i32,
        (rect.height() / cell).ceil() as i32,
    );
    // A huge zoomed image would mean too many squares; plain dark it is.
    if nx * ny > 40_000 {
        return;
    }
    for j in 0..ny {
        // Odd rows start one square in, which makes the checks.
        for i in (j & 1..nx).step_by(2) {
            let min = rect.min + egui::vec2(i as f32 * cell, j as f32 * cell);
            // Trim the squares on the right and bottom edges.
            let r = egui::Rect::from_min_size(min, egui::vec2(cell, cell)).intersect(rect);
            p.rect_filled(r, 0.0, b);
        }
    }
}

/// Paints the 2D selection over the viewport, with its controls, and
/// handles drag-to-pan.
fn draw(
    mut contexts: EguiContexts,
    loaded: Res<Loaded>,
    rect: Res<ViewportRect>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut view: ResMut<View2d>,
) -> Result {
    let Some(r) = loaded.renderable.as_deref().filter(|r| r.is_2d()) else {
        return Ok(());
    };
    let ctx = contexts.ctx_mut()?.clone();
    let ppp = window.scale_factor();
    // The viewport rect in points.
    let area = egui::Rect::from_min_size(
        egui::pos2(rect.position.x as f32 / ppp, rect.position.y as f32 / ppp),
        egui::vec2(rect.size.x as f32 / ppp, rect.size.y as f32 / ppp),
    );
    // A new selection starts on its first sub-sprite. The renderable's
    // address identifies it.
    let key = std::ptr::from_ref(r) as usize;
    if view.for_renderable != key {
        view.for_renderable = key;
        view.page = 0;
        view.sprites.clear();
    }
    // Drag to pan anywhere over the viewport.
    let id = egui::Id::new("viewport2d");
    let layer = egui::LayerId::new(egui::Order::Middle, id);
    // An invisible area over the whole viewport that catches drags.
    let response = egui::Area::new(id.with("input"))
        .order(egui::Order::Middle)
        .fixed_pos(area.min)
        .show(&ctx, |ui| {
            ui.allocate_rect(
                egui::Rect::from_min_size(area.min, area.size()),
                egui::Sense::drag(),
            )
        })
        .inner;
    if response.dragged() {
        view.pan += response.drag_delta();
    }
    let painter = ctx.layer_painter(layer).with_clip_rect(area);
    painter.rect_filled(area, 0.0, egui::Color32::from_gray(28));
    let zoom = view.zoom;
    let mut pages = 0;
    let mut info = String::new();
    match r {
        Renderable::Sheet { sheet, untrimmed } => {
            pages = sheet.sprites.len();
            view.page = view.page.min(pages.saturating_sub(1));
            let page = view.page;
            if let Some(img) = sheet_image(sheet, page, *untrimmed) {
                let tex = upload(
                    &ctx,
                    &mut view,
                    ImageKey {
                        renderable: key,
                        page,
                    },
                    &img,
                );
                let at = image_rect(area.min, view.pan, (img.width, img.height), zoom, ppp);
                // Checks 8 physical pixels across, but never under 4 points.
                checkerboard(&painter, at, (8.0 / ppp).max(4.0));
                painter.image(tex.id(), at, uv(), egui::Color32::WHITE);
                // Palette swatches under the image, 32 to a row. Entry 0 is
                // transparent, so it's skipped.
                let sw = 12.0;
                for (i, &c) in sheet.palette.iter().enumerate().skip(1) {
                    let min = at.left_bottom()
                        + egui::vec2(((i - 1) % 32) as f32 * sw, 8.0 + ((i - 1) / 32) as f32 * sw);
                    let col = egui::Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8);
                    painter.rect_filled(
                        egui::Rect::from_min_size(min, egui::vec2(sw - 1.0, sw - 1.0)),
                        0.0,
                        col,
                    );
                }
                let s = &sheet.sprites[page];
                info = format!(
                    "{}×{} at ({}, {}) on {}×{}, {} colours",
                    s.width,
                    s.height,
                    s.x_offset,
                    s.y_offset,
                    sheet.max_width,
                    sheet.max_height,
                    sheet.palette.len().saturating_sub(1)
                );
            }
        }
        Renderable::Image(img) => {
            let tex = upload(
                &ctx,
                &mut view,
                ImageKey {
                    renderable: key,
                    page: 0,
                },
                img,
            );
            let at = image_rect(area.min, view.pan, (img.width, img.height), zoom, ppp);
            checkerboard(&painter, at, (8.0 / ppp).max(4.0));
            painter.image(tex.id(), at, uv(), egui::Color32::WHITE);
            info = format!("{}×{}", img.width, img.height);
        }
        Renderable::Font(font) => {
            let (img, (cw, ch)) = glyph_sheet(font);
            let tex = upload(
                &ctx,
                &mut view,
                ImageKey {
                    renderable: key,
                    page: 0,
                },
                &img,
            );
            let at = image_rect(area.min, view.pan, (img.width, img.height), zoom, ppp);
            painter.rect_filled(at, 0.0, egui::Color32::BLACK);
            painter.image(tex.id(), at, uv(), egui::Color32::WHITE);
            let stroke = egui::Stroke::new(1.0, egui::Color32::from_gray(50));
            // Grid lines on the cell edges: cell size × zoom, in points.
            for i in 0..=GLYPH_GRID {
                let fx = at.min.x + (i * cw) as f32 * zoom as f32 / ppp;
                let fy = at.min.y + (i * ch) as f32 * zoom as f32 / ppp;
                painter.line_segment([egui::pos2(fx, at.min.y), egui::pos2(fx, at.max.y)], stroke);
                painter.line_segment([egui::pos2(at.min.x, fy), egui::pos2(at.max.x, fy)], stroke);
            }
            metrics_window(&ctx, area, font);
            info = format!("{}: 256 glyphs in {cw}×{ch} cells", font.name);
        }
        Renderable::Interface(scene) => {
            draw_interface(&ctx, &painter, &mut view, area, scene, ppp);
            info = format!(
                "{}×{}, {} components",
                scene.width,
                scene.height,
                scene.items.len()
            );
        }
        _ => {}
    }
    controls(&ctx, area, &mut view, pages, &info);
    Ok(())
}

/// The whole texture, for `painter.image`.
fn uv() -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
}

/// The texture for `key`, uploading `img` when it changed. There's one
/// texture slot, reused for every image.
fn upload(
    ctx: &egui::Context,
    view: &mut View2d,
    key: ImageKey,
    img: &Rgba,
) -> egui::TextureHandle {
    match &mut view.texture {
        Some((k, t)) if *k == key => t.clone(),
        Some((k, t)) => {
            t.set(img.color_image(), TEXTURE_OPTIONS);
            *k = key;
            t.clone()
        }
        None => {
            let t = ctx.load_texture("viewport2d", img.color_image(), TEXTURE_OPTIONS);
            view.texture = Some((key, t.clone()));
            t
        }
    }
}

/// The floating controls at the viewport's top-left: zoom, Reset (the
/// pan), the sub-sprite stepper when there's more than one, and `info`.
fn controls(ctx: &egui::Context, area: egui::Rect, view: &mut View2d, pages: usize, info: &str) {
    egui::Area::new(egui::Id::new("viewport2d_controls"))
        .order(egui::Order::Foreground)
        .fixed_pos(area.min + egui::vec2(8.0, 8.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Zoom");
                    for z in ZOOMS {
                        ui.selectable_value(&mut view.zoom, z, format!("{z}×"));
                    }
                    if ui.button("Reset").clicked() {
                        view.pan = View2d::default().pan;
                    }
                });
                if pages > 1 {
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(view.page > 0, egui::Button::new("◀"))
                            .clicked()
                        {
                            view.page -= 1;
                        }
                        ui.label(format!("sprite {} of {pages}", view.page + 1));
                        if ui
                            .add_enabled(view.page + 1 < pages, egui::Button::new("▶"))
                            .clicked()
                        {
                            view.page += 1;
                        }
                    });
                }
                if !info.is_empty() {
                    ui.weak(info);
                }
            });
        });
}

/// A font's metrics beside its glyph sheet: per character, its advance
/// and draw offsets.
fn metrics_window(ctx: &egui::Context, area: egui::Rect, font: &Font) {
    egui::Area::new(egui::Id::new("viewport2d_metrics"))
        .order(egui::Order::Foreground)
        .fixed_pos(egui::pos2(area.max.x - 220.0, area.min.y + 8.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(200.0);
                ui.strong("Metrics");
                egui::ScrollArea::vertical()
                    .max_height((area.height() - 60.0).max(80.0))
                    .show(ui, |ui| {
                        egui::Grid::new("metrics").striped(true).show(ui, |ui| {
                            for h in ["char", "adv", "dx", "dy"] {
                                ui.weak(h);
                            }
                            ui.end_row();
                            for i in 0..font.advance.len() {
                                // Printable ASCII as itself, anything else
                                // by number.
                                let c = char::from(i as u8);
                                let shown = if c.is_ascii_graphic() {
                                    c.to_string()
                                } else {
                                    format!("{i}")
                                };
                                ui.monospace(shown);
                                ui.monospace(font.advance[i].to_string());
                                ui.monospace(
                                    font.draw_offset_x
                                        .get(i)
                                        .map_or(String::new(), ToString::to_string),
                                );
                                ui.monospace(
                                    font.draw_offset_y
                                        .get(i)
                                        .map_or(String::new(), ToString::to_string),
                                );
                                ui.end_row();
                            }
                        });
                    });
            });
        });
}

/// Draws an interface: its components at their nested positions, scaled by
/// the zoom; hovering a component outlines it and shows its id and size.
fn draw_interface(
    ctx: &egui::Context,
    painter: &egui::Painter,
    view: &mut View2d,
    area: egui::Rect,
    scene: &InterfaceScene,
    ppp: f32,
) {
    // Points per interface pixel.
    let k = view.zoom as f32 / ppp;
    // The root's top-left, snapped to a physical pixel like an image's.
    let origin = image_rect(area.min, view.pan, (0, 0), 1, ppp).min;
    // Interface pixels to a screen rect.
    let to = |x: i32, y: i32, w: u16, h: u16| {
        egui::Rect::from_min_size(
            origin + egui::vec2(x as f32 * k, y as f32 * k),
            egui::vec2(f32::from(w) * k, f32::from(h) * k),
        )
    };
    painter.rect_filled(
        to(0, 0, scene.width, scene.height),
        0.0,
        egui::Color32::BLACK,
    );
    let rgb = |c: u32, a: u8| {
        egui::Color32::from_rgba_unmultiplied((c >> 16) as u8, (c >> 8) as u8, c as u8, a)
    };
    // The last (topmost) component under the pointer wins.
    let mut hovered = None;
    let pointer = ctx.pointer_hover_pos();
    for item in &scene.items {
        let r = to(item.x, item.y, item.width, item.height);
        match &item.kind {
            UiKind::Layer => {
                painter.rect_stroke(
                    r,
                    0.0,
                    egui::Stroke::new(1.0, egui::Color32::from_gray(70)),
                    egui::StrokeKind::Inside,
                );
            }
            UiKind::Rect {
                rgb: c,
                filled,
                opacity,
            } => {
                if *filled {
                    painter.rect_filled(r, 0.0, rgb(*c, *opacity));
                } else {
                    painter.rect_stroke(
                        r,
                        0.0,
                        egui::Stroke::new(k.max(1.0), rgb(*c, *opacity)),
                        egui::StrokeKind::Inside,
                    );
                }
            }
            UiKind::Sprite(sr) => {
                let Some(sheet) = scene.sheets.get(&sr.name) else {
                    continue;
                };
                let ix = sr.index as usize;
                let Some(s) = sheet.sprites.get(ix) else {
                    continue;
                };
                // Upload each sprite once per selection.
                let tex = view
                    .sprites
                    .entry((sr.name.clone(), sr.index))
                    .or_insert_with(|| {
                        let img = sheet_image(sheet, ix, false).unwrap_or(Rgba {
                            width: 1,
                            height: 1,
                            pixels: vec![0; 4],
                        });
                        ctx.load_texture(
                            format!("ui {} {}", sr.name, sr.index),
                            img.color_image(),
                            TEXTURE_OPTIONS,
                        )
                    })
                    .clone();
                // The original client draws a sprite at its stored offsets.
                let at = to(
                    item.x + i32::from(s.x_offset),
                    item.y + i32::from(s.y_offset),
                    s.width,
                    s.height,
                );
                painter.image(tex.id(), at, uv(), egui::Color32::WHITE);
            }
            UiKind::Text {
                text,
                rgb: c,
                centered,
                shadow,
            } => {
                // An 11-point stand-in at 1×, scaled with the zoom.
                let font = egui::FontId::monospace(11.0 * k);
                let (pos, align) = if *centered {
                    (r.center_top(), egui::Align2::CENTER_TOP)
                } else {
                    (r.left_top(), egui::Align2::LEFT_TOP)
                };
                if *shadow {
                    painter.text(
                        pos + egui::vec2(k, k),
                        align,
                        text,
                        font.clone(),
                        egui::Color32::BLACK,
                    );
                }
                painter.text(pos, align, text, font, rgb(*c, 255));
            }
            UiKind::Placeholder(what) => {
                painter.rect_stroke(
                    r,
                    0.0,
                    egui::Stroke::new(1.0, egui::Color32::from_rgb(90, 90, 140)),
                    egui::StrokeKind::Inside,
                );
                painter.text(
                    r.left_top() + egui::vec2(2.0, 2.0),
                    egui::Align2::LEFT_TOP,
                    *what,
                    egui::FontId::proportional(10.0),
                    egui::Color32::from_rgb(140, 140, 200),
                );
            }
        }
        if pointer.is_some_and(|p| r.contains(p) && area.contains(p)) {
            hovered = Some((item, r));
        }
    }
    if let Some((item, r)) = hovered {
        painter.rect_stroke(
            r,
            0.0,
            egui::Stroke::new(1.5, egui::Color32::YELLOW),
            egui::StrokeKind::Outside,
        );
        painter.text(
            r.left_bottom() + egui::vec2(0.0, 2.0),
            egui::Align2::LEFT_TOP,
            format!("interface {} ({}×{})", item.id, item.width, item.height),
            egui::FontId::proportional(12.0),
            egui::Color32::YELLOW,
        );
    }
}

/// Adds [`View2d`] and the painter, which runs after the panel layout.
pub fn plugin(app: &mut App) {
    app.init_resource::<View2d>().add_systems(
        EguiPrimaryContextPass,
        draw.after(crate::explorer::ui::Layout),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use rs2cache::types::sprite::Sprite;

    #[test]
    fn zoomed_images_cover_whole_physical_pixels() {
        let r = image_rect(
            egui::pos2(10.3, 20.0),
            egui::vec2(0.0, 0.0),
            (51, 51),
            4,
            2.0,
        );
        assert_eq!(r.width() * 2.0, 204.0, "each texel is 4 physical pixels");
        assert_eq!((r.min.x * 2.0).fract(), 0.0, "snapped to a physical pixel");
        assert_eq!(TEXTURE_OPTIONS.magnification, egui::TextureFilter::Nearest);
    }

    #[test]
    fn glyph_sheet_has_a_cell_per_glyph() {
        let glyph = |w: u16| Sprite {
            x_offset: 0,
            y_offset: 0,
            width: w,
            height: 3,
            column_major: false,
            pixels: vec![1; usize::from(w) * 3],
        };
        let font = Font {
            name: "t".into(),
            glyphs: SpriteSheet {
                max_width: 5,
                max_height: 3,
                palette: vec![0, 0xff0000],
                sprites: (0..256).map(|i| glyph(1 + (i % 5) as u16)).collect(),
            },
            advance: vec![4; 256],
            draw_offset_x: vec![0; 256],
            draw_offset_y: vec![0; 256],
        };
        let (img, (cw, ch)) = glyph_sheet(&font);
        assert_eq!((cw, ch), (6, 4));
        assert_eq!((img.width, img.height), (96, 64));
        // Glyph 17 (row 1, column 1) has its top-left texel set.
        let at = (cw + ch * img.width) * 4;
        assert_eq!(&img.pixels[at..at + 4], &[255, 0, 0, 255]);
    }
}
