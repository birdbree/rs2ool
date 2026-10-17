//! Animation frame packs from index 2.
//!
//! A pack is a frame base (transform slots, each with the labels it moves)
//! plus the frames that use it. The labels refer to a model's vertex labels,
//! or face labels for alpha slots.

use serde::{Deserialize, Serialize};

/// Transform kinds a frame base slot can have. Other values have no name.
pub mod kind {
    /// Sets the pivot to the centroid of the labelled vertices plus the
    /// frame's offset.
    pub const ORIGIN: u8 = 0;
    /// Moves the labelled vertices by `(dx, dy, dz)`.
    pub const TRANSLATE: u8 = 1;
    /// Rotation about the pivot. Each component is an 8-bit angle in
    /// 1/256-turn units (`(v & 0xff) * 8` into a 2048-entry sine table),
    /// applied roll (z), then pitch (x), then yaw (y).
    pub const ROTATE: u8 = 2;
    /// Scale about the pivot in 1/128 units.
    pub const SCALE: u8 = 3;
    /// Adds `dx * 8` to the alpha of the labelled faces, clamped to 0–255.
    pub const ALPHA: u8 = 5;
}

/// One transform slot of a frame base: what it does and which labels it
/// moves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseTransform {
    /// One of the [`kind`] constants.
    pub kind: u8,
    /// Vertex labels it acts on (face labels for [`kind::ALPHA`]).
    pub labels: Vec<u8>,
}

/// One transform applied by a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameTransform {
    /// Index into the pack's base.
    pub transform: u8,
    /// X component: an offset, angle, scale or alpha step, depending on
    /// the slot's kind. 0 when the frame leaves it out (128 for scale).
    pub dx: i32,
    /// Y component; same rules as `dx`.
    pub dy: i32,
    /// Z component; same rules as `dx`.
    pub dz: i32,
}

/// One frame: the transforms to apply, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    /// Global frame id, as `seq.dat` refers to it. Not a file id.
    pub id: u16,
    /// Default length in client cycles (20 ms each), used when the sequence
    /// gives 0.
    pub delay: u8,
    /// Expanded as the original client does: when a non-origin transform
    /// skips over an origin slot, that origin is inserted first with a zero
    /// offset, and unset scale components are 128. So this is exactly what
    /// gets applied, in order.
    pub transforms: Vec<FrameTransform>,
    /// Whether any transform is an alpha transform.
    pub alpha: bool,
}

/// One index-2 file: a frame base shared by every frame in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FramePack {
    /// The transform slots, indexed by [`FrameTransform::transform`].
    pub base: Vec<BaseTransform>,
    /// Frames in stored order.
    pub frames: Vec<Frame>,
}
