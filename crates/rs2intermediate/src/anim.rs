//! Animation frame packs from index 2.

use serde::{Deserialize, Serialize};

/// Transform types in a frame base.
pub mod kind {
    /// Sets the pivot to the centroid of the labelled vertices plus the
    /// frame's offset.
    pub const ORIGIN: u8 = 0;
    pub const TRANSLATE: u8 = 1;
    /// Rotation about the pivot. Each component is an 8-bit angle in
    /// 1/256-turn units (`(v & 0xff) * 8` into a 2048-entry sine table),
    /// applied roll (z), then pitch (x), then yaw (y).
    pub const ROTATE: u8 = 2;
    /// Scale about the pivot in 1/128 units.
    pub const SCALE: u8 = 3;
    /// Adds `dx * 8` to the alpha of the labelled faces.
    pub const ALPHA: u8 = 5;
}

/// One transform slot of a frame base: what it does and which labels it
/// moves.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseTransform {
    pub kind: u8,
    pub labels: Vec<u8>,
}

/// One transform applied by a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameTransform {
    /// Index into the pack's base.
    pub transform: u8,
    pub dx: i32,
    pub dy: i32,
    pub dz: i32,
}

/// One frame: the transforms to apply, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    /// Global frame id, as referenced by `seq.dat`.
    pub id: u16,
    /// Default length in client cycles, used when the sequence gives 0.
    pub delay: u8,
    /// Expanded as the client does: an origin slot is inserted before a
    /// non-origin transform when the frame skips it, and unset scale
    /// components are 128.
    pub transforms: Vec<FrameTransform>,
    /// Whether any transform is an alpha transform.
    pub alpha: bool,
}

/// One index-2 file: a frame base shared by every frame in it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FramePack {
    pub base: Vec<BaseTransform>,
    pub frames: Vec<Frame>,
}
