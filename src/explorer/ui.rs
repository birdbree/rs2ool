//! The egui layout: menu bar, source tree (left), inspector (right), and
//! object states, playback and jobs (bottom).
//!
//! Whatever's left in the middle is published as [`ViewportRect`] for the
//! cameras and painters. This module also keeps the Back history and the
//! "reveal this record in the tree" request that inspector links make.

use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};
use rs2cache::types::record::RecordType;

use crate::explorer::actions::{Actions, Settings, actions_menu};
use crate::explorer::classic::{ClassicView, RenderMode};
use crate::explorer::inspector;
use crate::explorer::jobs::Jobs;
use crate::explorer::kit::{Kit, kit_ui};
use crate::explorer::playback::{Playback, apply_state, playback_ui, states_ui};
use crate::explorer::selection::{CompareSelected, Loaded, Sel, Selected};
use crate::explorer::sources::{Kind, Source, Sources};
use crate::explorer::tree::{self, ListKey, Lists};
use crate::explorer::viewport3d::{Compare, LightingMode, Overlays, VertexColours};

/// The layout system, for ordering painters that draw over the viewport.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct Layout;

/// The central area left for the viewports, in physical pixels.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct ViewportRect {
    /// Top-left corner, from the window's top-left.
    pub position: UVec2,
    /// Width and height, at least 1 × 1.
    pub size: UVec2,
}

/// Height of one row in the tree lists and the hex view, in logical
/// pixels; virtual scrolling depends on it being fixed.
const ROW_HEIGHT: f32 = 18.0;

/// Selection history and a pending "reveal this record in the tree".
#[derive(Resource, Default)]
pub struct Nav {
    /// Earlier selections, newest last; **◀ Back** pops one.
    pub back: Vec<Selected>,
    /// A cache record to open and scroll to in the tree next frame, as
    /// `(source, type, id)`.
    pub reveal: Option<(usize, RecordType, u32)>,
}

/// One list in the tree: the search box, the "N of M" count, and the
/// rows. Clicking a row writes `selected`. If `reveal` points into this
/// list, it clears the search, scrolls to the row and uses the request up.
fn list_ui(
    ui: &mut egui::Ui,
    key: ListKey,
    source: &Source,
    si: usize,
    lists: &mut Lists,
    selected: &mut Selected,
    reveal: &mut Option<(usize, RecordType, u32)>,
) {
    lists.request(key, source);
    // The id to scroll to, if the reveal request is for this list.
    let target = match (key, *reveal) {
        (ListKey::Type(ksi, kt), Some((rsi, rt, id))) if ksi == rsi && kt == rt => Some(id),
        _ => None,
    };
    if target.is_some() {
        lists.search.insert(key, String::new());
    }
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
            let mut area = egui::ScrollArea::vertical()
                .id_salt(format!("{key:?}"))
                .max_height(240.0)
                .auto_shrink([false, true]);
            if let Some(id) = target {
                if let Some(i) = shown.iter().position(|r| r.id == id) {
                    // Land the row a little below the top, not flush with it.
                    area = area.vertical_scroll_offset((i as f32 * ROW_HEIGHT - 100.0).max(0.0));
                }
                *reveal = None;
            }
            // Only the visible rows get laid out, so long lists stay cheap.
            area.show_rows(ui, ROW_HEIGHT, shown.len(), |ui, range| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                for r in &shown[range] {
                    let sel = match key {
                        ListKey::Type(_, t) => Sel::Record(t, r.id),
                        ListKey::Pack(_) => Sel::PackRecord(r.id),
                        // A dump row's name is its relative path.
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

/// The whole source tree, one collapsible node per source.
fn tree_ui(
    ui: &mut egui::Ui,
    sources: &Sources,
    lists: &mut Lists,
    selected: &mut Selected,
    reveal: &mut Option<(usize, RecordType, u32)>,
) {
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
                    let kit = Selected {
                        source: Some(si),
                        sel: Some(Sel::PlayerKit),
                    };
                    if ui
                        .selectable_label(*selected == kit, "player kit")
                        .clicked()
                    {
                        *selected = kit;
                    }
                    for &t in RecordType::ALL {
                        // Force the type's node open when a reveal targets it.
                        let open = matches!(*reveal, Some((rsi, rt, _)) if rsi == si && rt == t);
                        egui::CollapsingHeader::new(t.name())
                            .id_salt((si, t.name()))
                            .open(open.then_some(true))
                            .show(ui, |ui| {
                                list_ui(ui, ListKey::Type(si, t), s, si, lists, selected, reveal);
                            });
                    }
                }
                Source::Pack { .. } => {
                    list_ui(ui, ListKey::Pack(si), s, si, lists, selected, reveal)
                }
                Source::DumpDir { .. } => {
                    list_ui(ui, ListKey::Dump(si), s, si, lists, selected, reveal)
                }
            });
    }
}

