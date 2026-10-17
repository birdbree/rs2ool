//! A software renderer that draws 317 models the way the original client
//! does, for when you want the authentic look.
//!
//! It reproduces the original client's observable behaviour: per-face and
//! per-vertex lighting into the 16-bit HSL colour table, the model-viewer
//! projection (`x · 512 / z` about the screen centre), back-face rejection,
//! depth buckets drawn far to near with the twelve-group priority
//! interleave, and the three triangle fillers (smooth-shaded in 4-pixel
//! colour runs, flat, and perspective-textured in 8-pixel spans with four
//! shade levels). All of it runs in the same fixed-point integer maths,
//! wrapping where the original's 32-bit `int` maths wraps. The code is
//! written from that behaviour; none of it is copied from the original client.
//!
//! Two things differ on purpose: a face with a vertex behind the near
//! plane is skipped rather than clipped, and a face whose depth bucket
//! lands outside the model's range isn't drawn.
//!
//! There's no window or GPU here: it draws into a plain `0xRRGGBB` buffer.
//!
//! # Modules
//!
//! - [`light`](mod@light): lights a model once into a [`LitModel`].
//! - [`render`](mod@render): projection, face sorting and drawing, with the
//!   model-viewer [`Camera`] or a world [`View`].
//! - [`raster`]: the three triangle fillers and alpha blending.
//! - [`texture`]: texture pages prepared from the texture sprites.
//! - [`tables`]: sine, cosine, reciprocal and HSL lookup tables.
//!
//! # Example
//!
//! Light a model, then draw it with an orbit camera:
//!
//! ```no_run
//! use rs2cache::types::model::Model;
//! use rs2classic::{Camera, Framebuffer, Textures, light, render};
//!
//! # fn draw(model: &Model) -> Vec<u8> {
//! let lit = light(model, &rs2classic::light::NPC);
//! let mut fb = Framebuffer::new(256, 256);
//! fb.clear(0x000000);
//! // Light once, then pass posed vertices here to animate.
//! render(&mut fb, &lit, &model.vertices, &Camera::orbit(0, 128, 800), &Textures::default());
//! fb.to_rgba()
//! # }
//! ```

#![warn(missing_docs)]

pub mod light;
pub mod raster;
pub mod render;
pub mod tables;
pub mod texture;

pub use light::{Lighting, LitModel, light};
pub use render::{Camera, Framebuffer, View, render, render_in_view};
pub use texture::Textures;
