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
//! Index entries are sorted by id; `offset` is from the start of the blob,
//! `length` is the stored (compressed) length and `crc` is the CRC32 of the
//! uncompressed record. The pack is named by the lowercase hex SHA-256 of
//! index and blob.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::bytes::{Reader, Truncated, Writer};

pub const MAGIC: &[u8; 4] = b"RS2P";
pub const VERSION: u16 = 1;
const HEADER: usize = 4 + 2 + 1 + 1 + 4 + 4;
const INDEX_ENTRY: usize = 16;
const ZSTD_LEVEL: i32 = 9;

/// What a pack holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum Kind {
    Models = 1,
    Materials = 2,
    Textures = 3,
    Sprites = 4,
    Objects = 5,
    Kits = 6,
    Rigs = 7,
    Items = 8,
    Anims = 9,
    Fluids = 10,
    Strings = 11,
}

impl Kind {
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
    None = 0,
    Zstd = 1,
}

/// The pack's JSON manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackManifest {
    pub kind: Kind,
    pub record_count: u32,
    /// CRC32 identifying the source cache (of its versionlist archive).
    pub source_cache_crc: u32,
    pub codec: Codec,
    /// Kind-specific tables (for example the floor → material map).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Error)]
pub enum PackError {
    #[error("not a pack (bad magic)")]
    Magic,
    #[error("unsupported pack version {0}")]
    Version(u16),
    #[error("unknown kind {0}")]
    Kind(u8),
    #[error("unknown codec {0}")]
    Codec(u8),
    #[error(transparent)]
    Truncated(#[from] Truncated),
    #[error("manifest: {0}")]
    Manifest(#[from] serde_json::Error),
    #[error("record {id}: stored bytes run past the blob")]
    Range { id: u32 },
    #[error("record {id}: {source}")]
    Decompress { id: u32, source: std::io::Error },
    #[error("record {id}: CRC {found:08x} does not match the index's {expected:08x}")]
    Crc { id: u32, expected: u32, found: u32 },
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
}

/// Collects records and bakes them into a pack.
#[derive(Debug, Clone)]
pub struct PackWriter {
    kind: Kind,
    source_cache_crc: u32,
    codec: Codec,
    records: BTreeMap<u32, Vec<u8>>,
    extra: BTreeMap<String, serde_json::Value>,
}

impl PackWriter {
    pub fn new(kind: Kind, source_cache_crc: u32) -> Self {
        Self {
            kind,
            source_cache_crc,
            codec: Codec::Zstd,
            records: BTreeMap::new(),
            extra: BTreeMap::new(),
        }
    }

    pub fn with_codec(mut self, codec: Codec) -> Self {
        self.codec = codec;
        self
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Add or replace record `id`.
    pub fn insert(&mut self, id: u32, record: Vec<u8>) {
        self.records.insert(id, record);
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Attach a kind-specific manifest table.
    pub fn set_extra(&mut self, key: &str, value: serde_json::Value) {
        self.extra.insert(key.to_owned(), value);
    }

    /// The pack bytes and its name (hex SHA-256 of index and blob).
    pub fn bake(&self) -> (String, Vec<u8>) {
        let mut index = Writer::default();
        let mut blob = Vec::new();
        for (&id, rec) in &self.records {
            let stored = match self.codec {
                Codec::None => rec.clone(),
                Codec::Zstd => zstd::bulk::compress(rec, ZSTD_LEVEL).expect("zstd compress"),
            };
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
        let mut h = Sha256::new();
        h.update(&index.0);
        h.update(&blob);
        let name: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
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
        (name, out.0)
    }

    /// Bake and write `<dir>/<kind>/<hash>.pack`; returns the path.
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

/// One index entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexEntry {
    pub id: u32,
    pub offset: u32,
    pub length: u32,
    pub crc: u32,
}

/// A pack opened for reading.
#[derive(Debug, Clone)]
pub struct PackReader {
    pub manifest: PackManifest,
    pub index: Vec<IndexEntry>,
    blob: Vec<u8>,
    /// Hex SHA-256 of index and blob, recomputed on open.
    pub hash: String,
}

impl PackReader {
    pub fn open(path: &Path) -> Result<Self, PackError> {
        let bytes = std::fs::read(path).map_err(|source| PackError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_bytes(&bytes)
    }

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
        let blob = bytes[HEADER + manifest_len + count * INDEX_ENTRY..].to_vec();
        let mut h = Sha256::new();
        h.update(index_bytes);
        h.update(&blob);
        let hash = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
        Ok(Self {
            manifest,
            index,
            blob,
            hash,
        })
    }

    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.index.iter().map(|e| e.id)
    }

    /// Whether record `id` is present, without reading it.
    pub fn contains(&self, id: u32) -> bool {
        self.index.binary_search_by_key(&id, |e| e.id).is_ok()
    }

    /// Record `id`, decompressed and CRC-checked; `None` if absent.
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
