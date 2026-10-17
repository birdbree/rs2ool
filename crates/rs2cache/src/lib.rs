//! Reader for the RuneScape 2 revision-317 cache: sector chains, jag
//! archives, and every record decoder. Everything decoded is returned as an
//! `rs2intermediate` struct.

pub mod anim;
pub mod archive;
pub mod cache;
pub mod compress;
pub mod config;
pub mod io;
pub mod model;
pub mod sprite;

pub use archive::{Archive, ArchiveError, EntryName};
pub use cache::{Cache, CacheError, IndexEntry, SectorFault, SectorVisit};
pub use compress::CompressError;

use thiserror::Error;

/// Any error from reading and decoding a record.
#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    Cache(#[from] CacheError),
    #[error("file {index}/{id}: {source}")]
    Compress {
        index: u8,
        id: u32,
        source: CompressError,
    },
    #[error("archive {archive}: {source}")]
    Archive { archive: u8, source: ArchiveError },
    #[error("archive {archive} has no entry {entry}")]
    MissingEntry { archive: u8, entry: EntryName },
    #[error(transparent)]
    Config(#[from] config::ConfigError),
    #[error("model {id}: {source}")]
    Model { id: u32, source: model::ModelError },
    #[error("frame pack {id}: {source}")]
    Anim { id: u32, source: anim::FrameError },
}

impl Cache {
    /// Read and inflate a file from index 1–4 (gzip when the magic is
    /// present, otherwise as stored).
    pub fn read(&self, index: u8, id: u32) -> Result<Vec<u8>, Error> {
        let raw = self.read_raw(index, id)?;
        compress::inflate(&raw).map_err(|source| Error::Compress { index, id, source })
    }

    /// Read and decode model `id` from index 1.
    pub fn model(&self, id: u32) -> Result<rs2intermediate::model::Model, Error> {
        model::decode(&self.read(1, id)?).map_err(|source| Error::Model { id, source })
    }

    /// Read and decode frame pack `id` from index 2.
    pub fn frame_pack(&self, id: u32) -> Result<rs2intermediate::anim::FramePack, Error> {
        anim::decode(&self.read(2, id)?).map_err(|source| Error::Anim { id, source })
    }

    /// Build the frame id → pack map by reading every pack's id list.
    pub fn frame_index(&self) -> Result<anim::FrameIndex, Error> {
        let mut packs = Vec::new();
        for id in 0..self.file_count(2)? as u32 {
            match self.read(2, id) {
                Ok(data) => packs.push((id, data)),
                Err(Error::Cache(CacheError::Absent { .. })) => {}
                Err(e) => return Err(e),
            }
        }
        anim::FrameIndex::build(packs.iter().map(|(id, d)| (*id, d.as_slice())))
            .map_err(|(id, source)| Error::Anim { id, source })
    }

    /// Read and decode archive `archive` of index 0.
    pub fn archive(&self, archive: u8) -> Result<Archive, Error> {
        let raw = self.read_raw(0, archive.into())?;
        Archive::decode(&raw, Some(archive)).map_err(|source| Error::Archive { archive, source })
    }
}

impl Cache {
    /// Decode every definition of a config type.
    pub fn config<T: config::ConfigType>(&self) -> Result<Vec<T::Def>, Error> {
        Ok(config::decode_all::<T>(&self.archive(archive::CONFIG)?)?)
    }
}

impl Archive {
    /// An entry's bytes, or [`Error::MissingEntry`].
    pub fn require(&self, archive: u8, name: &str) -> Result<&[u8], Error> {
        self.get(name).ok_or_else(|| Error::MissingEntry {
            archive,
            entry: EntryName::Named(name.to_owned()),
        })
    }
}
