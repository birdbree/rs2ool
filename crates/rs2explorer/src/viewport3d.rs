//! The 3D viewport: a world camera whose viewport follows the central
//! rect, under a separate camera that draws only egui.

use bevy::camera::visibility::RenderLayers;
use bevy::camera::{CameraOutputMode, Viewport};
use bevy::prelude::*;
use bevy::render::render_resource::BlendState;
use bevy_egui::{EguiContext, EguiGlobalSettings, PrimaryEguiContext};

use crate::ui::ViewportRect;

/// Marks the world camera(s).
#[derive(Component)]
pub struct WorldCamera;

fn setup(mut commands: Commands, mut egui_settings: ResMut<EguiGlobalSettings>) {
    egui_settings.auto_create_primary_context = false;
    commands.spawn((
        WorldCamera,
        Camera3d::default(),
        Transform::from_xyz(0.0, 150.0, 500.0).looking_at(Vec3::new(0.0, 100.0, 0.0), Vec3::Y),
    ));
    commands.spawn((
        DirectionalLight::default(),
        Transform::from_xyz(300.0, 600.0, 400.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        PrimaryEguiContext,
        Camera2d,
        RenderLayers::none(),
        Camera {
            order: 10,
            output_mode: CameraOutputMode::Write {
                blend_state: Some(BlendState::ALPHA_BLENDING),
                clear_color: ClearColorConfig::None,
            },
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..default()
        },
    ));
}

fn follow_rect(
    rect: Res<ViewportRect>,
    mut cameras: Query<&mut Camera, (With<WorldCamera>, Without<EguiContext>)>,
) {
    if !rect.is_changed() || rect.size == UVec2::ZERO {
        return;
    }
    for mut cam in &mut cameras {
        cam.viewport = Some(Viewport {
            physical_position: rect.position,
            physical_size: rect.size,
            ..default()
        });
    }
}

pub fn plugin(app: &mut App) {
    app.add_systems(Startup, setup)
        .add_systems(Update, follow_rect);
}