/// Key/value rows: keys in a fixed-width column, values wrapping within the
/// panel. (`egui::Grid` gives columns unbounded width, so values in it
/// never wrap.)
///
/// Values that refer to other records (per [`inspector::link_type`]) show
/// each id as a link; the clicked one is returned.
fn grid(
    ui: &mut egui::Ui,
    id: &str,
    rows: &[(String, String)],
    of: Option<RecordType>,
) -> Option<(RecordType, u32)> {
    let key_width = (ui.available_width() * 0.4).min(190.0);
    let mut clicked = None;
    ui.push_id(id, |ui| {
        for (k, v) in rows {
            ui.horizontal_top(|ui| {
                ui.add_sized(
                    [key_width, 0.0],
                    egui::Label::new(egui::RichText::new(k).monospace()).truncate(),
                );
                let target = of.and_then(|t| inspector::link_type(t, k));
                let ids = target.map(|_| inspector::link_ids(v)).unwrap_or_default();
                match target {
                    Some(t) if !ids.is_empty() => {
                        ui.horizontal_wrapped(|ui| {
                            for id in ids {
                                if ui
                                    .link(id.to_string())
                                    .on_hover_text(format!("open {t} {id}"))
                                    .clicked()
                                {
                                    clicked = Some((t, id));
                                }
                            }
                            if v.contains('…') {
                                ui.label("…");
                            }
                        });
                    }
                    _ => {
                        ui.add(egui::Label::new(v).wrap());
                    }
                }
            });
        }
    });
    clicked
}

/// A short label for a selection.
fn sel_label(sources: &Sources, s: &Selected) -> String {
    let src = s
        .source
        .and_then(|i| sources.list.get(i))
        .map_or_else(String::new, |s| s.label());
    match &s.sel {
        None => "nothing".into(),
        Some(Sel::Record(t, id)) => format!("{t} {id} ({src})"),
        Some(Sel::PackRecord(id)) => format!("record {id:#x} ({src})"),
        Some(Sel::DumpFile(p)) => format!("{} ({src})", p.display()),
        Some(Sel::PlayerKit) => format!("player kit ({src})"),
    }
}

/// Compare mode's controls: the toggle, and what the right pane shows
/// (the selection's other version, original or converted).
fn compare_ui(
    ui: &mut egui::Ui,
    compare: &mut Compare,
    right: &CompareSelected,
    sources: &Sources,
    loaded: &Loaded<CompareSelected>,
) {
    ui.checkbox(&mut compare.enabled, "Compare original and converted")
        .on_hover_text(
            "show the selected model's other version beside it: a cache model or NPC \
             and its record in an open pack, either way round",
        );
    if !compare.enabled {
        return;
    }
    if right.0.sel.is_none() {
        ui.weak("No other version open: open the converted pack (or the cache) to compare.");
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.label("Right:");
        ui.add(egui::Label::new(sel_label(sources, &right.0)).truncate());
        if loaded.is_loading() {
            ui.spinner();
        }
    });
    if let Some(e) = &loaded.error {
        ui.colored_label(egui::Color32::LIGHT_RED, e);
    }
}

