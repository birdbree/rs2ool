//! The output directory: files, sidecars, and `manifest.json`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::Context;
use rs2cache::types::record::{Meta, Source};
use serde::Serialize;

/// One written file.
#[derive(Debug, Clone, Serialize)]
pub struct ManifestEntry {
    /// Relative to the output directory, `/`-separated.
    pub path: String,
    pub sources: Vec<Source>,
    /// Stored size of the source(s).
    pub raw_size: u64,
    /// Inflated size of the source(s).
    pub inflated_size: u64,
    /// CRC32 of the written file.
    pub crc32: u32,
}

/// A record that could not be written.
#[derive(Debug, Clone, Serialize)]
pub struct Failure {
    pub record: String,
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
    Off,
    With,
    Only,
}

pub struct Output {
    root: PathBuf,
    pub meta: MetaMode,
    files: Mutex<Vec<ManifestEntry>>,
    failures: Mutex<Vec<Failure>>,
    absent: AtomicUsize,
}

/// Sizes and sources of what a written file came from.
#[derive(Debug, Clone, Default)]
pub struct Provenance {
    pub sources: Vec<Source>,
    pub raw_size: u64,
    pub inflated_size: u64,
}

impl Provenance {
    pub fn from_meta(m: &Meta) -> Self {
        Self {
            sources: m.files.iter().map(|f| f.source.clone()).collect(),
            raw_size: m.files.iter().map(|f| f.raw_size).sum(),
            inflated_size: m.files.iter().map(|f| f.inflated_size).sum(),
        }
    }
}

impl Output {
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

    pub fn root(&self) -> &Path {
        &self.root
    }

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

    /// Write a payload, unless only sidecars were asked for.
    pub fn payload(&self, rel: &str, bytes: &[u8], prov: &Provenance) -> anyhow::Result<()> {
        if self.meta == MetaMode::Only {
            return Ok(());
        }
        self.put(rel, bytes, prov)
    }

    /// Write a JSON payload (pretty-printed).
    pub fn json(&self, rel: &str, value: &impl Serialize, prov: &Provenance) -> anyhow::Result<()> {
        if self.meta == MetaMode::Only {
            return Ok(());
        }
        let mut bytes = serde_json::to_vec_pretty(value)?;
        bytes.push(b'\n');
        self.put(rel, &bytes, prov)
    }

    /// Write a sidecar `<stem>.meta.json`, when sidecars were asked for.
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

    /// Write an RGBA image as PNG.
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

    pub fn fail(&self, record: String, error: impl std::fmt::Display) {
        self.failures.lock().unwrap().push(Failure {
            record,
            error: error.to_string(),
        });
    }

    pub fn absent(&self) {
        self.absent.fetch_add(1, Ordering::Relaxed);
    }

    /// Write `manifest.json` (sorted, so runs are reproducible) and return
    /// `(files written, absent, failures)`.
    pub fn finish(self) -> anyhow::Result<(usize, usize, usize)> {
        let mut files = self.files.into_inner().unwrap();
        files.sort_by(|a, b| a.path.cmp(&b.path));
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
