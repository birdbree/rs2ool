//! The pack container.
//!
//! ```text
//! magic        "RS2P"
//! version      u16 = 1
//! kind         u8   (Kind)
//! codec        u8   (Codec)
//! manifestLen  u32
//! recordCount  u32
//! manifest     manifestLen bytes of UTF-8 JSON (PackManifest)
//! index        recordCount × {id u32, offset u32, length u32, crc u32}
//! blob         the records, each compressed on its own
//! ```
//!
//! The header is 16 bytes, and there's no blob length: the blob is
//! everything after the index. Index entries are sorted by id; `offset` is
//! from the start of the blob, `length` is the stored (compressed) length
//! and `crc` is the CRC32 of the uncompressed record, whatever the codec.
//!
//! The pack is named by the lowercase hex SHA-256 of the whole file, header
//! and manifest included, so `sha256sum` on a pack prints its name. Two packs
//! that differ anywhere, even only in the manifest's `source_cache_crc` or
//! `extra` tables, get different names.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::bytes::{Reader, Truncated, Writer};

/// The first four bytes of every pack.
pub const MAGIC: &[u8; 4] = b"RS2P";
/// The only container version there is.
pub const VERSION: u16 = 1;
// magic, version, kind, codec, manifestLen, recordCount
const HEADER: usize = 4 + 2 + 1 + 1 + 4 + 4;
// id, offset, length, crc: four u32s
const INDEX_ENTRY: usize = 16;
// Fixed, so the same records always compress to the same bytes.
const ZSTD_LEVEL: i32 = 9;

/// What a pack holds.
///
/// The number goes in the header; the lowercase name goes in the manifest
/// and is the directory [`PackWriter::write_to`] puts the pack in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum Kind {
    /// Triangle meshes, see [`crate::records::model`].
    Models = 1,
    /// Materials and the palette, see [`crate::records::material`].
    Materials = 2,
    /// Texture rectangles and atlases, see [`crate::records::texture`].
    Textures = 3,
    /// Sprites and font metrics, see [`crate::records::sprite`].
    Sprites = 4,
    /// Placeable objects, see [`crate::records::object`].
    Objects = 5,
    /// Model kits, see [`crate::records::object::KitRecord`].
    Kits = 6,
    /// Bone hierarchies, see [`crate::records::rig`].
    Rigs = 7,
    /// Reserved. The container accepts it, but there's no record layout.
    Items = 8,
    /// Animation clips, see [`crate::records::anim`].
    Anims = 9,
    /// Reserved. The container accepts it, but there's no record layout.
    Fluids = 10,
    /// String tables, see [`crate::records::strings`].
    Strings = 11,
}

impl Kind {
    /// Every kind, in number order.
    pub const ALL: [Kind; 11] = [
        Kind::Models,
        Kind::Materials,
        Kind::Textures,
        Kind::Sprites,
        Kind::Objects,
        Kind::Kits,
        Kind::Rigs,
        Kind::Items,
        Kind::Anims,
        Kind::Fluids,
        Kind::Strings,
    ];

    /// The lowercase name used in manifests and directory names.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Models => "models",
            Kind::Materials => "materials",
            Kind::Textures => "textures",
            Kind::Sprites => "sprites",
            Kind::Objects => "objects",
            Kind::Kits => "kits",
            Kind::Rigs => "rigs",
            Kind::Items => "items",
            Kind::Anims => "anims",
            Kind::Fluids => "fluids",
            Kind::Strings => "strings",
        }
    }

    fn from_u8(v: u8) -> Option<Self> {
        Kind::ALL.into_iter().find(|k| *k as u8 == v)
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// How each record in the blob is compressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum Codec {
    /// Records are stored as they are.
    None = 0,
    /// Each record is one zstd frame at level 9, no dictionary. The default.
    Zstd = 1,
}

/// The pack's JSON manifest.
///
/// Serialised compactly with the fields in this order. The reader doesn't
/// check that `kind`, `record_count` and `codec` agree with the header; the
/// writer always makes them agree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackManifest {
    /// What the records are.
    pub kind: Kind,
    /// Number of records, same as the header's count.
    pub record_count: u32,
    /// CRC32 identifying the source cache: the CRC of its stored versionlist
    /// archive, before decompression. It changes whenever the cache does.
    pub source_cache_crc: u32,
    /// How the records are compressed. The reader decompresses with this,
    /// not the header byte.
    pub codec: Codec,
    /// Kind-specific tables (for example the floor → material map).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// Why a pack wouldn't open, or a record wouldn't read back.
