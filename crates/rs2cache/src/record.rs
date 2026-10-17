//! Decode any record by type and id, list the ids of a type, and build its
//! metadata sidecar.
//!
//! Ids per type: models, frame packs and MIDI use their index file id; maps
//! the region id `x << 8 | y`; config types the definition id; textures
//! 0–49; fonts 0–3 in [`sprite::FONTS`] order; interfaces the component id;
//! sprites a position in the media archive's sprite entries followed by the
//! title archive's, in stored order, fonts excluded.
//!
//! [`Records`] memoises archives and decoded tables; the free functions are
//! one-shot conveniences over it.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use rs2intermediate::interface::{Content, Interface};
use rs2intermediate::record::{FileMeta, Meta, Record, RecordType, Source, TypeSummary};
use rs2intermediate::sprite::SpriteSheet;

use crate::archive::{self, Archive, Entry, EntryName};
use crate::config::{
    self, ConfigType, flo::Flo, idk::Idk, loc::Loc, npc::Npc, obj::Obj, seq::Seq,
    spotanim::SpotAnim, varbit::Varbit, varp::Varp,
};
use crate::{Cache, CacheError, Error, compress, interface, map, sprite};

type Table = Arc<BTreeMap<u32, Record>>;

/// A cache plus memoised archives and decoded tables, so decoding many
/// records does each expensive step once. `Send + Sync`.
pub struct Records<'c> {
    cache: &'c Cache,
    archives: Mutex<BTreeMap<u8, Arc<Archive>>>,
    tables: Mutex<BTreeMap<RecordType, Table>>,
    map_index: Mutex<Option<Arc<Vec<map::MapIndexEntry>>>>,
}

fn is_table(t: RecordType) -> bool {
    matches!(
        t,
        RecordType::Loc
            | RecordType::Npc
            | RecordType::Obj
            | RecordType::Idk
            | RecordType::Seq
            | RecordType::SpotAnim
            | RecordType::Flo
            | RecordType::Varp
            | RecordType::Varbit
            | RecordType::Interface
    )
}

fn decode_table<T: ConfigType>(
    a: &Archive,
    wrap: fn(T::Def) -> Record,
) -> Result<BTreeMap<u32, Record>, Error> {
    Ok(config::decode_all::<T>(a)?
        .into_iter()
        .enumerate()
        .map(|(i, d)| (i as u32, wrap(d)))
        .collect())
}

impl<'c> Records<'c> {
    pub fn new(cache: &'c Cache) -> Self {
        Self {
            cache,
            archives: Mutex::default(),
            tables: Mutex::default(),
            map_index: Mutex::default(),
        }
    }

