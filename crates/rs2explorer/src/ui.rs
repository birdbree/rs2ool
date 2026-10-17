//! The egui layout: menu bar, source tree (left), inspector (right), jobs
//! and playback (bottom); the remaining central rect is published as
//! [`ViewportRect`] for the cameras.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass, egui};

use crate::jobs::Jobs;
use crate::sources::{Kind, Sources};
use crate::tree;

/// The central area left for the viewports, in physical pixels.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct ViewportRect {
    pub position: UVec2,
    pub size: UVec2,
}

fn layout(
    mut contexts: EguiContexts,
    mut sources: ResMut<Sources>,
    jobs: Res<Jobs>,
    mut rect: ResMut<ViewportRect>,
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
                ui.menu_button("Open…", |ui| {
                    if ui.button("Cache directory").clicked() {
                        sources.pick(Kind::Cache);
                    }
                    if ui.button("Dump directory").clicked() {
                        sources.pick(Kind::DumpDir);
                    }
                    if ui.button("Pack").clicked() {
                        sources.pick(Kind::Pack);
                    }
                });
                if sources.pending() > 0 {
                    ui.spinner();
                    ui.label("opening…");
                }
            });
        })
        .response
        .rect
        .height();
    let left = egui::Panel::left("tree")
        .resizable(true)
        .default_size(260.0)
        .show(&mut root, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for s in &sources.list {
                    egui::CollapsingHeader::new(s.label())
                        .default_open(true)
                        .show(ui, |ui| {
                            for row in tree::index_rows(s) {
                                ui.label(format!(
                                    "index {}: {} files ({} present)",
                                    row.index, row.files, row.present
                                ));
                            }
                        });
                }
                for e in &sources.errors {
                    ui.colored_label(egui::Color32::LIGHT_RED, e);
                }
            });
            ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
        })
        .response
        .rect
        .width();
    let right = egui::Panel::right("inspector")
        .resizable(true)
        .default_size(300.0)
        .show(&mut root, |ui| {
            ui.heading("Inspector");
            ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::hover());
        })
        .response
        .rect
        .width();
    let bottom = egui::Panel::bottom("jobs")
        .resizable(true)
        .default_size(80.0)
        .show(&mut root, |ui| {
            for job in jobs.list.iter().rev().take(4) {
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
    *rect = ViewportRect {
        position: pos,
        size: size.max(UVec2::ONE),
    };
    Ok(())
}

pub fn plugin(app: &mut App) {
    app.init_resource::<ViewportRect>()
        .add_systems(EguiPrimaryContextPass, layout);
}
