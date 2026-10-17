//! Record encoders and decoders, one module per pack kind.
//!
//! Every record round-trips: `decode(encode(x)) == x`, and the bytes come
//! back identical too. A record holds only its own fields, with no length
//! prefix, since the container's index already gives the length. Decoders
//! reject a record that ends early ([`RecordError::Truncated`]) and one with
//! bytes left over ([`RecordError::Trailing`]).
//!
//! Models, rigs and anims start with a 4-byte magic and a version byte
//! ([`VERSION`]); the other kinds have no header at all. Optional `u32`
//! references use `0xFFFFFFFF` for "none", and 8-bit bone and parent fields
//! use 255.
//!
//! | Module | Kinds | Records |
//! | --- | --- | --- |
//! | [`model`] | `models` | Triangle meshes in pack space |
//! | [`material`] | `materials` | Materials and the palette |
//! | [`texture`] | `textures` | Texture rectangles and atlas images |
//! | [`sprite`] | `sprites` | Sprites, font glyphs and font metrics |
//! | [`object`] | `objects`, `kits` | Placeable objects and model kits |
//! | [`rig`] | `rigs` | Bone hierarchies |
//! | [`anim`] | `anims` | Animation clips |
//! | [`strings`] | `strings` | String tables |
//!
//! Pack space is the cache's coordinates with y and z both negated. That's
//! half a turn about x, so nothing's mirrored and y points up.

pub mod anim;
pub mod material;
pub mod model;
pub mod object;
pub mod rig;
pub mod sprite;
pub mod strings;
pub mod texture;

use thiserror::Error;

pub use crate::bytes::Truncated;

/// Why a record wouldn't encode or decode. Shared by every record kind.
#[derive(Debug, Error)]
pub enum RecordError {
    /// A field runs past the end of the record.
    #[error(transparent)]
    Truncated(#[from] Truncated),
    /// The first four bytes aren't this kind's magic.
    #[error("bad magic: expected {expected:?}")]
    Magic {
        /// The magic the decoder wanted.
        expected: &'static [u8; 4],
    },
    /// The version byte isn't [`VERSION`].
    #[error("unsupported record version {0}")]
    Version(u8),
    /// The record breaks one of its kind's invariants (an index out of
    /// range, keys out of order, a count too big for its field and so on).
    #[error("invalid record: {0}")]
    Invalid(String),
    /// This many bytes are left over after the last field.
    #[error("{0} bytes left over after the record")]
    Trailing(usize),
}

/// Version byte of the model, rig and anim record headers.
///
/// Version 2 maps cache space to pack space by negating y and z. Version 1
/// negated y only, which mirrored every asset, so the decoders turn it away.
pub const VERSION: u8 = 2;

/// Read and check a header: the 4-byte magic, then the version byte.
pub(crate) fn magic(r: &mut crate::bytes::Reader, m: &'static [u8; 4]) -> Result<(), RecordError> {
    if r.bytes(4)? != m {
        return Err(RecordError::Magic { expected: m });
    }
    let v = r.u8()?;
    if v != VERSION {
        return Err(RecordError::Version(v));
    }
    Ok(())
}

/// Fail with [`RecordError::Trailing`] unless the reader's used every byte.
pub(crate) fn finish(r: &crate::bytes::Reader) -> Result<(), RecordError> {
    match r.remaining() {
        0 => Ok(()),
        n => Err(RecordError::Trailing(n)),
    }
}
