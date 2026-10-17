//! Plain `serde` structs the decoders produce and everything downstream
//! uses. No logic lives here beyond small accessors.
//!
//! - [`model`] — [`Model`](model::Model), from index 1.
//! - [`anim`] — [`FramePack`](anim::FramePack), from index 2.
//! - [`map`] — [`Region`](map::Region), from index 4.
//! - [`config`] — every config definition (`LocDef`, `NpcDef`, `SeqDef`, …).
//! - [`sprite`] — [`SpriteSheet`](sprite::SpriteSheet) and
//!   [`Font`](sprite::Font).
//! - [`interface`] — [`Interface`](interface::Interface) components.
//! - [`versionlist`] — [`VersionList`](versionlist::VersionList).
//! - [`record`] — [`RecordType`](record::RecordType), the
//!   [`Record`](record::Record) wrapper and the [`Meta`](record::Meta)
//!   sidecar.

pub mod anim;
pub mod config;
pub mod interface;
pub mod map;
pub mod model;
pub mod record;
pub mod sprite;
pub mod versionlist;
