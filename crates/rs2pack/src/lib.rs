//! The low-poly client's content-pack container, its record encoders and
//! the palette tools. Byte layouts are specified in `PLAN.md` § Phase C1.
//! Everything is little-endian, and writing is deterministic: records are
//! sorted by id, the manifest is generated from ordered maps, and nothing
//! depends on the clock.

pub mod bytes;
pub mod pack;
pub mod palette;
pub mod records;

pub use pack::{Codec, Kind, PackError, PackManifest, PackReader, PackWriter};
