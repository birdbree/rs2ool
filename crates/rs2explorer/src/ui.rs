//! The egui layout: menu bar, source tree (left), inspector (right), jobs
//! and playback (bottom); the remaining central rect is published as
//! [`ViewportRect`] for the cameras.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use rs2intermediate::record::RecordType;

use crate::classic::{ClassicView, RenderMode};
use crate::inspector;
use crate::jobs::Jobs;
use crate::selection::{Loaded, Sel, Selected};
use crate::sources::{Kind, Source, Sources};
use crate::tree::{self, ListKey, Lists};
use crate::viewport3d::{LightingMode, Overlays};

/// The layout system, for ordering painters that draw over the viewport.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Layout;

/// The central area left for the viewports, in physical pixels.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct ViewportRect {
    pub position: UVec2,
    pub size: UVec2,
}

const ROW_HEIGHT: f32 = 18.0;

fn list_ui(
    ui: &mut egui::Ui,
    key: ListKey,
    source: &Source,
    si: usize,
    lists: &mut Lists,
    selected: &mut Selected,
) {
    lists.request(key, source);
    let search = lists.search.entry(key).or_default();
    ui.add(
        egui::TextEdit::singleline(search)
            .hint_text("search id or name")
            .desired_width(f32::INFINITY),
    );
    let filter = search.clone();
    match lists.rows.get(&key) {
        None => {
            ui.spinner();
        }
        Some(Err(e)) => {
            ui.colored_label(egui::Color32::LIGHT_RED, e);
        }
        Some(Ok(rows)) => {
            let shown: Vec<&tree::Row> = rows.iter().filter(|r| r.matches(&filter)).collect();
            ui.label(format!("{} of {}", shown.len(), rows.len()));
            egui::ScrollArea::vertical()
                .id_salt(format!("{key:?}"))
                .max_height(240.0)
                .show_rows(ui, ROW_HEIGHT, shown.len(), |ui, range| {
                    for r in &shown[range] {
                        let sel = match key {
                            ListKey::Type(_, t) => Sel::Record(t, r.id),
                            ListKey::Pack(_) => Sel::PackRecord(r.id),
                            ListKey::Dump(_) => Sel::DumpFile(r.name.clone().into()),
                        };
                        let is = selected.source == Some(si) && selected.sel.as_ref() == Some(&sel);
                        if ui.selectable_label(is, r.label()).clicked() {
                            *selected = Selected {
                                source: Some(si),
                                sel: Some(sel),
                            };
                        }
                    }
                });
        }
    }
}

fn tree_ui(ui: &mut egui::Ui, sources: &Sources, lists: &mut Lists, selected: &mut Selected) {
    for (si, s) in sources.list.iter().enumerate() {
        egui::CollapsingHeader::new(s.label())
            .id_salt(si)
            .default_open(true)
            .show(ui, |ui| match s {
                Source::Cache { .. } => {
                    egui::CollapsingHeader::new("indices")
                        .id_salt((si, "idx"))
                        .show(ui, |ui| {
                            for row in tree::index_rows(s) {
                                ui.label(format!(
                                    "index {}: {} files ({} present)",
                                    row.index, row.files, row.present
                                ));
                            }
                        });
                    for &t in RecordType::ALL {
                        egui::CollapsingHeader::new(t.name())
                            .id_salt((si, t.name()))
                            .show(ui, |ui| {
                                list_ui(ui, ListKey::Type(si, t), s, si, lists, selected);
                            });
                    }
                }
                Source::Pack { .. } => list_ui(ui, ListKey::Pack(si), s, si, lists, selected),
                Source::DumpDir { .. } => list_ui(ui, ListKey::Dump(si), s, si, lists, selected),
            });
    }
}

fn grid(ui: &mut egui::Ui, id: &str, rows: &[(String, String)]) {
    egui::Grid::new(id)
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            for (k, v) in rows {
                ui.monospace(k);
                ui.label(v);
                ui.end_row();
            }
        });
}

fn inspector_ui(ui: &mut egui::Ui, loaded: &Loaded) {
    ui.heading("Inspector");
    if loaded.is_loading() {
        ui.spinner();
    }
    if let Some(e) = &loaded.error {
        ui.colored_label(egui::Color32::LIGHT_RED, e);
    }
    let Some(data) = &loaded.data else { return };
    let (meta, fields) = inspector::sections(data);
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::CollapsingHeader::new("metadata")
            .default_open(true)
            .show(ui, |ui| grid(ui, "meta", &meta));
        if !fields.is_empty() {
            egui::CollapsingHeader::new("fields")
                .default_open(true)
                .show(ui, |ui| grid(ui, "fields", &fields));
        }
        if let Some(bytes) = inspector::raw_bytes(data) {
            egui::CollapsingHeader::new(format!("bytes ({})", bytes.len())).show(ui, |ui| {
                let lines = bytes.len().div_ceil(16);
                egui::ScrollArea::vertical()
                    .id_salt("hex")
                    .max_height(300.0)
                    .show_rows(ui, ROW_HEIGHT, lines, |ui, range| {
                        for row in range {
                            ui.monospace(inspector::hex_line(bytes, row));
                        }
                    });
            });
        }
    });
}