    pub fn cache(&self) -> &'c Cache {
        self.cache
    }

    /// Archive `n`, decoded once.
    pub fn archive(&self, n: u8) -> Result<Arc<Archive>, Error> {
        if let Some(a) = self.archives.lock().unwrap().get(&n) {
            return Ok(a.clone());
        }
        let a = Arc::new(self.cache.archive(n)?);
        self.archives.lock().unwrap().insert(n, a.clone());
        Ok(a)
    }

    /// The versionlist's `map_index`, decoded once.
    pub fn map_index(&self) -> Result<Arc<Vec<map::MapIndexEntry>>, Error> {
        if let Some(m) = self.map_index.lock().unwrap().as_ref() {
            return Ok(m.clone());
        }
        let m = Arc::new(self.cache.map_index()?);
        *self.map_index.lock().unwrap() = Some(m.clone());
        Ok(m)
    }

    /// Every record of a table type (config types and interfaces), decoded
    /// once, by id.
    fn table(&self, t: RecordType) -> Result<Table, Error> {
        if let Some(v) = self.tables.lock().unwrap().get(&t) {
            return Ok(v.clone());
        }
        let cfg = || self.archive(archive::CONFIG);
        let v = match t {
            RecordType::Loc => decode_table::<Loc>(&*cfg()?, Record::Loc)?,
            RecordType::Npc => decode_table::<Npc>(&*cfg()?, Record::Npc)?,
            RecordType::Obj => decode_table::<Obj>(&*cfg()?, Record::Obj)?,
            RecordType::Idk => decode_table::<Idk>(&*cfg()?, Record::Idk)?,
            RecordType::Seq => decode_table::<Seq>(&*cfg()?, Record::Seq)?,
            RecordType::SpotAnim => decode_table::<SpotAnim>(&*cfg()?, Record::SpotAnim)?,
            RecordType::Flo => decode_table::<Flo>(&*cfg()?, Record::Flo)?,
            RecordType::Varp => decode_table::<Varp>(&*cfg()?, Record::Varp)?,
            RecordType::Varbit => decode_table::<Varbit>(&*cfg()?, Record::Varbit)?,
            RecordType::Interface => {
                let a = self.archive(archive::INTERFACE)?;
                let data = a.require(archive::INTERFACE, "data")?;
                interface::decode(data)
                    .map_err(Error::Interface)?
                    .1
                    .into_iter()
                    .map(|i: Interface| (u32::from(i.id), Record::Interface(i)))
                    .collect()
            }
            _ => unreachable!("{t} is not a table type"),
        };
        let v = Arc::new(v);
        self.tables.lock().unwrap().insert(t, v.clone());
        Ok(v)
    }

    /// Every valid id of a record type, ascending.
    pub fn ids(&self, t: RecordType) -> Result<Vec<u32>, Error> {
        if is_table(t) {
            return Ok(self.table(t)?.keys().copied().collect());
        }
        Ok(match t {
            RecordType::Model => present(self.cache, 1)?,
            RecordType::Anim => present(self.cache, 2)?,
            RecordType::Midi => present(self.cache, 3)?,
            RecordType::Map => {
                let mut v: Vec<u32> = self.map_index()?.iter().map(|e| e.region.into()).collect();
                v.sort_unstable();
                v
            }
            RecordType::Texture => (0..50).collect(),
            RecordType::Font => (0..sprite::FONTS.len() as u32).collect(),
            RecordType::Sprite => (0..self.sprite_entries()?.len() as u32).collect(),
            _ => unreachable!(),
        })
    }

    fn sprite_entries(&self) -> Result<Vec<(u8, Arc<Archive>, i32)>, Error> {
        let fonts: Vec<i32> = sprite::FONTS
            .iter()
            .map(|f| archive::jag_hash(&format!("{f}.dat")))
            .collect();
        let mut out = Vec::new();
        for n in [archive::MEDIA, archive::TITLE] {
            let a = self.archive(n)?;
            let hashes: Vec<i32> = sprite::sprite_entries(&a)
                .map_err(|e| sprite_error(n, e))?
                .into_iter()
                .map(|(e, _)| e.hash)
                .collect();
            for e in &a.entries {
                if hashes.contains(&e.hash) && !fonts.contains(&e.hash) {
                    out.push((n, a.clone(), e.hash));
                }
            }
        }
        Ok(out)
    }

    /// The archive and entry name of sprite `id`.
    pub fn sprite_source(&self, id: u32) -> Result<(u8, EntryName), Error> {
        let list = self.sprite_entries()?;
        let (n, a, hash) = list
            .get(id as usize)
            .ok_or(missing(RecordType::Sprite, id))?;
        let e = a
            .entries
            .iter()
            .find(|e| e.hash == *hash)
            .expect("listed entry");
        Ok((*n, e.entry_name()))
    }

    /// The `map_index` entry of region `id`.
    pub fn region_entry(&self, id: u32) -> Result<map::MapIndexEntry, Error> {
        self.map_index()?
            .iter()
            .find(|e| u32::from(e.region) == id)
            .copied()
            .ok_or(missing(RecordType::Map, id))
    }

    /// Decode record `id` of type `t`.
    pub fn decode(&self, t: RecordType, id: u32) -> Result<Record, Error> {
        if is_table(t) {
            return self.table(t)?.get(&id).cloned().ok_or(missing(t, id));
        }
        let cache = self.cache;
        Ok(match t {
            RecordType::Model => Record::Model(cache.model(id)?),
            RecordType::Anim => Record::Anim(cache.frame_pack(id)?),
            RecordType::Midi => Record::Midi(cache.midi(id)?),
            RecordType::Map => Record::Map(cache.region_for(&self.region_entry(id)?)?),
            RecordType::Texture => {
                if id >= 50 {
                    return Err(missing(t, id));
                }
                let a = self.archive(archive::TEXTURES)?;
                let hash = archive::jag_hash(&format!("{id}.dat"));
                Record::Texture(sheet_of(&a, archive::TEXTURES, hash)?)
            }
            RecordType::Sprite => {
                let list = self.sprite_entries()?;
                let (n, a, hash) = list.get(id as usize).ok_or(missing(t, id))?;
                Record::Sprite(sheet_of(a, *n, *hash)?)
            }
            RecordType::Font => {
                let name = sprite::FONTS.get(id as usize).ok_or(missing(t, id))?;
                let a = self.archive(archive::TITLE)?;
                let font =
                    sprite::decode_font(&a, name).map_err(|e| sprite_error(archive::TITLE, e))?;
                Record::Font(font)
            }
            _ => unreachable!(),
        })
    }

    /// The archive and entries an archive-backed record lives in.
    fn archive_entries(&self, t: RecordType, id: u32) -> Result<(u8, Vec<EntryName>), Error> {
        let named = |s: &str| EntryName::Named(s.to_owned());
        let cfg = |dat: &str, idx: Option<&str>| {
            let mut v = vec![named(dat)];
            v.extend(idx.map(named));
            (archive::CONFIG, v)
        };
        Ok(match t {
            RecordType::Loc => cfg(Loc::DAT, Loc::IDX),
            RecordType::Npc => cfg(Npc::DAT, Npc::IDX),
            RecordType::Obj => cfg(Obj::DAT, Obj::IDX),
            RecordType::Idk => cfg(Idk::DAT, None),
            RecordType::Seq => cfg(Seq::DAT, None),
            RecordType::SpotAnim => cfg(SpotAnim::DAT, None),
            RecordType::Flo => cfg(Flo::DAT, None),
            RecordType::Varp => cfg(Varp::DAT, None),
            RecordType::Varbit => cfg(Varbit::DAT, None),
            RecordType::Texture => (
                archive::TEXTURES,
                vec![named(&format!("{id}.dat")), named("index.dat")],
            ),
            RecordType::Font => {
                let name = sprite::FONTS.get(id as usize).ok_or(missing(t, id))?;
                (
                    archive::TITLE,
                    vec![named(&format!("{name}.dat")), named("index.dat")],
                )
            }
            RecordType::Sprite => {
                let (n, entry) = self.sprite_source(id)?;
                (n, vec![entry, named("index.dat")])
            }
            RecordType::Interface => (archive::INTERFACE, vec![named("data")]),
            _ => unreachable!("{t} is not archive-backed"),
        })
    }

    /// The stored files record `id` of type `t` was decoded from.
    pub fn files(&self, t: RecordType, id: u32) -> Result<Vec<FileMeta>, Error> {
        let cache = self.cache;
        Ok(match t {
            RecordType::Model => vec![index_file(cache, 1, id)?],
            RecordType::Anim => vec![index_file(cache, 2, id)?],
            RecordType::Midi => vec![index_file(cache, 3, id)?],
            RecordType::Map => {
                let e = self.region_entry(id)?;
                vec![
                    index_file(cache, 4, e.tile_file.into())?,
                    index_file(cache, 4, e.location_file.into())?,
                ]
            }
            _ => {
                let (n, names) = self.archive_entries(t, id)?;
                let a = self.archive(n)?;
                names
                    .iter()
                    .map(|name| {
                        let e = a.entry(name).ok_or_else(|| Error::MissingEntry {
                            archive: n,
                            entry: name.clone(),
                        })?;
                        Ok(entry_file(n, &a, e))
                    })
                    .collect::<Result<_, Error>>()?
            }
        })
    }

    /// The metadata sidecar for record `id` of type `t`.
    pub fn meta(&self, t: RecordType, id: u32) -> Result<Meta, Error> {
        let record = self.decode(t, id)?;
        Ok(Meta {
            record_type: t,
            id,
            files: self.files(t, id)?,
            summary: summarize(&record),
        })
    }
}

