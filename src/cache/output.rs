//! The output directory: files, sidecars, and `manifest.json`.
//!
//! Every write goes through [`Output`], which records it for the manifest.
//! Writers run in parallel, so the lists sit behind mutexes and get sorted
//! at the end; two runs on the same cache give the same manifest.
//!
//! ```text
//! manifest.json
//! {
//!   "files":    [ { "path", "sources", "raw_size", "inflated_size", "crc32" }, … ],
//!   "absent":   N,
//!   "failures": [ { "record", "error" }, … ]
//! }
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Context;
use rs2cache::types::record::{Meta, Source};
use serde::Serialize;

/// One written file, as listed in `manifest.json`.
#[derive(Debug, Clone, Serialize)]
pub struct ManifestEntry {
    /// Relative to the output directory, `/`-separated.
    pub path: String,
    /// The cache files or archive entries it came from. Empty for files
    /// that don't come from one place, like `xref.json` or a `.jsonl`.
    pub sources: Vec<Source>,
    /// Stored size of the source(s), in bytes; 0 when unknown.
    pub raw_size: u64,
    /// Inflated size of the source(s), in bytes; 0 when unknown.
    pub inflated_size: u64,
    /// CRC32 of the written file.
    pub crc32: u32,
}

/// A record that couldn't be written, as listed in `manifest.json`.
#[derive(Debug, Clone, Serialize)]
pub struct Failure {
    /// What failed, e.g. `npc 12`, `idx1 40` or `region 50_50`.
    pub record: String,
    /// The error with its whole context chain.
    pub error: String,
}

#[derive(Serialize)]
struct Manifest<'a> {
    files: &'a [ManifestEntry],
    absent: usize,
    failures: &'a [Failure],
}

/// What to write beside each payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaMode {
    /// Payloads only, no sidecars.
    Off,
    /// Payloads and a `.meta.json` sidecar for each (`--meta`).
    With,
    /// Sidecars only (`--meta-only`).
    Only,
}

/// An output directory being written to.
///
/// Shared by reference across rayon workers. Call [`Output::finish`] at the
/// end to write `manifest.json`; drop it without that and there's no
/// manifest.
pub struct Output {
    root: PathBuf,
    /// Which of payloads and sidecars get written.
    pub meta: MetaMode,
    files: Mutex<Vec<ManifestEntry>>,
    failures: Mutex<Vec<Failure>>,
    absent: AtomicUsize,
}

/// Sizes and sources of what a written file came from.
///
/// Copied into the [`ManifestEntry`] of every file written with it. The
/// default (no sources, sizes 0) is for files that don't come from one
/// place.
#[derive(Debug, Clone, Default)]
pub struct Provenance {
    /// The cache files or archive entries read.
    pub sources: Vec<Source>,
    /// Their total stored size, in bytes.
    pub raw_size: u64,
    /// Their total inflated size, in bytes.
    pub inflated_size: u64,
}

impl Provenance {
    /// Takes the sources from a record's sidecar and adds up their sizes.
    pub fn from_meta(m: &Meta) -> Self {
        Self {
            sources: m.files.iter().map(|f| f.source.clone()).collect(),
            raw_size: m.files.iter().map(|f| f.raw_size).sum(),
            inflated_size: m.files.iter().map(|f| f.inflated_size).sum(),
        }
    }
}

impl Output {
    /// Creates `root` (and its parents) if needed and starts an empty
    /// manifest.
    pub fn new(root: &Path, meta: MetaMode) -> anyhow::Result<Self> {
        fs::create_dir_all(root).with_context(|| format!("creating {}", root.display()))?;
        Ok(Self {
            root: root.to_path_buf(),
            meta,
            files: Mutex::default(),
            failures: Mutex::default(),
            absent: AtomicUsize::new(0),
        })
    }

    /// The output directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Writes `bytes` to `rel` under the root and adds it to the manifest.
    fn put(&self, rel: &str, bytes: &[u8], prov: &Provenance) -> anyhow::Result<()> {
        let path = self.root.join(rel);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
        self.files.lock().unwrap().push(ManifestEntry {
            path: rel.to_owned(),
            sources: prov.sources.clone(),
            raw_size: prov.raw_size,
            inflated_size: prov.inflated_size,
            crc32: crc32fast::hash(bytes),
        });
        Ok(())
    }

    /// Writes a payload to `rel`, unless only sidecars were asked for.
    pub fn payload(&self, rel: &str, bytes: &[u8], prov: &Provenance) -> anyhow::Result<()> {
        if self.meta == MetaMode::Only {
            return Ok(());
        }
        self.put(rel, bytes, prov)
    }

    /// Writes a payload as pretty-printed JSON with a trailing newline,
    /// unless only sidecars were asked for.
    pub fn json(&self, rel: &str, value: &impl Serialize, prov: &Provenance) -> anyhow::Result<()> {
        if self.meta == MetaMode::Only {
            return Ok(());
        }
        let mut bytes = serde_json::to_vec_pretty(value)?;
        bytes.push(b'\n');
        self.put(rel, &bytes, prov)
    }

    /// Writes the sidecar `<rel_stem>.meta.json`, if sidecars were asked
    /// for.
    pub fn sidecar(
        &self,
        rel_stem: &str,
        value: &impl Serialize,
        prov: &Provenance,
    ) -> anyhow::Result<()> {
        if self.meta == MetaMode::Off {
            return Ok(());
        }
        let mut bytes = serde_json::to_vec_pretty(value)?;
        bytes.push(b'\n');
        self.put(&format!("{rel_stem}.meta.json"), &bytes, prov)
    }

    /// Writes an RGBA image (`width × height × 4` bytes, row by row) as a
    /// PNG.
    ///
    /// Skipped when only sidecars were asked for, and for empty images,
    /// which PNG can't hold.
    pub fn png(
        &self,
        rel: &str,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
        prov: &Provenance,
    ) -> anyhow::Result<()> {
        if self.meta == MetaMode::Only || width == 0 || height == 0 {
            return Ok(());
        }
        let img = image::RgbaImage::from_raw(width, height, rgba).context("image size mismatch")?;
        let mut bytes = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )?;
        self.put(rel, &bytes, prov)
    }

    /// Notes a record that couldn't be written. The run carries on and
    /// ends [`Partial`](crate::cache::Outcome::Partial).
    pub fn fail(&self, record: String, error: impl std::fmt::Display) {
        self.failures.lock().unwrap().push(Failure {
            record,
            error: error.to_string(),
        });
    }

    /// Counts an id that has no record. That's not a failure: indices have
    /// empty slots.
    pub fn absent(&self) {
        self.absent.fetch_add(1, Ordering::Relaxed);
    }

    /// Writes `manifest.json` and returns `(files written, absent,
    /// failures)`.
    ///
    /// Everything's sorted first, so runs are reproducible.
    pub fn finish(self) -> anyhow::Result<(usize, usize, usize)> {
        let mut files = self.files.into_inner().unwrap();
        files.sort_by(|a, b| a.path.cmp(&b.path));
        // A path written twice only gets listed once.
        files.dedup_by(|a, b| a.path == b.path);
        let mut failures = self.failures.into_inner().unwrap();
        failures.sort_by(|a, b| a.record.cmp(&b.record));
        let absent = self.absent.into_inner();
        let manifest = Manifest {
            files: &files,
            absent,
            failures: &failures,
        };
        let mut bytes = serde_json::to_vec_pretty(&manifest)?;
        bytes.push(b'\n');
        fs::write(self.root.join("manifest.json"), bytes)?;
        Ok((files.len(), absent, failures.len()))
    }
}
