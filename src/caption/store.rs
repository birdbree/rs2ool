//! Captions kept on disk: one JSON file per cache, in the platform data
//! directory, never in the repository.
//!
//! ```text
//! <data dir>/captions/<cache crc, 8 hex digits>.json
//! { "cache_crc": <n>, "captions": [ { "npc", "seq", "model", "frames", "text", "class", "reply" }, … ] }
//! ```
//!
//! Captions are keyed by NPC, sequence and vision model and written sorted
//! by that key. The file is rewritten after every caption, through a
//! temporary file, so stopping a run part way keeps what it had.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// One caption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The NPC whose model the sequence was drawn on.
    pub npc: u32,
    /// The sequence.
    pub seq: u32,
    /// The vision model that wrote it.
    pub model: String,
    /// How many frames the image showed.
    pub frames: u32,
    /// The description.
    pub text: String,
    /// The class it picked from [`super::CLASSES`], if it picked one.
    pub class: Option<String>,
    /// The whole reply, as it came.
    pub reply: String,
}

#[derive(Serialize, Deserialize)]
struct File {
    cache_crc: u32,
    captions: Vec<Entry>,
}

/// The captions for one cache.
pub struct Store {
    path: PathBuf,
    cache_crc: u32,
    entries: BTreeMap<(u32, u32, String), Entry>,
}

impl Store {
    /// The captions for the cache with `cache_crc`, kept under `dir`
    /// (normally the platform data directory). Empty if there's no file
    /// yet.
    pub fn open(dir: &Path, cache_crc: u32) -> Result<Self> {
        let path = dir.join("captions").join(format!("{cache_crc:08x}.json"));
        let entries = match std::fs::read(&path) {
            Ok(bytes) => {
                let f: File = serde_json::from_slice(&bytes)
                    .with_context(|| format!("reading {}", path.display()))?;
                f.captions
                    .into_iter()
                    .map(|e| ((e.npc, e.seq, e.model.clone()), e))
                    .collect()
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        };
        Ok(Self {
            path,
            cache_crc,
            entries,
        })
    }

    /// Where the file is.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every caption, sorted by NPC, sequence and model.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values()
    }

    /// Whether `model` has captioned sequence `seq` on NPC `npc`.
    pub fn has(&self, npc: u32, seq: u32, model: &str) -> bool {
        self.entries.contains_key(&(npc, seq, model.to_owned()))
    }

    /// Whether `model` has captioned sequence `seq` on any NPC.
    pub fn has_seq(&self, seq: u32, model: &str) -> bool {
        self.entries
            .values()
            .any(|e| e.seq == seq && e.model == model)
    }

    /// Adds a caption, replacing any with the same key.
    pub fn insert(&mut self, e: Entry) {
        self.entries.insert((e.npc, e.seq, e.model.clone()), e);
    }

    /// Writes the file, making its directory if needed.
    pub fn save(&self) -> Result<()> {
        let dir = self.path.parent().expect("the path has a directory");
        std::fs::create_dir_all(dir)?;
        let file = File {
            cache_crc: self.cache_crc,
            captions: self.entries.values().cloned().collect(),
        };
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(&file)?)?;
        std::fs::rename(&tmp, &self.path)
            .with_context(|| format!("writing {}", self.path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(npc: u32, seq: u32) -> Entry {
        Entry {
            npc,
            seq,
            model: "m".into(),
            frames: 6,
            text: "falls over".into(),
            class: Some("fall over".into()),
            reply: "falls over\nfall over".into(),
        }
    }

    #[test]
    fn round_trips_sorted() {
        let dir = std::env::temp_dir().join(format!("rs2ool-captions-{}", std::process::id()));
        let mut s = Store::open(&dir, 0xabc).unwrap();
        assert!(s.path().ends_with("captions/00000abc.json"));
        s.insert(entry(5, 9));
        s.insert(entry(1, 9));
        s.insert(entry(1, 2));
        s.save().unwrap();
        let again = Store::open(&dir, 0xabc).unwrap();
        let keys: Vec<_> = again.entries().map(|e| (e.npc, e.seq)).collect();
        assert_eq!(keys, [(1, 2), (1, 9), (5, 9)]);
        assert!(again.has(1, 9, "m") && !again.has(1, 9, "other"));
        assert!(again.has_seq(9, "m") && !again.has_seq(3, "m"));
        // Another cache's captions are a different file.
        assert_eq!(Store::open(&dir, 1).unwrap().entries().count(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }
}