/// The inspector: Back, then the metadata, fields and bytes sections.
/// Sets `back` when Back is clicked, and returns a clicked link.
fn inspector_ui(
    ui: &mut egui::Ui,
    loaded: &Loaded,
    nav: &Nav,
    back: &mut bool,
) -> Option<(RecordType, u32)> {
    let mut clicked = None;
    ui.horizontal(|ui| {
        ui.heading("Inspector");
        if ui
            .add_enabled(!nav.back.is_empty(), egui::Button::new("◀ Back"))
            .clicked()
        {
            *back = true;
        }
    });
    if loaded.is_loading() {
        ui.spinner();
    }
    if let Some(e) = &loaded.error {
        ui.colored_label(egui::Color32::LIGHT_RED, e);
    }
    let Some(data) = &loaded.data else {
        return None;
    };
    let (meta, fields) = inspector::sections(data);
    let of = match &**data {
        crate::explorer::selection::LoadedData::Record { record, .. } => Some(record.record_type()),
        _ => None,
    };
    egui::ScrollArea::vertical()
        .auto_shrink([false, true])
        .show(ui, |ui| {
            egui::CollapsingHeader::new("metadata")
                .default_open(true)
                .show(ui, |ui| grid(ui, "meta", &meta, None));
            if !fields.is_empty() {
                egui::CollapsingHeader::new("fields")
                    .default_open(true)
                    .show(ui, |ui| {
                        if let Some(c) = grid(ui, "fields", &fields, of) {
                            clicked = Some(c);
                        }
                    });
            }
            if let Some(bytes) = inspector::raw_bytes(data) {
                egui::CollapsingHeader::new(format!("bytes ({})", bytes.len())).show(ui, |ui| {
                    // 16 bytes a line, the last one maybe short.
                    let lines = bytes.len().div_ceil(16);
                    egui::ScrollArea::vertical()
                        .id_salt("hex")
                        .max_height(300.0)
                        .auto_shrink([false, true])
                        .show_rows(ui, ROW_HEIGHT, lines, |ui, range| {
                            ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                            for row in range {
                                ui.monospace(inspector::hex_line(bytes, row));
                            }
                        });
                });
            }
        });
    clicked
}

/// The resources the layout reads and writes.
#[derive(SystemParam)]
struct UiState<'w> {
    sources: ResMut<'w, Sources>,
    lists: ResMut<'w, Lists>,
    selected: ResMut<'w, Selected>,
    loaded: ResMut<'w, Loaded>,
    jobs: ResMut<'w, Jobs>,
    rect: ResMut<'w, ViewportRect>,
    render_mode: ResMut<'w, RenderMode>,
    lighting: ResMut<'w, LightingMode>,
    overlays: ResMut<'w, Overlays>,
    classic: ResMut<'w, ClassicView>,
    nav: ResMut<'w, Nav>,
    world: ResMut<'w, crate::explorer::map_view::WorldMap>,
    compare: ResMut<'w, Compare>,
    compare_sel: ResMut<'w, CompareSelected>,
    compare_loaded: ResMut<'w, Loaded<CompareSelected>>,
    playback: ResMut<'w, Playback>,
    kit: ResMut<'w, Kit>,
    actions: ResMut<'w, Actions>,
    settings: Res<'w, Settings>,
}

