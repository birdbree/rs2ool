//! Raw file access: the five index files and the sector chains in
//! `main_file_cache.dat`.
//!
//! An index record is 6 bytes, `size u24, firstSector u24`. The data file is
//! a run of 520-byte sectors, each an 8-byte header
//! `{fileId u16, chunk u16, nextSector u24, indexType u8}` followed by up to
//! 512 bytes of payload. `indexType` is the index number plus one.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use memmap2::Mmap;
use thiserror::Error;

/// Number of index files in a 317 cache.
pub const INDEX_COUNT: u8 = 5;

pub const SECTOR_SIZE: usize = 520;
pub const SECTOR_HEADER: usize = 8;
pub const SECTOR_PAYLOAD: usize = SECTOR_SIZE - SECTOR_HEADER;
const INDEX_RECORD: usize = 6;

/// What went wrong with one sector of a chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectorFault {
    /// The sector number points outside the data file.
    OutOfRange,
    /// The sector's header or payload runs past the end of the data file.
    Short,
    FileIdMismatch {
        found: u16,
    },
    ChunkMismatch {
        found: u16,
    },
    TypeMismatch {
        found: u8,
    },
    /// `nextSector` is 0 but the index record says more bytes remain.
    ChainEnded {
        remaining: u32,
    },
}

impl fmt::Display for SectorFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OutOfRange => write!(f, "sector number is outside the data file"),
            Self::Short => write!(f, "sector runs past the end of the data file"),
            Self::FileIdMismatch { found } => write!(f, "header names file {found}"),
            Self::ChunkMismatch { found } => write!(f, "header names chunk {found}"),
            Self::TypeMismatch { found } => write!(f, "header names index type {found}"),
            Self::ChainEnded { remaining } => {
                write!(f, "chain ends with {remaining} bytes still to read")
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum CacheError {
    #[error("cannot read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("index {0} does not exist; a 317 cache has indices 0 to 4")]
    NoSuchIndex(u8),
    #[error("index {index} has {count} files; there is no file {id}")]
    NoSuchFile { index: u8, id: u32, count: usize },
    #[error("file {index}/{id} is absent (its index record has no first sector)")]
    Absent { index: u8, id: u32 },
    #[error("file {index}/{id}: chunk {chunk} at sector {sector}: {fault}")]
    BadSector {
        index: u8,
        id: u32,
        chunk: u16,
        sector: u32,
        fault: SectorFault,
    },
}

/// One 6-byte index record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IndexEntry {
    pub size: u32,
    pub sector: u32,
}

impl IndexEntry {
    fn parse(b: &[u8]) -> Self {
        Self {
            size: u32::from_be_bytes([0, b[0], b[1], b[2]]),
            sector: u32::from_be_bytes([0, b[3], b[4], b[5]]),
        }
    }

    /// The client treats a record with no first sector as missing.
    pub fn is_absent(&self) -> bool {
        self.sector == 0
    }
}

/// One sector visited while reading a file, for metadata sidecars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SectorVisit {
    pub sector: u32,
    pub chunk: u16,
}

enum Backing {
    Mapped(Mmap),
    Owned(Vec<u8>),
}

impl Backing {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Mapped(m) => m,
            Self::Owned(v) => v,
        }
    }
}

/// An open 317 cache. Readers take `&self` and the type is `Send + Sync`, so
/// files can be decoded in parallel.
pub struct Cache {
    dat: Backing,
    indices: Vec<Vec<IndexEntry>>,
}

