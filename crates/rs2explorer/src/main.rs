//! `rs2explorer`: browse, render, compare, export and convert a 317 cache.
//!
//! Every cache read and conversion runs on Bevy's `AsyncComputeTaskPool`
//! through [`jobs`] or [`sources`]; UI systems only poll finished tasks.

use bevy::prelude::*;
use bevy_egui::EguiPlugin;

use rs2explorer::{jobs, sources, ui, viewport3d};

fn main() {
    App::new()
        .insert_resource(ClearColor(Color::srgb(0.12, 0.12, 0.14)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: "rs2explorer".into(),
                resolution: (1400, 900).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(EguiPlugin::default())
        .add_plugins((
            ui::plugin,
            sources::plugin,
            viewport3d::plugin,
            jobs::plugin,
        ))
        .run();
}
