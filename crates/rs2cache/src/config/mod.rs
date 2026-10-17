//! Config archive decoders.
//!
//! `loc`, `npc` and `obj` are indexed: the `.idx` entry is `count u16` then a
//! `length u16` per definition, and definition `n` starts in the `.dat` at
//! 2 plus the lengths before it. The other types are sequential: the `.dat`
//! is `count u16` then the definitions back to back. Each definition is an
//! opcode stream ending with opcode 0.
//!
//! An opcode a decoder does not know is logged at `warn`, recorded in the
//! definition's `unknown_opcodes`, and skipped without consuming data, which
//! is what the 317 client does.

pub mod flo;
pub mod idk;
pub mod loc;
pub mod npc;
pub mod obj;
pub mod seq;
pub mod spotanim;

use rs2intermediate::config::{Morph, Recolour};
use thiserror::Error;

use crate::archive::{self, Archive};
use crate::io::{Cursor, IoError};

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("config archive has no {0}")]
    MissingEntry(&'static str),
    #[error("{kind} index: {source}")]
    Index { kind: &'static str, source: IoError },
    #[error("{kind} {id}: {source}")]
    Definition {
        kind: &'static str,
        id: u32,
        source: IoError,
    },
    #[error("{kind}: the .dat holds {dat} definitions but the .idx lists {idx}")]
    CountMismatch {
        kind: &'static str,
        dat: u16,
        idx: u16,
    },
    #[error("{kind} has {count} definitions; there is no {id}")]
    NoSuchDefinition {
        kind: &'static str,
        id: u32,
        count: usize,
    },
}

/// One config record type.
pub trait ConfigType {
    type Def;
    /// Name used in errors and on the command line (`"loc"`).
    const KIND: &'static str;
    const DAT: &'static str;
    /// The `.idx` entry, for indexed types.
    const IDX: Option<&'static str>;
    /// Decode one definition from `c`, which is positioned at its first
    /// opcode. Reads up to and including the terminating opcode 0.
    fn decode(id: u32, c: &mut Cursor) -> Result<Self::Def, IoError>;
}

fn entry<'a>(config: &'a Archive, name: &'static str) -> Result<&'a [u8], ConfigError> {
    config.get(name).ok_or(ConfigError::MissingEntry(name))
}

/// Start offsets in the `.dat` of every definition of an indexed type.
pub fn offsets<T: ConfigType>(
    config: &Archive,
    idx: &'static str,
) -> Result<Vec<usize>, ConfigError> {
    let index_err = |source| ConfigError::Index {
        kind: T::KIND,
        source,
    };
    let mut c = Cursor::new(entry(config, idx)?);
    let count = c.u16().map_err(index_err)?;
    let mut at = 2usize;
    let mut out = Vec::with_capacity(count.into());
    for _ in 0..count {
        out.push(at);
        at += usize::from(c.u16().map_err(index_err)?);
    }
    let dat_count = Cursor::new(entry(config, T::DAT)?)
        .u16()
        .map_err(index_err)?;
    if dat_count != count {
        return Err(ConfigError::CountMismatch {
            kind: T::KIND,
            dat: dat_count,
            idx: count,
        });
    }
    Ok(out)
}

fn definition<T: ConfigType>(id: u32, c: &mut Cursor) -> Result<T::Def, ConfigError> {
    T::decode(id, c).map_err(|source| ConfigError::Definition {
        kind: T::KIND,
        id,
        source,
    })
}

/// Decode every definition of type `T` from the config archive.
pub fn decode_all<T: ConfigType>(config: &Archive) -> Result<Vec<T::Def>, ConfigError> {
    let dat = entry(config, T::DAT)?;
    match T::IDX {
        Some(idx) => offsets::<T>(config, idx)?
            .into_iter()
            .enumerate()
            .map(|(id, at)| {
                let id = id as u32;
                let mut c = Cursor::at(dat, at).map_err(|source| ConfigError::Definition {
                    kind: T::KIND,
                    id,
                    source,
                })?;
                definition::<T>(id, &mut c)
            })
            .collect(),
        None => {
            let mut c = Cursor::new(dat);
            let count = c.u16().map_err(|source| ConfigError::Index {
                kind: T::KIND,
                source,
            })?;
            (0..u32::from(count))
                .map(|id| definition::<T>(id, &mut c))
                .collect()
        }
    }
}

/// Decode one definition. Sequential types decode every definition before
/// it, since nothing records where each one starts.
pub fn decode_one<T: ConfigType>(config: &Archive, id: u32) -> Result<T::Def, ConfigError> {
    let dat = entry(config, T::DAT)?;
    let missing = |count| ConfigError::NoSuchDefinition {
        kind: T::KIND,
        id,
        count,
    };
    match T::IDX {
        Some(idx) => {
            let offsets = offsets::<T>(config, idx)?;
            let &at = offsets.get(id as usize).ok_or(missing(offsets.len()))?;
            let mut c = Cursor::at(dat, at).map_err(|source| ConfigError::Definition {
                kind: T::KIND,
                id,
                source,
            })?;
            definition::<T>(id, &mut c)
        }
        None => {
            let mut all = decode_all::<T>(config)?;
            let count = all.len();
            if (id as usize) < count {
                Ok(all.swap_remove(id as usize))
            } else {
                Err(missing(count))
            }
        }
    }
}

/// Number of definitions of type `T`.
pub fn count<T: ConfigType>(config: &Archive) -> Result<usize, ConfigError> {
    let name = T::IDX.unwrap_or(T::DAT);
    Cursor::new(entry(config, name)?)
        .u16()
        .map(usize::from)
        .map_err(|source| ConfigError::Index {
            kind: T::KIND,
            source,
        })
}

/// Run an opcode loop: read an opcode, stop at 0, otherwise hand it to
/// `op`, which returns `Ok(false)` for an opcode it does not know. Returns
/// the unknown opcodes in the order met.
pub(crate) fn opcodes(
    kind: &'static str,
    id: u32,
    c: &mut Cursor,
    mut op: impl FnMut(u8, &mut Cursor) -> Result<bool, IoError>,
) -> Result<Vec<u8>, IoError> {
    let mut unknown = Vec::new();
    loop {
        let code = c.u8()?;
        if code == 0 {
            return Ok(unknown);
        }
        if !op(code, c)? {
            tracing::warn!(
                kind,
                id,
                opcode = code,
                offset = c.pos() - 1,
                "unknown opcode"
            );
            unknown.push(code);
        }
    }
}

/// `count u8` then `{from u16, to u16}` pairs.
pub(crate) fn recolours(c: &mut Cursor) -> Result<Vec<Recolour>, IoError> {
    let n = c.u8()?;
    (0..n)
        .map(|_| {
            Ok(Recolour {
                from: c.u16()?,
                to: c.u16()?,
            })
        })
        .collect()
}

/// Pair parallel `from`/`to` arrays up to the first zero `from`, as the
/// client applies the fixed-size recolour arrays of idk and spotanim.
pub(crate) fn paired_recolours(from: &[u16], to: &[u16]) -> Vec<Recolour> {
    from.iter()
        .zip(to)
        .take_while(|(f, _)| **f != 0)
        .map(|(&from, &to)| Recolour { from, to })
        .collect()
}

/// A `u16` where 65535 means none.
pub(crate) fn opt_u16(c: &mut Cursor) -> Result<Option<u16>, IoError> {
    Ok(Some(c.u16()?).filter(|&v| v != u16::MAX))
}

/// An option string; `"hidden"` (any case) means none.
pub(crate) fn action(c: &mut Cursor) -> Result<Option<String>, IoError> {
    let s = c.string()?;
    Ok((!s.eq_ignore_ascii_case("hidden")).then_some(s))
}

/// `varbit u16, varp u16, count u8, child u16 × (count + 1)`.
pub(crate) fn morph(c: &mut Cursor) -> Result<Morph, IoError> {
    let varbit = opt_u16(c)?;
    let varp = opt_u16(c)?;
    let n = c.u8()?;
    let children = (0..=n).map(|_| opt_u16(c)).collect::<Result<_, _>>()?;
    Ok(Morph {
        varbit,
        varp,
        children,
    })
}

/// The config archive number, re-exported for callers.
pub const ARCHIVE: u8 = archive::CONFIG;

#[cfg(test)]
pub(crate) mod synth {
    //! Write opcode streams for decoder tests.

    #[derive(Default)]
    pub struct Ops(pub Vec<u8>);

    impl Ops {
        pub fn op(mut self, code: u8) -> Self {
            self.0.push(code);
            self
        }
        pub fn u8(mut self, v: u8) -> Self {
            self.0.push(v);
            self
        }
        pub fn u16(mut self, v: u16) -> Self {
            self.0.extend_from_slice(&v.to_be_bytes());
            self
        }
        pub fn u24(mut self, v: u32) -> Self {
            self.0.extend_from_slice(&v.to_be_bytes()[1..]);
            self
        }
        pub fn i32(mut self, v: i32) -> Self {
            self.0.extend_from_slice(&v.to_be_bytes());
            self
        }
        pub fn str(mut self, s: &str) -> Self {
            self.0.extend_from_slice(s.as_bytes());
            self.0.push(b'\n');
            self
        }
        pub fn end(mut self) -> Vec<u8> {
            self.0.push(0);
            self.0
        }
    }
}