/// Draws every panel, applies what was clicked, and publishes what's left
/// for the viewport.
fn layout(
    mut contexts: EguiContexts,
    mut st: UiState,
    window: Single<&Window, With<PrimaryWindow>>,
) -> Result {
    let ctx = contexts.ctx_mut()?;
    // A full-window background `Ui` the panels carve space out of.
    let mut root = egui::Ui::new(
        ctx.clone(),
        "root".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(ctx.viewport_rect()),
    );
    // Each panel hands back its thickness (logical pixels) for the
    // viewport maths at the end.
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
                        ui.label("Vertex colours");
                        let colours = &mut st.overlays.colours;
                        ui.radio_value(colours, VertexColours::Faces, "Face colours");
                        ui.radio_value(colours, VertexColours::Labels, "By vertex label");
                        ui.radio_value(colours, VertexColours::Bones, "By bone");
                        ui.checkbox(&mut st.overlays.bones, "Bones");
                    });
                    ui.separator();
                    ui.checkbox(&mut st.world.open, "World map");
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
                actions_menu(
                    ui,
                    &mut st.actions,
                    &st.settings,
                    &st.selected,
                    &st.sources,
                    &mut st.jobs,
                );
                if st.sources.pending() > 0 {
                    ui.spinner();
                    ui.label("opening…");
                }
            });
        })
        .response
        .rect
        .height();
    // Work on copies so a click only counts as a change (and goes in the
    // Back history) when it really differs.
    let mut sel = st.selected.clone();
    let mut reveal = st.nav.reveal.take();
    let left = egui::Panel::left("tree")
        .resizable(true)
        .default_size(280.0)
        .size_range(160.0..=420.0)
        .show(&mut root, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("tree")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    tree_ui(ui, &st.sources, &mut st.lists, &mut sel, &mut reveal);
                    for e in &st.sources.errors {
                        ui.colored_label(egui::Color32::LIGHT_RED, e);
                    }
                });
            ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
        })
        .response
        .rect
        .width();
    st.nav.reveal = reveal;
    if sel != *st.selected {
        let old = std::mem::replace(&mut *st.selected, sel);
        if old.sel.is_some() {
            st.nav.back.push(old);
        }
    }
    let (mut back, mut clicked) = (false, None);
    let right = egui::Panel::right("inspector")
        .resizable(true)
        .default_size(340.0)
        .size_range(200.0..=480.0)
        .show(&mut root, |ui| {
            // Same trick: only write `Compare` back when the box was toggled.
            let mut c = *st.compare;
            compare_ui(ui, &mut c, &st.compare_sel, &st.sources, &st.compare_loaded);
            kit_ui(ui, &mut st.kit);
            if c != *st.compare {
                *st.compare = c;
            }
            ui.separator();
            clicked = inspector_ui(ui, &st.loaded, &st.nav, &mut back);
            ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
        })
        .response
        .rect
        .width();
    // Back wins over a link click in the same frame. Either way the
    // record is revealed in the tree.
    if back && let Some(prev) = st.nav.back.pop() {
        if let (Some(si), Some(Sel::Record(t, id))) = (prev.source, &prev.sel) {
            st.nav.reveal = Some((si, *t, *id));
        }
        *st.selected = prev;
    } else if let (Some((t, id)), Some(si)) = (clicked, st.selected.source) {
        let old = std::mem::replace(
            &mut *st.selected,
            Selected {
                source: Some(si),
                sel: Some(Sel::Record(t, id)),
            },
        );
        st.nav.back.push(old);
        st.nav.reveal = Some((si, t, id));
    }
    let mut open = Vec::new();
    let bottom = egui::Panel::bottom("jobs")
        .resizable(true)
        .default_size(80.0)
        .show(&mut root, |ui| {
            // Only write the loaded selections when a state is chosen, so
            // the viewports are not rebuilt every frame.
            if let Some(i) = states_ui(ui, &st.loaded) {
                apply_state(&mut st.loaded, i);
                if st.compare.enabled {
                    apply_state(&mut st.compare_loaded, i);
                }
            }
            let right = st
                .compare_loaded
                .renderable
                .as_deref()
                .filter(|_| st.compare.enabled);
            playback_ui(ui, &mut st.playback, st.loaded.renderable.as_deref(), right);
            for job in st.jobs.list.iter().rev().take(4) {
                ui.horizontal(|ui| {
                    ui.label(&job.name);
                    match &job.done {
                        None => {
                            ui.add(egui::ProgressBar::new(job.fraction()).desired_width(200.0));
                        }
                        Some(Ok(done)) => {
                            ui.label(&done.message);
                            for (kind, path) in &done.opens {
                                if ui
                                    .small_button(crate::explorer::jobs::open_label(*kind, path))
                                    .on_hover_text(path.display().to_string())
                                    .clicked()
                                {
                                    open.push((*kind, path.clone()));
                                }
                            }
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
    for (kind, path) in open {
        st.sources.open_path(kind, path);
    }
    // Panels are measured in logical pixels; the cameras want physical ones.
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
    // Only write on a real change so `ViewportRect` change detection means
    // something.
    if *st.rect != new {
        *st.rect = new;
    }
    Ok(())
}

/// Adds the layout system and its resources.
pub fn plugin(app: &mut App) {
    app.init_resource::<ViewportRect>()
        .init_resource::<Nav>()
        .add_systems(EguiPrimaryContextPass, layout.in_set(Layout));
}
