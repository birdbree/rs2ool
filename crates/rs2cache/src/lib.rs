//! Reader for the RuneScape 2 revision-317 cache: sector chains, jag
//! archives, and every record decoder. Everything decoded is returned as an
//! `rs2intermediate` struct.

pub mod anim;
pub mod archive;
pub mod cache;
pub mod compress;
pub mod config;
pub mod interface;
pub mod io;
pub mod map;
pub mod model;
pub mod record;
pub mod sprite;
pub mod versionlist;

pub use archive::{Archive, ArchiveError, EntryName};
pub use cache::{Cache, CacheError, IndexEntry, SectorFault, SectorVisit};
pub use compress::CompressError;
pub use record::{decode_any, ids, meta};

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
    #[error("region {x}_{y}: {source}")]
    Map { x: u8, y: u8, source: map::MapError },
    #[error("no region {x}_{y} in map_index")]
    NoRegion { x: u8, y: u8 },
    #[error("archive {archive} sprites: {source}")]
    Sprite {
        archive: u8,
        source: sprite::SpriteError,
    },
    #[error(transparent)]
    Interface(interface::InterfaceError),
    #[error("there is no {kind} {id}")]
    NoRecord {
        kind: rs2intermediate::record::RecordType,
        id: u32,
    },
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

    /// The versionlist tables (versions, CRCs, model/anim/midi indices).
    pub fn versionlist(&self) -> Result<rs2intermediate::versionlist::VersionList, Error> {
        let v = self.archive(archive::VERSIONLIST)?;
        versionlist::decode(&v).map_err(|e| Error::Archive {
            archive: archive::VERSIONLIST,
            source: ArchiveError::Truncated(e),
        })
    }

    /// Read and inflate MIDI file `id` from index 3. MIDI is not decoded.
    pub fn midi(&self, id: u32) -> Result<Vec<u8>, Error> {
        self.read(3, id)
    }

    /// The versionlist's `map_index`.
    pub fn map_index(&self) -> Result<Vec<map::MapIndexEntry>, Error> {
        let v = self.archive(archive::VERSIONLIST)?;
        map::map_index(v.require(archive::VERSIONLIST, "map_index")?).map_err(|source| Error::Map {
            x: 0,
            y: 0,
            source,
        })
    }

    /// Decode the region a `map_index` entry names.
    pub fn region_for(
        &self,
        e: &map::MapIndexEntry,
    ) -> Result<rs2intermediate::map::Region, Error> {
        let tiles = self.read(4, e.tile_file.into())?;
        let locs = self.read(4, e.location_file.into())?;
        map::decode(e, &tiles, &locs).map_err(|source| Error::Map {
            x: e.x(),
            y: e.y(),
            source,
        })
    }

    /// Decode region `(x, y)`.
    pub fn region(&self, x: u8, y: u8) -> Result<rs2intermediate::map::Region, Error> {
        let id = u16::from(x) << 8 | u16::from(y);
        let e = self
            .map_index()?
            .into_iter()
            .find(|e| e.region == id)
            .ok_or(Error::NoRegion { x, y })?;
        self.region_for(&e)
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