impl fmt::Debug for Cache {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Cache")
            .field("dat_len", &self.dat.bytes().len())
            .field(
                "file_counts",
                &self.indices.iter().map(Vec::len).collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl Cache {
    /// Open a cache directory holding `main_file_cache.dat` and
    /// `main_file_cache.idx0`–`idx4`. The data file is memory-mapped
    /// read-only; the cache is never written.
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, CacheError> {
        let dir = dir.as_ref();
        let dat_path = dir.join("main_file_cache.dat");
        let io = |path: &Path| {
            let path = path.to_path_buf();
            move |source| CacheError::Io { path, source }
        };
        let file = fs::File::open(&dat_path).map_err(io(&dat_path))?;
        // SAFETY: the map is read-only and these tools never write the cache.
        // Another process truncating the file while it is mapped would be
        // undefined behaviour; the cache is supplied as a static input.
        let dat = unsafe { Mmap::map(&file) }.map_err(io(&dat_path))?;
        let mut idx = Vec::with_capacity(INDEX_COUNT.into());
        for i in 0..INDEX_COUNT {
            let path = dir.join(format!("main_file_cache.idx{i}"));
            idx.push(fs::read(&path).map_err(io(&path))?);
        }
        Ok(Self::build(Backing::Mapped(dat), &idx))
    }

    /// Build a cache from in-memory bytes. Used by tests with synthetic data.
    pub fn from_bytes(dat: Vec<u8>, indices: &[Vec<u8>]) -> Self {
        Self::build(Backing::Owned(dat), indices)
    }

    fn build(dat: Backing, indices: &[Vec<u8>]) -> Self {
        let indices = indices
            .iter()
            .map(|raw| {
                raw.chunks_exact(INDEX_RECORD)
                    .map(IndexEntry::parse)
                    .collect()
            })
            .collect();
        Self { dat, indices }
    }

    /// Number of indices present (5 for a real cache).
    pub fn index_count(&self) -> u8 {
        self.indices.len() as u8
    }

    /// Number of file ids in an index: the index file's size / 6.
    pub fn file_count(&self, index: u8) -> Result<usize, CacheError> {
        Ok(self.entries(index)?.len())
    }

    pub fn entries(&self, index: u8) -> Result<&[IndexEntry], CacheError> {
        self.indices
            .get(usize::from(index))
            .map(Vec::as_slice)
            .ok_or(CacheError::NoSuchIndex(index))
    }

    pub fn entry(&self, index: u8, id: u32) -> Result<IndexEntry, CacheError> {
        let entries = self.entries(index)?;
        usize::try_from(id)
            .ok()
            .and_then(|i| entries.get(i))
            .copied()
            .ok_or(CacheError::NoSuchFile {
                index,
                id,
                count: entries.len(),
            })
    }

    /// Size of the data file in bytes.
    pub fn dat_len(&self) -> usize {
        self.dat.bytes().len()
    }

    /// Read a file's stored bytes (still compressed) by following its sector
    /// chain, validating every sector header.
    pub fn read_raw(&self, index: u8, id: u32) -> Result<Vec<u8>, CacheError> {
        self.read_chain(index, id, |_| ())
    }

    /// As [`read_raw`](Self::read_raw), also returning the sectors visited.
    pub fn read_raw_with_chain(
        &self,
        index: u8,
        id: u32,
    ) -> Result<(Vec<u8>, Vec<SectorVisit>), CacheError> {
        let mut chain = Vec::new();
        let data = self.read_chain(index, id, |v| chain.push(v))?;
        Ok((data, chain))
    }

    fn read_chain(
        &self,
        index: u8,
        id: u32,
        mut visit: impl FnMut(SectorVisit),
    ) -> Result<Vec<u8>, CacheError> {
        let entry = self.entry(index, id)?;
        if entry.is_absent() {
            return Err(CacheError::Absent { index, id });
        }
        let dat = self.dat.bytes();
        let size = entry.size as usize;
        let mut out = Vec::with_capacity(size);
        let mut sector = entry.sector;
        let mut chunk: u16 = 0;
        let bad = |chunk, sector, fault| CacheError::BadSector {
            index,
            id,
            chunk,
            sector,
            fault,
        };
        while out.len() < size {
            if sector == 0 {
                let remaining = (size - out.len()) as u32;
                return Err(bad(chunk, sector, SectorFault::ChainEnded { remaining }));
            }
            let start = sector as usize * SECTOR_SIZE;
            if start >= dat.len() {
                return Err(bad(chunk, sector, SectorFault::OutOfRange));
            }
            let take = (size - out.len()).min(SECTOR_PAYLOAD);
            let Some(block) = dat.get(start..start + SECTOR_HEADER + take) else {
                return Err(bad(chunk, sector, SectorFault::Short));
            };
            let h = &block[..SECTOR_HEADER];
            let file_id = u16::from_be_bytes([h[0], h[1]]);
            let found_chunk = u16::from_be_bytes([h[2], h[3]]);
            let next = u32::from_be_bytes([0, h[4], h[5], h[6]]);
            let kind = h[7];
            // The header stores the id as 16 bits; compare the same way the
            // client does, against the requested id.
            if u32::from(file_id) != id {
                return Err(bad(
                    chunk,
                    sector,
                    SectorFault::FileIdMismatch { found: file_id },
                ));
            }
            if found_chunk != chunk {
                return Err(bad(
                    chunk,
                    sector,
                    SectorFault::ChunkMismatch { found: found_chunk },
                ));
            }
            if kind != index + 1 {
                return Err(bad(
                    chunk,
                    sector,
                    SectorFault::TypeMismatch { found: kind },
                ));
            }
            visit(SectorVisit { sector, chunk });
            out.extend_from_slice(&block[SECTOR_HEADER..]);
            sector = next;
            chunk = chunk.wrapping_add(1);
        }
        Ok(out)
    }
}

#[cfg(test)]
pub(crate) mod synth {
    //! Build synthetic caches for tests.

    use super::*;

    /// Lay out files into a data file. `files[index]` is a list of
    /// `(id, bytes)`; ids not listed are absent. Sector 0 is left unused, as
    /// in real caches.
    pub fn build(files: &[Vec<(u32, Vec<u8>)>]) -> (Vec<u8>, Vec<Vec<u8>>) {
        let mut dat = vec![0u8; SECTOR_SIZE];
        let mut indices = Vec::new();
        for (index, list) in files.iter().enumerate() {
            let count = list.iter().map(|(id, _)| id + 1).max().unwrap_or(0) as usize;
            let mut idx = vec![0u8; count * INDEX_RECORD];
            for (id, data) in list {
                let first = (dat.len() / SECTOR_SIZE) as u32;
                let rec = &mut idx[*id as usize * INDEX_RECORD..][..INDEX_RECORD];
                rec[..3].copy_from_slice(&(data.len() as u32).to_be_bytes()[1..]);
                rec[3..].copy_from_slice(&first.to_be_bytes()[1..]);
                let chunks: Vec<&[u8]> = data.chunks(SECTOR_PAYLOAD).collect();
                for (chunk, payload) in chunks.iter().enumerate() {
                    let this = (dat.len() / SECTOR_SIZE) as u32;
                    let next = if chunk + 1 == chunks.len() {
                        0
                    } else {
                        this + 1
                    };
                    dat.extend_from_slice(&(*id as u16).to_be_bytes());
                    dat.extend_from_slice(&(chunk as u16).to_be_bytes());
                    dat.extend_from_slice(&next.to_be_bytes()[1..]);
                    dat.push(index as u8 + 1);
                    dat.extend_from_slice(payload);
                    dat.resize(dat.len().next_multiple_of(SECTOR_SIZE), 0);
                }
            }
            indices.push(idx);
        }
        (dat, indices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn three_sector_file() -> Vec<u8> {
        (0..SECTOR_PAYLOAD * 2 + 100).map(|i| i as u8).collect()
    }

    fn cache_with(files: &[Vec<(u32, Vec<u8>)>]) -> (Vec<u8>, Vec<Vec<u8>>) {
        synth::build(files)
    }

    #[test]
    fn reads_a_three_sector_file() {
        let data = three_sector_file();
        let (dat, idx) = cache_with(&[vec![], vec![(2, data.clone())]]);
        let cache = Cache::from_bytes(dat, &idx);
        assert_eq!(cache.file_count(1).unwrap(), 3);
        let (read, chain) = cache.read_raw_with_chain(1, 2).unwrap();
        assert_eq!(read, data);
        assert_eq!(
            chain
                .iter()
                .map(|v| (v.sector, v.chunk))
                .collect::<Vec<_>>(),
            [(1, 0), (2, 1), (3, 2)]
        );
    }

    #[test]
    fn absent_and_missing_files() {
        let (dat, idx) = cache_with(&[vec![(1, vec![1, 2, 3])]]);
        let cache = Cache::from_bytes(dat, &idx);
        assert!(matches!(
            cache.read_raw(0, 0),
            Err(CacheError::Absent { index: 0, id: 0 })
        ));
        assert!(matches!(
            cache.read_raw(0, 2),
            Err(CacheError::NoSuchFile { count: 2, .. })
        ));
        assert!(matches!(
            cache.read_raw(7, 0),
            Err(CacheError::NoSuchIndex(7))
        ));
        assert_eq!(cache.read_raw(0, 1).unwrap(), [1, 2, 3]);
    }

    #[test]
    fn broken_chain_is_a_typed_error() {
        let (mut dat, idx) = cache_with(&[vec![(0, three_sector_file())]]);
        // Point sector 1's next at 0 so the chain ends early.
        dat[SECTOR_SIZE + 4..SECTOR_SIZE + 7].copy_from_slice(&[0, 0, 0]);
        let cache = Cache::from_bytes(dat, &idx);
        match cache.read_raw(0, 0) {
            Err(CacheError::BadSector {
                chunk: 1,
                sector: 0,
                fault: SectorFault::ChainEnded { remaining },
                ..
            }) => assert_eq!(remaining as usize, SECTOR_PAYLOAD + 100),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn mismatched_type_byte_is_a_typed_error() {
        let (mut dat, idx) = cache_with(&[vec![], vec![(0, three_sector_file())]]);
        dat[2 * SECTOR_SIZE + 7] = 9;
        let cache = Cache::from_bytes(dat, &idx);
        assert!(matches!(
            cache.read_raw(1, 0),
            Err(CacheError::BadSector {
                chunk: 1,
                sector: 2,
                fault: SectorFault::TypeMismatch { found: 9 },
                ..
            })
        ));
    }

    #[test]
    fn wrong_file_id_chunk_and_truncation() {
        let (dat, idx) = cache_with(&[vec![(0, three_sector_file())]]);

        let mut d = dat.clone();
        d[2 * SECTOR_SIZE + 1] = 5;
        let err = Cache::from_bytes(d, &idx).read_raw(0, 0).unwrap_err();
        assert!(matches!(
            err,
            CacheError::BadSector {
                fault: SectorFault::FileIdMismatch { found: 5 },
                ..
            }
        ));

        let mut d = dat.clone();
        d[3 * SECTOR_SIZE + 3] = 7;
        let err = Cache::from_bytes(d, &idx).read_raw(0, 0).unwrap_err();
        assert!(matches!(
            err,
            CacheError::BadSector {
                fault: SectorFault::ChunkMismatch { found: 7 },
                ..
            }
        ));

        let mut d = dat.clone();
        d.truncate(3 * SECTOR_SIZE + 50);
        let err = Cache::from_bytes(d, &idx).read_raw(0, 0).unwrap_err();
        assert!(matches!(
            err,
            CacheError::BadSector {
                sector: 3,
                fault: SectorFault::Short,
                ..
            }
        ));

        let mut d = dat;
        d.truncate(2 * SECTOR_SIZE);
        let err = Cache::from_bytes(d, &idx).read_raw(0, 0).unwrap_err();
        assert!(matches!(
            err,
            CacheError::BadSector {
                sector: 2,
                fault: SectorFault::OutOfRange,
                ..
            }
        ));
    }

    #[test]
    fn cache_is_send_and_sync() {
        fn check<T: Send + Sync>() {}
        check::<Cache>();
    }
}
