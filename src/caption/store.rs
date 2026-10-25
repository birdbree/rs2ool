//! Captions kept on disk: one JSON file per cache, in the platform data
//! directory, never in the repository.
//!
//! ```text
//! <data dir>/captions/<cache crc, 8 hex digits>.json
//! { "cache_crc": <n>, "captions": [ { "npc", "seq", "model", "frames", "text", "class", "reply", "review"? }, … ] }
//! ```
//!
//! Captions are keyed by NPC, sequence and vision model and written sorted
//! by that key. The file is rewritten after every caption and every
//! review, through a temporary file, so stopping a run part way keeps what
//! it had.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

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
    /// What someone checking it by eye made of it, once they have.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<Review>,
}

/// A caption checked by eye.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Review {
    /// Whether the caption's class is right.
    pub right: bool,
    /// When it's wrong, the right class, if the reviewer gave one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
}

impl Entry {
    /// The class someone checked: the caption's own when it was marked
    /// right, the corrected one when it was marked wrong. `None` when it
    /// hasn't been reviewed, or was marked wrong with no class given.
    pub fn reviewed_class(&self) -> Option<&str> {
        match &self.review {
            Some(Review { right: true, .. }) => self.class.as_deref(),
            Some(Review {
                right: false,
                class,
            }) => class.as_deref(),
            None => None,
        }
    }
}

/// Locks a shared store. A thread that panicked holding it can't have left
/// it half-changed (every change is one map operation), so a poisoned lock
/// is taken anyway.
pub fn lock(store: &Mutex<Store>) -> MutexGuard<'_, Store> {
    store.lock().unwrap_or_else(|e| e.into_inner())
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
    // The keys again, by sequence, for looking a sequence up on any NPC.
    by_seq: BTreeMap<u32, BTreeSet<(u32, String)>>,
}

impl Store {
    /// The captions for the cache with `cache_crc`, kept under `dir`
    /// (normally the platform data directory). Empty if there's no file
    /// yet.
    pub fn open(dir: &Path, cache_crc: u32) -> Result<Self> {
        let path = dir.join("captions").join(format!("{cache_crc:08x}.json"));
        let entries: BTreeMap<(u32, u32, String), Entry> = match std::fs::read(&path) {
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
        let mut by_seq: BTreeMap<u32, BTreeSet<(u32, String)>> = BTreeMap::new();
        for (npc, seq, model) in entries.keys() {
            by_seq
                .entry(*seq)
                .or_default()
                .insert((*npc, model.clone()));
        }
        Ok(Self {
            path,
            cache_crc,
            entries,
            by_seq,
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
        self.by_seq
            .get(&seq)
            .is_some_and(|keys| keys.iter().any(|(_, m)| m == model))
    }

    /// The caption to show for sequence `seq` on NPC `npc`: one drawn on
    /// that NPC, by `model` if there is one, or else one drawn on another
    /// NPC (as `--all` captions each sequence once).
    pub fn lookup(&self, npc: u32, seq: u32, model: &str) -> Option<&Entry> {
        let keys = self.by_seq.get(&seq)?;
        let key = keys
            .iter()
            .find(|(n, m)| *n == npc && m == model)
            .or_else(|| keys.iter().find(|(n, _)| *n == npc))
            .or_else(|| keys.iter().find(|(_, m)| m == model))
            .or_else(|| keys.first())?;
        self.entries.get(&(key.0, seq, key.1.clone()))
    }

    /// Sets or clears the review of a caption; `false` if there's no such
    /// caption.
    pub fn review(&mut self, npc: u32, seq: u32, model: &str, review: Option<Review>) -> bool {
        match self.entries.get_mut(&(npc, seq, model.to_owned())) {
            Some(e) => {
                e.review = review;
                true
            }
            None => false,
        }
    }

    /// Adds a caption, replacing any with the same key, review and all.
    pub fn insert(&mut self, e: Entry) {
        self.by_seq
            .entry(e.seq)
            .or_default()
            .insert((e.npc, e.model.clone()));
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
            review: None,
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
        // Reviews are kept.
        let mut again = again;
        let wrong = Review {
            right: false,
            class: Some("death".into()),
        };
        assert!(again.review(1, 9, "m", Some(wrong.clone())));
        assert!(!again.review(1, 9, "other", None));
        again.save().unwrap();
        let back = Store::open(&dir, 0xabc).unwrap();
        let e = back.lookup(1, 9, "m").unwrap();
        assert_eq!(e.review, Some(wrong));
        assert_eq!(e.reviewed_class(), Some("death"));
        // Another cache's captions are a different file.
        assert_eq!(Store::open(&dir, 1).unwrap().entries().count(), 0);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn lookup_prefers_this_npc_then_this_model() {
        let dir = std::env::temp_dir().join(format!("rs2ool-lookup-{}", std::process::id()));
        let mut s = Store::open(&dir, 1).unwrap();
        let by = |npc, seq, model: &str| Entry {
            model: model.into(),
            ..entry(npc, seq)
        };
        s.insert(by(1, 5, "a"));
        s.insert(by(2, 5, "b"));
        s.insert(by(3, 5, "c"));
        let found = |npc, model| s.lookup(npc, 5, model).map(|e| (e.npc, e.model.clone()));
        assert_eq!(found(2, "a"), Some((2, "b".into())), "this NPC, any model");
        assert_eq!(
            found(9, "c"),
            Some((3, "c".into())),
            "another NPC, this model"
        );
        assert_eq!(
            found(9, "z"),
            Some((1, "a".into())),
            "another NPC, any model"
        );
        assert!(s.lookup(1, 6, "a").is_none());
        // Marked right, the caption's own class counts; unreviewed, none.
        let mut e = entry(1, 1);
        assert_eq!(e.reviewed_class(), None);
        e.review = Some(Review {
            right: true,
            class: None,
        });
        assert_eq!(e.reviewed_class(), Some("fall over"));
    }
}
