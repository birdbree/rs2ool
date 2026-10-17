//! The explorer: browse, render, compare, export and convert a 317 cache
//! and its packs. Its plugins are public so headless tests can drive them;
//! [`app::run`] opens the window.
//!
//! Every cache read and conversion runs on Bevy's `AsyncComputeTaskPool`
//! through [`jobs`] or [`sources`]; UI systems only poll finished tasks.

pub mod actions;
pub mod app;
pub mod classic;
pub mod inspector;
pub mod jobs;
pub mod kit;
pub mod map_view;
pub mod mesh;
pub mod playback;
pub mod renderable;
pub mod selection;
pub mod sources;
pub mod tree;
pub mod ui;
pub mod viewport2d;
pub mod viewport3d;