fn sprite_error(archive: u8, source: sprite::SpriteError) -> Error {
    Error::Sprite { archive, source }
}

fn missing(kind: RecordType, id: u32) -> Error {
    Error::NoRecord { kind, id }
}

fn present(cache: &Cache, index: u8) -> Result<Vec<u32>, Error> {
    Ok(cache
        .entries(index)?
        .iter()
        .enumerate()
        .filter(|(_, e)| !e.is_absent())
        .map(|(i, _)| i as u32)
        .collect())
}

fn sheet_of(a: &Archive, n: u8, hash: i32) -> Result<SpriteSheet, Error> {
    let index = a
        .get("index.dat")
        .ok_or(sprite_error(n, sprite::SpriteError::NoIndex))?;
    let (e, end) = sprite::sprite_entries(a)
        .map_err(|e| sprite_error(n, e))?
        .into_iter()
        .find(|(e, _)| e.hash == hash)
        .ok_or(Error::MissingEntry {
            archive: n,
            entry: EntryName::Hash(hash),
        })?;
    sprite::decode_block(&e.data, index, end).map_err(|e| sprite_error(n, e))
}

/// Metadata of one file of index 0–4.
pub fn index_file(cache: &Cache, index: u8, id: u32) -> Result<FileMeta, Error> {
    let (raw, chain) = cache.read_raw_with_chain(index, id)?;
    let gz = compress::is_gzip(&raw);
    let inflated =
        compress::inflate(&raw).map_err(|source| Error::Compress { index, id, source })?;
    Ok(FileMeta {
        source: Source::Index { index, id },
        sector_chain: chain.iter().map(|v| v.sector).collect(),
        raw_size: raw.len() as u64,
        inflated_size: inflated.len() as u64,
        crc32: crc32fast::hash(&raw),
        compression: if gz { "gzip" } else { "none" }.into(),
    })
}

