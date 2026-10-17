//! Conversion pipelines from `rs2intermediate` records to `rs2pack` records:
//! palette, models, creatures, rig inference, clips, validation, textures,
//! sprites, floors and objects.

pub mod clip;
pub mod creature;
pub mod deform;
pub mod model;
pub mod palette;
pub mod pipeline;
pub mod rig;
pub mod texture;
pub mod validate;

use rs2cache::{Cache, archive};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConvertError {
    #[error(transparent)]
    Cache(#[from] rs2cache::Error),
    #[error(transparent)]
    Record(#[from] rs2pack::records::RecordError),
    #[error("{0}")]
    Invalid(String),
}

/// Material ids. Faces reference materials through a `u8` slot, so model
/// materials live below 256; floors start at [`FLOOR_BASE`].
pub mod material_id {
    /// The palette-mode material every untextured face uses.
    pub const PALETTE: u16 = 0;
    /// Texture `t` is material `TEXTURE_BASE + t`.
    pub const TEXTURE_BASE: u16 = 1;
    /// Floor `f` is material `FLOOR_BASE + f`.
    pub const FLOOR_BASE: u16 = 1000;
}

/// The CRC32 identifying the source cache: that of its stored versionlist
/// archive, which changes whenever any versioned file does.
pub fn source_cache_crc(cache: &Cache) -> Result<u32, rs2cache::Error> {
    Ok(crc32fast::hash(
        &cache.read_raw(0, archive::VERSIONLIST.into())?,
    ))
}