/// The resources the layout reads and writes.
#[derive(SystemParam)]
struct UiState<'w> {
    sources: ResMut<'w, Sources>,
    lists: ResMut<'w, Lists>,
    selected: ResMut<'w, Selected>,
    loaded: Res<'w, Loaded>,
    jobs: Res<'w, Jobs>,
    rect: ResMut<'w, ViewportRect>,
    render_mode: ResMut<'w, RenderMode>,
    lighting: ResMut<'w, LightingMode>,
    overlays: ResMut<'w, Overlays>,
    classic: ResMut<'w, ClassicView>,
}

fn layout(
    mut contexts: EguiContexts,
    mut st: UiState,
    window: Single<&Window, With<PrimaryWindow>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    let mut root = egui::Ui::new(
        ctx.clone(),
        "root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    let top = egui::Panel::top("menu")
        .show(&mut root, |ui| {
            ui.horizontal(|ui| {
                ui.menu_button("View", |ui| {
                    ui.label("Renderer");
                    ui.radio_value(&mut *st.render_mode, RenderMode::Modern, "Modern (Bevy)");
                    ui.radio_value(
                        &mut *st.render_mode,
                        RenderMode::Classic,
                        "Classic (317 software renderer)",
                    );
                    ui.separator();
                    ui.add_enabled_ui(*st.render_mode == RenderMode::Modern, |ui| {
                        ui.label("Lighting");
                        ui.radio_value(&mut *st.lighting, LightingMode::Client, "317 lighting");
                        ui.radio_value(&mut *st.lighting, LightingMode::Real, "Engine lighting");
                        ui.checkbox(&mut st.overlays.labels, "Colour by vertex label");
                    });
                    ui.add_enabled_ui(*st.render_mode == RenderMode::Classic, |ui| {
                        ui.label("Classic pixel size");
                        ui.add(egui::Slider::new(&mut st.classic.scale, 1..=4).suffix("×"));
                    });
                });
                ui.menu_button("Open…", |ui| {
                    if ui.button("Cache directory").clicked() {
                        st.sources.pick(Kind::Cache);
                    }
                    if ui.button("Dump directory").clicked() {
                        st.sources.pick(Kind::DumpDir);
                    }
                    if ui.button("Pack").clicked() {
                        st.sources.pick(Kind::Pack);
                    }
                });
                if st.sources.pending() > 0 {
                    ui.spinner();
                    ui.label("opening…");
                }
            });
        })
        .response
        .rect
        .height();
    let mut sel = st.selected.clone();
    let left = egui::Panel::left("tree")
        .resizable(true)
        .default_size(280.0)
        .size_range(160.0..=420.0)
        .show(&mut root, |ui| {
            egui::ScrollArea::vertical().id_salt("tree").show(ui, |ui| {
                tree_ui(ui, &st.sources, &mut st.lists, &mut sel);
                for e in &st.sources.errors {
                    ui.colored_label(egui::Color32::LIGHT_RED, e);
                }
            });
            ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
        })
        .response
        .rect
        .width();
    if sel != *st.selected {
        *st.selected = sel;
    }
    let right = egui::Panel::right("inspector")
        .resizable(true)
        .default_size(340.0)
        .size_range(200.0..=480.0)
        .show(&mut root, |ui| {
            inspector_ui(ui, &st.loaded);
            ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
        })
        .response
        .rect
        .width();
    let bottom = egui::Panel::bottom("jobs")
        .resizable(true)
        .default_size(80.0)
        .show(&mut root, |ui| {
            for job in st.jobs.list.iter().rev().take(4) {
                ui.horizontal(|ui| {
                    ui.label(&job.name);
                    match &job.done {
                        None => {
                            ui.add(egui::ProgressBar::new(job.fraction()).desired_width(200.0));
                        }
                        Some(Ok(msg)) => {
                            ui.label(msg);
                        }
                        Some(Err(e)) => {
                            ui.colored_label(egui::Color32::LIGHT_RED, e);
                        }
                    }
                });
            }
            ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
        })
        .response
        .rect
        .height();
    let scale = window.scale_factor();
    let pos = UVec2::new((left * scale) as u32, (top * scale) as u32);
    let full = UVec2::new(window.physical_width(), window.physical_height());
    let size = full
        .saturating_sub(pos)
        .saturating_sub(UVec2::new((right * scale) as u32, (bottom * scale) as u32));
    let new = ViewportRect {
        position: pos,
        size: size.max(UVec2::ONE),
    };
    if *st.rect != new {
        *st.rect = new;
    }
    Ok(())
}

pub fn plugin(app: &mut App) {
    app.init_resource::<ViewportRect>()
        .add_systems(EguiPrimaryContextPass, layout.in_set(Layout));
}
