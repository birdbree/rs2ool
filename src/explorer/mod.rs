//! The explorer: browse, render, compare, export and convert a 317 cache
//! and its packs.
//!
//! [`app::run`] opens the window. Each module below hands Bevy a `plugin`
//! function, and they're all public so headless tests can put an app
//! together from just the pieces they need.
//!
//! Every cache read, decode and conversion runs on Bevy's
//! `AsyncComputeTaskPool`; UI systems only poll tasks and pick up what's
//! finished, so the window never waits on the cache.
//!
//! # Modules
//!
//! Getting things open and selected:
//!
//! - [`app`]: the window, the plugin list, and the startup options
//!   (`--select`, `--compare`, `--classic`).
//! - [`sources`]: the open caches, dump directories and packs.
//! - [`tree`]: the left-hand tree's lists and search.
//! - [`selection`]: what's selected (main and compare slots) and its
//!   decoded form.
//! - [`renderable`]: what a selection draws, and matching a record with
//!   its converted version for compare mode.
//!
//! Panels:
//!
//! - [`ui`]: the egui layout; it publishes the free middle as
//!   [`ui::ViewportRect`].
//! - [`inspector`]: metadata, fields, links and the hex view.
//! - [`actions`]: the Actions menu, which calls the same library code as
//!   the CLI.
//! - [`jobs`]: background jobs and their progress.
//!
//! Drawing:
//!
//! - [`viewport3d`]: cameras, orbit controls, compare panes and the
//!   modern renderer.
//! - [`mesh`]: Bevy meshes from cache and pack models, with the 317
//!   lighting.
//! - [`viewport2d`]: textures, sprites, fonts and interfaces.
//! - [`classic`]: the `rs2classic` software renderer view.
//! - [`map_view`]: map regions, the region overlay, and the world map.
//! - [`playback`]: the animation clock and posing.
//! - [`kit`]: the player kit and its equipment slots.

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
