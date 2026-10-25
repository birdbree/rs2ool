//! A sequence drawn as one image: a few frames, evenly spaced in time, in a
//! grid, with the classic renderer (see [`crate::render`]).
//!
//! Every tile shares one camera fitted to all the frames shown, so a model
//! that falls, shrinks or moves away does so on screen. Faces fade as the
//! original client fades them. The darker band at the bottom of each tile
//! is the ground where the model starts.

use image::RgbaImage;
use rs2classic::{Textures, light};
use rs2convert::creature::Creature;

use crate::render::{self, BORDER, View};

pub use crate::render::{png, sample, textures};

/// Width and height of one frame's tile, in pixels.
pub const TILE: usize = 256;

/// Sequence `s` of `c` (an index into its sequences) as a grid of up to
/// `n` frames, left to right and top to bottom. `None` when a frame won't
/// pose or the model is empty.
pub fn sheet(c: &Creature, s: usize, n: usize, textures: &Textures) -> Option<RgbaImage> {
    let shown = sample(&c.delays(s), n);
    let poses: Vec<Vec<[i32; 3]>> = shown
        .iter()
        .map(|&i| c.posed(s, i))
        .collect::<Option<_>>()?;
    let first = poses.first().filter(|p| !p.is_empty())?;
    let view = View {
        width: TILE,
        height: TILE,
        ..View::default()
    };
    let framing = render::frame(&view, &poses, Some(first))?;
    let lit = light(&c.rest_model(), &rs2classic::light::NPC);
    let tiles: Vec<RgbaImage> = poses
        .iter()
        .zip(shown)
        .map(|(pose, i)| {
            let faded = c.faded(s, i).map(|alphas| rs2classic::LitModel {
                alphas,
                ..lit.clone()
            });
            let lit = faded.as_ref().unwrap_or(&lit);
            let mut fb = render::draw(&view, &framing, lit, pose, textures);
            render::outline(&mut fb, BORDER);
            render::image(&fb)
        })
        .collect();
    Some(render::grid(&tiles, None))
}
