//! Reader for the RS2 revision-317 cache: sector chains, jag archives, and
//! a decoder for every record type. Everything decoded comes back as a
//! plain `serde` struct from [`types`].
//!
//! The cache is read-only here. [`Cache::open`] memory-maps
//! `main_file_cache.dat` and reads the five index files; every reader takes
//! `&self`, so you can decode in parallel. Decoders never panic on bad
//! input: they return a typed error, and unknown config opcodes are logged
//! through `tracing` and skipped.
//!
//! # Modules
//!
//! - [`cache`] — index records and sector chains ([`Cache`]).
//! - [`compress`] — gzip for index 1–4 files, headerless bzip2 for archives.
//! - [`archive`] — jag archives in index 0 and the entry name hash.
//! - [`io`] — the big-endian [`Cursor`](io::Cursor) every decoder reads with.
//! - [`model`] — models (index 1).
//! - [`anim`] — frame packs (index 2) and the frame id → pack
//!   [`FrameIndex`](anim::FrameIndex).
//! - [`map`] — regions (index 4) and `map_index`.
//! - [`config`] — `loc`, `npc`, `obj`, `idk`, `seq`, `spotanim`, `flo`,
//!   `varp` and `varbit` definitions.
//! - [`sprite`] — textures, media and title sprites, and fonts.
//! - [`interface`] — interface components.
//! - [`versionlist`] — per-file versions and CRCs.
//! - [`record`] — decode any record by [`RecordType`](types::record::RecordType)
//!   and id, list ids, and build metadata sidecars ([`Records`]).
//! - [`types`] — the decoded structs.
//!
//! Cache integers are big-endian. The cache is right-handed with y pointing
//! down, so negative y is up.
//!
//! # Example
//!
//! ```no_run
//! use rs2cache::config::npc::Npc;
//! use rs2cache::types::record::RecordType;
//! use rs2cache::{Cache, Records};
//!
//! let cache = Cache::open("/path/to/cache")?;
//!
//! // One model, straight from index 1.
//! let model = cache.model(0)?;
//! println!("{} vertices, {} faces", model.vertices.len(), model.faces.len());
//!
//! // A whole config table.
//! let npcs = cache.config::<Npc>()?;
//! println!("{} npc definitions", npcs.len());
//!
//! // Many records of any type: Records decodes each archive and table once.
//! let records = Records::new(&cache);
//! for id in records.ids(RecordType::Seq)?.into_iter().take(3) {
//!     let meta = records.meta(RecordType::Seq, id)?;
//!     println!("seq {id}: {} stored file(s)", meta.files.len());
//! }
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![warn(missing_docs)]

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
pub mod types;
pub mod versionlist;

pub use archive::{Archive, ArchiveError, EntryName};
pub use cache::{Cache, CacheError, IndexEntry, SectorFault, SectorVisit};
pub use compress::CompressError;
pub use record::{Records, decode_any, ids, meta};

use thiserror::Error;

