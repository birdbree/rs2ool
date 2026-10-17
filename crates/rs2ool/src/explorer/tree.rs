//! The data behind the left-hand tree, kept apart from egui so it can be
//! tested. Lists of ids (with names) load on the compute pool when a node
//! is first expanded.

use std::collections::BTreeMap;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use rs2cache::types::record::{Record, RecordType};

use crate::explorer::sources::Source;

/// One index of a cache with its file count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexRow {
    pub index: u8,
    pub files: usize,
    pub present: usize,
}

/// The indices of a cache source.
pub fn index_rows(source: &Source) -> Vec<IndexRow> {
    let Source::Cache { records, .. } = source else {
        return Vec::new();
    };
    let cache = records.cache();
    (0..cache.index_count())
        .filter_map(|i| {
            let e = cache.entries(i).ok()?;
            Some(IndexRow {
                index: i,
                files: e.len(),
                present: e.iter().filter(|x| !x.is_absent()).count(),
            })
        })
        .collect()
}

/// One row of a list: an id and a display name (empty when it has none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: u32,
    pub name: String,
}

impl Row {
    pub fn label(&self) -> String {
        if self.name.is_empty() {
            self.id.to_string()
        } else {
            format!("{} {}", self.id, self.name)
        }
    }

    /// Whether `filter` matches the id exactly or the name, ignoring case.
    pub fn matches(&self, filter: &str) -> bool {
        let f = filter.trim();
        f.is_empty()
            || f.parse::<u32>().is_ok_and(|n| n == self.id)
            || self.name.to_lowercase().contains(&f.to_lowercase())
    }
}

/// Which list: a record type of a cache, the records of a pack, or the
/// files of a dump directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ListKey {
    Type(usize, RecordType),
    Pack(usize),
    Dump(usize),
}

/// The rows of a cache record type, named where the type has names.
pub fn type_rows(source: &Source, t: RecordType) -> Result<Vec<Row>, String> {
    let Source::Cache { records, .. } = source else {
        return Err("not a cache".into());
    };
    let ids = records.ids(t).map_err(|e| e.to_string())?;
    ids.into_iter()
        .map(|id| {
            let name = match t {
                RecordType::Loc | RecordType::Npc | RecordType::Obj => {
                    match records.decode(t, id).map_err(|e| e.to_string())? {
                        Record::Loc(d) => d.name.unwrap_or_default(),
                        Record::Npc(d) => d.name.unwrap_or_default(),
                        Record::Obj(d) => d.name.unwrap_or_default(),
                        _ => String::new(),
                    }
                }
                RecordType::Sprite => records
                    .sprite_source(id)
                    .map(|(_, e)| e.to_string())
                    .unwrap_or_default(),
                RecordType::Font => rs2cache::sprite::FONTS
                    .get(id as usize)
                    .map(|s| s.to_string())
                    .unwrap_or_default(),
                RecordType::Map => format!("{}_{}", id >> 8, id & 0xff),
                _ => String::new(),
            };
            Ok(Row { id, name })
        })
        .collect()
}

/// The rows of a pack: its record ids.
pub fn pack_rows(source: &Source) -> Vec<Row> {
    let Source::Pack { reader, .. } = source else {
        return Vec::new();
    };
    reader
        .ids()
        .map(|id| Row {
            id,
            name: String::new(),
        })
        .collect()
}

/// The files of a dump directory, from its manifest, as rows numbered by
/// position.
pub fn dump_rows(source: &Source) -> Result<Vec<(Row, std::path::PathBuf)>, String> {
    let Source::DumpDir { path } = source else {
        return Err("not a dump directory".into());
    };
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(path.join("manifest.json")).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    Ok(manifest["files"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|f| f["path"].as_str())
                .map(std::path::PathBuf::from)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            (
                Row {
                    id: i as u32,
                    name: p.display().to_string(),
                },
                p,
            )
        })
        .collect())
}

type RowsTask = Task<Result<Vec<Row>, String>>;

/// Loaded and loading lists, and the search text per list.
#[derive(Resource, Default)]
pub struct Lists {
    pub rows: BTreeMap<ListKey, Result<Vec<Row>, String>>,
    pub search: BTreeMap<ListKey, String>,
    pending: BTreeMap<ListKey, RowsTask>,
}

impl Lists {
    /// Start loading a list if it is neither loaded nor loading.
    pub fn request(&mut self, key: ListKey, source: &Source) {
        if self.rows.contains_key(&key) || self.pending.contains_key(&key) {
            return;
        }
        let task = match (key, source) {
            (ListKey::Type(_, t), Source::Cache { path, records }) => {
                let s = Source::Cache {
                    path: path.clone(),
                    records: records.clone(),
                };
                AsyncComputeTaskPool::get().spawn(async move { type_rows(&s, t) })
            }
            (ListKey::Pack(_), Source::Pack { path, reader }) => {
                let s = Source::Pack {
                    path: path.clone(),
                    reader: reader.clone(),
                };
                AsyncComputeTaskPool::get().spawn(async move { Ok(pack_rows(&s)) })
            }
            (ListKey::Dump(_), Source::DumpDir { path }) => {
                let s = Source::DumpDir { path: path.clone() };
                AsyncComputeTaskPool::get().spawn(async move {
                    dump_rows(&s).map(|v| v.into_iter().map(|(r, _)| r).collect())
                })
            }
            _ => return,
        };
        self.pending.insert(key, task);
    }

    pub fn loading(&self, key: &ListKey) -> bool {
        self.pending.contains_key(key)
    }
}

fn poll(mut lists: ResMut<Lists>) {
    let done: Vec<ListKey> = lists.pending.keys().copied().collect();
    for key in done {
        let finished = lists
            .pending
            .get_mut(&key)
            .and_then(|t| block_on(poll_once(t)));
        if let Some(r) = finished {
            lists.pending.remove(&key);
            lists.rows.insert(key, r);
        }
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Lists>().add_systems(Update, poll);
}
