//! A reimplementation of the RS2 317 client's software model renderer, for
//! a maximally authentic model view.
//!
//! It follows the client's behaviour: per-face and per-vertex lighting
//! into the 16-bit HSL colour table, the model-viewer projection
//! (`x · 512 / z` about the screen centre), back-face rejection, depth
//! buckets drawn far to near with the twelve-group priority interleave,
//! and the three triangle fillers (smooth-shaded with 4-pixel colour
//! runs, flat, and perspective-textured in 8-pixel spans with four shade
//! levels), all in the client's fixed-point arithmetic. Nothing here is
//! copied from the client; it is written from the behaviour, with the
//! deobfuscated client as the reference.

pub mod light;
pub mod raster;
pub mod render;
pub mod tables;
pub mod texture;

pub use light::{Lighting, LitModel, light};
pub use render::{Camera, Framebuffer, render};
pub use texture::Textures;