/// Any error from reading and decoding a record.
///
/// [`record::is_absent`] tells you whether one just means "nothing there".
#[derive(Debug, Error)]
pub enum Error {
    /// Reading the raw file failed.
    #[error(transparent)]
    Cache(#[from] CacheError),
    /// A file of index 1–4 wouldn't inflate.
    #[error("file {index}/{id}: {source}")]
    Compress {
        /// Index number.
        index: u8,
        /// File id.
        id: u32,
        /// What went wrong.
        source: CompressError,
    },
    /// An archive in index 0 wouldn't decode.
    #[error("archive {archive}: {source}")]
    Archive {
        /// Archive number.
        archive: u8,
        /// What went wrong.
        source: ArchiveError,
    },
    /// An archive lacks an entry the decoder needs.
    #[error("archive {archive} has no entry {entry}")]
    MissingEntry {
        /// Archive number.
        archive: u8,
        /// The entry looked for.
        entry: EntryName,
    },
    /// A config table or definition wouldn't decode.
    #[error(transparent)]
    Config(#[from] config::ConfigError),
    /// A model wouldn't decode.
    #[error("model {id}: {source}")]
    Model {
        /// Model id.
        id: u32,
        /// What went wrong.
        source: model::ModelError,
    },
    /// A frame pack wouldn't decode.
    #[error("frame pack {id}: {source}")]
    Anim {
        /// Index-2 file id.
        id: u32,
        /// What went wrong.
        source: anim::FrameError,
    },
    /// A region (or `map_index`, reported as region `0_0`) wouldn't decode.
    #[error("region {x}_{y}: {source}")]
    Map {
        /// Region x.
        x: u8,
        /// Region y.
        y: u8,
        /// What went wrong.
        source: map::MapError,
    },
    /// `map_index` doesn't list the region.
    #[error("no region {x}_{y} in map_index")]
    NoRegion {
        /// Region x.
        x: u8,
        /// Region y.
        y: u8,
    },
    /// A sprite, texture or font entry wouldn't decode.
    #[error("archive {archive} sprites: {source}")]
    Sprite {
        /// Archive number.
        archive: u8,
        /// What went wrong.
        source: sprite::SpriteError,
    },
    /// The interface data wouldn't decode.
    #[error(transparent)]
    Interface(interface::InterfaceError),
    /// There's no record with that id.
    #[error("there is no {kind} {id}")]
    NoRecord {
        /// Record type.
        kind: crate::types::record::RecordType,
        /// Requested id.
        id: u32,
    },
}

impl Cache {
    /// Read and inflate a file from index 1–4 (gunzipped when it starts with
    /// the gzip magic, otherwise as stored).
    pub fn read(&self, index: u8, id: u32) -> Result<Vec<u8>, Error> {
        let raw = self.read_raw(index, id)?;
        compress::inflate(&raw).map_err(|source| Error::Compress { index, id, source })
    }

    /// Read and decode model `id` from index 1.
    pub fn model(&self, id: u32) -> Result<crate::types::model::Model, Error> {
        model::decode(&self.read(1, id)?).map_err(|source| Error::Model { id, source })
    }

    /// Read and decode frame pack `id` from index 2.
    pub fn frame_pack(&self, id: u32) -> Result<crate::types::anim::FramePack, Error> {
        anim::decode(&self.read(2, id)?).map_err(|source| Error::Anim { id, source })
    }

    /// Build the frame id → pack map by reading every pack's id list.
    /// Absent files are skipped.
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
    pub fn versionlist(&self) -> Result<crate::types::versionlist::VersionList, Error> {
        let v = self.archive(archive::VERSIONLIST)?;
        // The only failure is a short read, so report it as a truncated
        // archive.
        versionlist::decode(&v).map_err(|e| Error::Archive {
            archive: archive::VERSIONLIST,
            source: ArchiveError::Truncated(e),
        })
    }

    /// Read and inflate MIDI file `id` from index 3. MIDI is not decoded.
    pub fn midi(&self, id: u32) -> Result<Vec<u8>, Error> {
        self.read(3, id)
    }

    /// The versionlist's `map_index`: which index-4 files make up each
    /// region.
    pub fn map_index(&self) -> Result<Vec<map::MapIndexEntry>, Error> {
        let v = self.archive(archive::VERSIONLIST)?;
        map::map_index(v.require(archive::VERSIONLIST, "map_index")?).map_err(|source| Error::Map {
            x: 0,
            y: 0,
            source,
        })
    }

    /// Decode the region a `map_index` entry names.
    pub fn region_for(&self, e: &map::MapIndexEntry) -> Result<crate::types::map::Region, Error> {
        let tiles = self.read(4, e.tile_file.into())?;
        let locs = self.read(4, e.location_file.into())?;
        map::decode(e, &tiles, &locs).map_err(|source| Error::Map {
            x: e.x(),
            y: e.y(),
            source,
        })
    }

    /// Decode region `(x, y)`. Scans `map_index` each call; use
    /// [`Records`] when decoding many.
    pub fn region(&self, x: u8, y: u8) -> Result<crate::types::map::Region, Error> {
        let id = u16::from(x) << 8 | u16::from(y);
        let e = self
            .map_index()?
            .into_iter()
            .find(|e| e.region == id)
            .ok_or(Error::NoRegion { x, y })?;
        self.region_for(&e)
    }

    /// Read and decode archive `archive` of index 0, with its known entry
    /// names attached.
    pub fn archive(&self, archive: u8) -> Result<Archive, Error> {
        let raw = self.read_raw(0, archive.into())?;
        Archive::decode(&raw, Some(archive)).map_err(|source| Error::Archive { archive, source })
    }
}

impl Cache {
    /// Decode every definition of a config type, such as
    /// `cache.config::<config::npc::Npc>()`.
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
