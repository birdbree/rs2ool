//! Turns decoded 317 cache records into `contentpack` records.
//!
//! You hand it [`rs2cache`] records and it gives you pack writers: a colour
//! palette, models, creatures with an inferred rig and animation clips
//! (checked against the original client's own deformation), textures,
//! sprites, floors and objects. Nothing here touches the disk except
//! [`run::write`].
//!
//! # Modules
//!
//! - [`run`]: one call per conversion command, plus [`run::write`] to
//!   save the result. Start here.
//! - [`pipeline`]: whole-creature conversion (model, rig, clips,
//!   validation) and the record-id namespaces in [`pipeline::ids`].
//! - [`palette`]: the 1024-entry palette quantised from every model face
//!   colour, and the materials pack that holds it.
//! - [`model`]: one cache model to a `models` record.
//! - [`creature`]: merging an NPC's (or kit part's) models into one mesh,
//!   and collecting its sequences and frame packs.
//! - [`deform`]: the original client's frame application, in integer maths.
//!   This is the reference everything else is checked against.
//! - [`rig`]: bone inference from frame-base label sets.
//! - [`clip`]: sequences to per-bone pose keys.
//! - [`validate`]: re-skinning the stored rig and clip, and measuring the
//!   error against [`deform`].
//! - [`infer`]: links the cache doesn't store, such as which sequences
//!   could animate an NPC, inferred from frame-base families.
//! - [`classify`]: a small decision tree for sequence roles, learned from
//!   labelled sequences.
//! - [`slot`]: an item's equipment slot, inferred from its wear models'
//!   vertex labels.
//! - [`texture`]: the texture atlas and texture materials.
//! - [`sprite`]: media and title sprites, and fonts.
//! - [`floor`]: floor materials and their names.
//! - [`morph`]: which child a morph table shows for a variable's value,
//!   as the original client picks it.
//! - [`object`]: location definitions to models, objects, wall kits and
//!   strings.
//! - [`item`]: item definitions to items records, their inventory and
//!   wear models, and strings, with inferred slots.
//! - [`structure`]: whole structures found in the map, with their rooms,
//!   doors, walls, roofs and contents.
//! - [`furnish`]: what a placed object is in general terms: its furnishing
//!   category, and what its minimap icon shows.
//! - [`purpose`]: what a structure and its rooms are likely for, with the
//!   evidence.
//! - [`learn`]: learning structure classes, room uses and sites from
//!   labelled structures.
//! - [`material`]: what a structure's walls, roofs and floors are made of.
//! - [`tree`]: a small decision tree over plain feature vectors, for any
//!   label, with cross-validation.
//! - [`preview`]: the model a record describes, put together for viewing.
//! - [`space`]: cache space to pack space (y and z negated).
//!
//! # Example
//!
//! Convert one NPC with its animations and write the packs:
//!
//! ```no_run
//! use rs2cache::{Cache, Records};
//! use rs2convert::{run, source_cache_crc};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let cache = Cache::open(std::env::var("RS2_CACHE")?)?;
//! let records = Records::new(&cache);
//! let crc = source_cache_crc(&cache)?;
//! let outcome = run::npc(&records, crc, 1, true, &[], None)?;
//! // No PNG encoder needed: an NPC conversion produces no images.
//! run::write(&outcome, "packs".as_ref(), |_, _, _, _| Ok(()))?;
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

pub mod classify;
pub mod clip;
pub mod creature;
pub mod deform;
pub mod floor;
pub mod furnish;
pub mod infer;
pub mod item;
pub mod learn;
pub mod material;
pub mod model;
pub mod morph;
pub mod object;
pub mod palette;
pub mod pipeline;
pub mod preview;
pub mod purpose;
pub mod rig;
pub mod run;
pub mod slot;
pub mod space;
pub mod sprite;
pub mod structure;
pub mod texture;
pub mod tree;
pub mod validate;

use rs2cache::{Cache, archive};
use thiserror::Error;

/// Anything that can go wrong in a conversion.
#[derive(Debug, Error)]
pub enum ConvertError {
    /// Reading or decoding a cache record failed.
    #[error(transparent)]
    Cache(#[from] rs2cache::Error),
    /// Encoding or decoding a pack record failed.
    #[error(transparent)]
    Record(#[from] contentpack::records::RecordError),
    /// The input can't be converted (a value out of range, a missing
    /// frame, a bad override, an I/O failure while writing). The message
    /// says which.
    #[error("{0}")]
    Invalid(String),
}

/// Material ids.
///
/// Faces point at materials through a `u8` slot, so model materials have to
/// stay below 256. Floors don't, and start at
/// [`FLOOR_BASE`](material_id::FLOOR_BASE).
pub mod material_id {
    /// The palette-mode material every untextured face uses.
    pub const PALETTE: u16 = 0;
    /// Texture `t` is material `TEXTURE_BASE + t`.
    pub const TEXTURE_BASE: u16 = 1;
    /// Floor `f` is material `FLOOR_BASE + f`.
    pub const FLOOR_BASE: u16 = 1000;
}

/// The CRC32 that identifies the source cache.
///
/// It's the CRC of the stored versionlist archive, which changes whenever
/// any versioned file does. Every pack records it.
pub fn source_cache_crc(cache: &Cache) -> Result<u32, rs2cache::Error> {
    Ok(crc32fast::hash(
        &cache.read_raw(0, archive::VERSIONLIST.into())?,
    ))
}
