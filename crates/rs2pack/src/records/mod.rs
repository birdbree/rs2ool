//! Record encoders and decoders, one module per pack kind. Layouts are in
//! `PLAN.md` § Phase C1; every record round-trips through `encode`/`decode`.

pub mod anim;
pub mod material;
pub mod model;
pub mod rig;
pub mod texture;

use thiserror::Error;

pub use crate::bytes::Truncated;

#[derive(Debug, Error)]
pub enum RecordError {
    #[error(transparent)]
    Truncated(#[from] Truncated),
    #[error("bad magic: expected {expected:?}")]
    Magic { expected: &'static [u8; 4] },
    #[error("unsupported record version {0}")]
    Version(u8),
    #[error("invalid record: {0}")]
    Invalid(String),
    #[error("{0} bytes left over after the record")]
    Trailing(usize),
}

pub(crate) fn magic(r: &mut crate::bytes::Reader, m: &'static [u8; 4]) -> Result<(), RecordError> {
    if r.bytes(4)? != m {
        return Err(RecordError::Magic { expected: m });
    }
    let v = r.u8()?;
    if v != 1 {
        return Err(RecordError::Version(v));
    }
    Ok(())
}

pub(crate) fn finish(r: &crate::bytes::Reader) -> Result<(), RecordError> {
    match r.remaining() {
        0 => Ok(()),
        n => Err(RecordError::Trailing(n)),
    }
}