/// Metadata of one archive entry.
pub fn entry_file(n: u8, a: &Archive, e: &Entry) -> FileMeta {
    FileMeta {
        source: Source::Archive {
            archive: n,
            entry: e.entry_name().to_string(),
        },
        sector_chain: Vec::new(),
        raw_size: e.compressed_size.into(),
        inflated_size: e.uncompressed_size.into(),
        crc32: crc32fast::hash(&e.data),
        compression: if e.compressed {
            "bzip2"
        } else if a.whole_compressed {
            "bzip2-archive"
        } else {
            "none"
        }
        .into(),
    }
}

fn distinct(labels: &Option<Vec<u8>>) -> usize {
    labels
        .as_ref()
        .map_or(0, |l| l.iter().collect::<BTreeSet<_>>().len())
}

/// Summarise a decoded record. Cross-reference fields are left empty.
pub fn summarize(record: &Record) -> TypeSummary {
    match record {
        Record::Model(m) => TypeSummary::Model {
            vertex_count: m.vertices.len(),
            face_count: m.faces.len(),
            textured_face_count: m.textured_faces.len(),
            vertex_label_count: distinct(&m.vertex_labels),
            face_label_count: distinct(&m.face_labels),
            bounds: m.bounds(),
            textures: None,
        },
        Record::Anim(p) => TypeSummary::Anim {
            frame_ids: p.frames.iter().map(|f| f.id).collect(),
            transform_count: p.base.len(),
            labels: p
                .base
                .iter()
                .flat_map(|b| b.labels.iter().copied())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        },
        Record::Map(r) => TypeSummary::Map {
            region: r.id,
            members: r.members,
            height_range: r
                .planes
                .iter()
                .map(|p| {
                    let lo = p.heights.iter().copied().min().unwrap_or(0);
                    let hi = p.heights.iter().copied().max().unwrap_or(0);
                    (lo, hi)
                })
                .collect(),
            object_count: r.locations.len(),
        },
        Record::Texture(s) | Record::Sprite(s) => TypeSummary::Sprite {
            width: s.max_width,
            height: s.max_height,
            palette_size: s.palette.len(),
            sub_sprites: s.sprites.len(),
            used_by_models: None,
        },
        Record::Font(f) => TypeSummary::Font {
            glyphs: f.glyphs.sprites.len(),
            max_height: f.glyphs.max_height,
        },
        Record::Interface(i) => TypeSummary::Interface {
            kind: i.kind,
            width: i.width,
            height: i.height,
            children: match &i.content {
                Content::Layer { children, .. } => children.len(),
                _ => 0,
            },
        },
        Record::Midi(m) => TypeSummary::Midi { size: m.len() },
        config => TypeSummary::Config {
            definition: Box::new(config.clone()),
        },
    }
}

/// Every valid id of a record type. For many calls, use [`Records`].
pub fn ids(cache: &Cache, t: RecordType) -> Result<Vec<u32>, Error> {
    Records::new(cache).ids(t)
}

/// Decode one record. For many records, use [`Records`].
pub fn decode_any(cache: &Cache, t: RecordType, id: u32) -> Result<Record, Error> {
    Records::new(cache).decode(t, id)
}

/// The metadata sidecar for one record. For many records, use [`Records`].
pub fn meta(cache: &Cache, t: RecordType, id: u32) -> Result<Meta, Error> {
    Records::new(cache).meta(t, id)
}

/// Whether `err` means the record simply is not there (an absent file).
pub fn is_absent(err: &Error) -> bool {
    matches!(
        err,
        Error::Cache(CacheError::Absent { .. }) | Error::NoRecord { .. }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_is_send_and_sync() {
        fn check<T: Send + Sync>() {}
        check::<Records<'static>>();
    }
}