#[derive(Debug, Error)]
pub enum PackError {
    /// The file doesn't start with [`MAGIC`].
    #[error("not a pack (bad magic)")]
    Magic,
    /// The version isn't [`VERSION`].
    #[error("unsupported pack version {0}")]
    Version(u16),
    /// The kind number isn't one of the [`Kind`]s.
    #[error("unknown kind {0}")]
    Kind(u8),
    /// The codec byte is above 1.
    #[error("unknown codec {0}")]
    Codec(u8),
    /// The header, manifest or index runs past the end of the file.
    #[error(transparent)]
    Truncated(#[from] Truncated),
    /// The manifest isn't valid JSON for [`PackManifest`].
    #[error("manifest: {0}")]
    Manifest(#[from] serde_json::Error),
    /// A record's stored bytes run past the end of the blob.
    #[error("record {id}: stored bytes run past the blob")]
    Range {
        /// The record's id.
        id: u32,
    },
    /// A record's zstd frame didn't decode.
    #[error("record {id}: {source}")]
    Decompress {
        /// The record's id.
        id: u32,
        /// The decoder's error.
        source: std::io::Error,
    },
    /// A record decompressed, but its CRC32 doesn't match the index.
    #[error("record {id}: CRC {found:08x} does not match the index's {expected:08x}")]
    Crc {
        /// The record's id.
        id: u32,
        /// The CRC the index entry gives.
        expected: u32,
        /// The CRC of the bytes that came out.
        found: u32,
    },
    /// A file couldn't be read or written.
    #[error("{path}: {source}")]
    Io {
        /// The file or directory involved.
        path: PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },
}

/// Collects records and bakes them into a pack.
///
/// Records live in a map ordered by id, so the order you insert them in
/// doesn't matter, and inserting an id twice keeps the later record.
#[derive(Debug, Clone)]
pub struct PackWriter {
    kind: Kind,
    source_cache_crc: u32,
    codec: Codec,
    records: BTreeMap<u32, Vec<u8>>,
    extra: BTreeMap<String, serde_json::Value>,
}

impl PackWriter {
    /// An empty writer for `kind`, compressing with zstd.
    pub fn new(kind: Kind, source_cache_crc: u32) -> Self {
        Self {
            kind,
            source_cache_crc,
            codec: Codec::Zstd,
            records: BTreeMap::new(),
            extra: BTreeMap::new(),
        }
    }

    /// Use `codec` instead of zstd.
    pub fn with_codec(mut self, codec: Codec) -> Self {
        self.codec = codec;
        self
    }

    /// The kind this writer was made for.
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Add or replace record `id`.
    pub fn insert(&mut self, id: u32, record: Vec<u8>) {
        self.records.insert(id, record);
    }

    /// Number of records so far.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether there are no records yet.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Attach a kind-specific manifest table under `key`, replacing any
    /// earlier one with that key.
    pub fn set_extra(&mut self, key: &str, value: serde_json::Value) {
        self.extra.insert(key.to_owned(), value);
    }

    /// Bake the pack. Returns its name (hex SHA-256 of the whole file) and
    /// its bytes.
    pub fn bake(&self) -> (String, Vec<u8>) {
        let mut index = Writer::default();
        let mut blob = Vec::new();
        // BTreeMap iteration is by id, so the index comes out sorted and each
        // record starts where the previous one ended.
        for (&id, rec) in &self.records {
            let stored = match self.codec {
                Codec::None => rec.clone(),
                Codec::Zstd => zstd::bulk::compress(rec, ZSTD_LEVEL).expect("zstd compress"),
            };
            // The CRC is over the uncompressed record, whatever the codec.
            index
                .u32(id)
                .u32(blob.len() as u32)
                .u32(stored.len() as u32)
                .u32(crc32fast::hash(rec));
            blob.extend(stored);
        }
        let manifest = PackManifest {
            kind: self.kind,
            record_count: self.records.len() as u32,
            source_cache_crc: self.source_cache_crc,
            codec: self.codec,
            extra: self.extra.clone(),
        };
        let manifest = serde_json::to_vec(&manifest).expect("manifest serialises");
        let mut out = Writer::default();
        out.bytes(MAGIC)
            .u16(VERSION)
            .u8(self.kind as u8)
            .u8(self.codec as u8)
            .u32(manifest.len() as u32)
            .u32(self.records.len() as u32)
            .bytes(&manifest)
            .bytes(&index.0)
            .bytes(&blob);
        (name(&out.0), out.0)
    }

    /// Bake and write to `<dir>/<kind>/<hash>.pack`, creating directories as
    /// needed. Returns the path.
    pub fn write_to(&self, dir: &Path) -> Result<PathBuf, PackError> {
        let (name, bytes) = self.bake();
        let dir = dir.join(self.kind.name());
        let io = |path: &Path| {
            let path = path.to_path_buf();
            move |source| PackError::Io { path, source }
        };
        std::fs::create_dir_all(&dir).map_err(io(&dir))?;
        let path = dir.join(format!("{name}.pack"));
        std::fs::write(&path, bytes).map_err(io(&path))?;
        Ok(path)
    }
}

/// A pack's name: the lowercase hex SHA-256 of all its bytes.
fn name(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// One index entry: where a record sits in the blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexEntry {
    /// Record id.
    pub id: u32,
    /// Start of the stored bytes, from the start of the blob.
    pub offset: u32,
    /// Stored (compressed) length in bytes.
    pub length: u32,
    /// CRC32 of the uncompressed record.
    pub crc: u32,
}

/// A pack opened for reading.
///
/// Opening checks the header and parses the manifest and index, but doesn't
/// touch any record. Each [`get`](Self::get) decompresses and CRC-checks one.
#[derive(Debug, Clone)]
pub struct PackReader {
    /// The parsed manifest.
    pub manifest: PackManifest,
    /// The index, in file order. The writer sorts it by id, and lookups
    /// assume it is; the reader doesn't check.
    pub index: Vec<IndexEntry>,
    blob: Vec<u8>,
    /// Hex SHA-256 of the whole file, recomputed on open. Nothing compares
    /// it with the file name.
    pub hash: String,
}

impl PackReader {
    /// Read and open the pack at `path`.
    pub fn open(path: &Path) -> Result<Self, PackError> {
        let bytes = std::fs::read(path).map_err(|source| PackError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_bytes(&bytes)
    }

    /// Open a pack that's already in memory. The blob is copied out.
    ///
    /// Checks, in order: magic, version, kind number, codec byte, then that
    /// the manifest and index fit and the manifest parses.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PackError> {
        let mut r = Reader::new(bytes);
        if r.bytes(4)? != MAGIC {
            return Err(PackError::Magic);
        }
        let version = r.u16()?;
        if version != VERSION {
            return Err(PackError::Version(version));
        }
        let kind = r.u8()?;
        // Only validated here; the manifest's kind is the one that's kept.
        Kind::from_u8(kind).ok_or(PackError::Kind(kind))?;
        let codec = r.u8()?;
        if codec > 1 {
            return Err(PackError::Codec(codec));
        }
        let manifest_len = r.u32()? as usize;
        let count = r.u32()? as usize;
        let manifest: PackManifest = serde_json::from_slice(r.bytes(manifest_len)?)?;
        let index_bytes = r.bytes(count * INDEX_ENTRY)?;
        let mut ir = Reader::new(index_bytes);
        let index = (0..count)
            .map(|_| {
                Ok(IndexEntry {
                    id: ir.u32()?,
                    offset: ir.u32()?,
                    length: ir.u32()?,
                    crc: ir.u32()?,
                })
            })
            .collect::<Result<Vec<_>, Truncated>>()?;
        // The blob runs from the end of the index to the end of the file.
        let blob = bytes[HEADER + manifest_len + count * INDEX_ENTRY..].to_vec();
        Ok(Self {
            manifest,
            index,
            blob,
            hash: name(bytes),
        })
    }

    /// Every record id, in index order (ascending, for any pack the writer
    /// made).
    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.index.iter().map(|e| e.id)
    }

    /// Whether record `id` is present, without reading it.
    pub fn contains(&self, id: u32) -> bool {
        self.index.binary_search_by_key(&id, |e| e.id).is_ok()
    }

    /// Record `id`, decompressed and CRC-checked.
    ///
    /// `None` if the id isn't in the index, which isn't an error. Found by
    /// binary search, so it relies on the index being sorted.
    pub fn get(&self, id: u32) -> Option<Result<Vec<u8>, PackError>> {
        let e = *self
            .index
            .get(self.index.binary_search_by_key(&id, |e| e.id).ok()?)?;
        Some(self.read(e))
    }

    fn read(&self, e: IndexEntry) -> Result<Vec<u8>, PackError> {
        let (start, end) = (e.offset as usize, e.offset as usize + e.length as usize);
        let stored = self
            .blob
            .get(start..end)
            .ok_or(PackError::Range { id: e.id })?;
        let data = match self.manifest.codec {
            Codec::None => stored.to_vec(),
            Codec::Zstd => zstd::stream::decode_all(stored)
                .map_err(|source| PackError::Decompress { id: e.id, source })?,
        };
        let found = crc32fast::hash(&data);
        if found != e.crc {
            return Err(PackError::Crc {
                id: e.id,
                expected: e.crc,
                found,
            });
        }
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> PackWriter {
        let mut w = PackWriter::new(Kind::Models, 0xdead_beef);
        w.insert(9, b"nine".to_vec());
        w.insert(2, vec![7; 1000]);
        w.insert(5, Vec::new());
        w.set_extra("note", serde_json::json!({"b": 1, "a": 2}));
        w
    }

    #[test]
    fn round_trips_and_sorts_by_id() {
        let (name, bytes) = sample().bake();
        let r = PackReader::from_bytes(&bytes).unwrap();
        assert_eq!(r.hash, name);
        assert_eq!(r.manifest.kind, Kind::Models);
        assert_eq!(r.manifest.record_count, 3);
        assert_eq!(r.manifest.source_cache_crc, 0xdead_beef);
        assert_eq!(r.ids().collect::<Vec<_>>(), [2, 5, 9]);
        assert_eq!(r.get(9).unwrap().unwrap(), b"nine");
        assert_eq!(r.get(2).unwrap().unwrap(), vec![7; 1000]);
        assert_eq!(r.get(5).unwrap().unwrap(), b"");
        assert!(r.get(3).is_none());
    }

    #[test]
    fn baking_is_deterministic_and_order_independent() {
        let a = sample().bake();
        let mut w = PackWriter::new(Kind::Models, 0xdead_beef);
        w.set_extra("note", serde_json::json!({"a": 2, "b": 1}));
        w.insert(5, Vec::new());
        w.insert(9, b"nine".to_vec());
        w.insert(2, vec![7; 1000]);
        assert_eq!(a, w.bake());
        assert_eq!(a.0.len(), 64);
    }

    #[test]
    fn name_covers_the_whole_file() {
        let (name, bytes) = sample().bake();
        let digest: String = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(name, digest);

        // Same records, different manifests: every one gets its own name.
        let mut other_crc = sample();
        other_crc.source_cache_crc = 1;
        let mut other_extra = sample();
        other_extra.set_extra("note", serde_json::json!({"a": 3}));
        let mut other_kind = sample();
        other_kind.kind = Kind::Textures;
        let names = [
            sample(),
            other_crc,
            other_extra,
            other_kind,
            sample().with_codec(Codec::None),
        ]
        .map(|w| w.bake().0);
        for (i, a) in names.iter().enumerate() {
            for b in &names[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn uncompressed_codec() {
        let (_, bytes) = sample().with_codec(Codec::None).bake();
        let r = PackReader::from_bytes(&bytes).unwrap();
        assert_eq!(r.get(9).unwrap().unwrap(), b"nine");
    }

    #[test]
    fn corruption_is_detected() {
        let (_, mut bytes) = sample().with_codec(Codec::None).bake();
        let n = bytes.len();
        bytes[n - 1] ^= 0xff;
        let r = PackReader::from_bytes(&bytes).unwrap();
        assert!(matches!(
            r.get(9).unwrap(),
            Err(PackError::Crc { id: 9, .. })
        ));
        assert!(matches!(
            PackReader::from_bytes(b"nope"),
            Err(PackError::Magic)
        ));
        assert!(PackReader::from_bytes(&bytes[..20]).is_err());
    }
}
