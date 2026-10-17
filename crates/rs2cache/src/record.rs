//! Decode any record by type and id, list the ids of a type, and build its
//! metadata sidecar.
//!
//! Ids per type: models, frame packs and MIDI use their index file id; maps
//! the region id `x << 8 | y`; config types the definition id; textures
//! 0–49; fonts 0–3 in [`sprite::FONTS`] order; interfaces the component id;
//! sprites a position in the media archive's sprite entries followed by the
//! title archive's, in stored order.

use std::collections::BTreeSet;

use rs2intermediate::record::{FileMeta, Meta, Record, RecordType, Source, TypeSummary};

use crate::archive::{self, Archive, Entry};
use crate::config::{
    self, ConfigType, flo::Flo, idk::Idk, loc::Loc, npc::Npc, obj::Obj, seq::Seq,
    spotanim::SpotAnim, varbit::Varbit, varp::Varp,
};
use crate::{Cache, CacheError, Error, compress, interface, map, sprite};

/// A sprite entry: which archive, and the entry.
fn sprite_entries(cache: &Cache) -> Result<Vec<(u8, Archive, i32)>, Error> {
    let mut out = Vec::new();
    for n in [archive::MEDIA, archive::TITLE] {
        let a = cache.archive(n)?;
        let hashes: Vec<i32> = sprite::sprite_entries(&a)
            .map_err(|e| sprite_error(n, e))?
            .into_iter()
            .map(|(e, _)| e.hash)
            .collect();
        // Keep the archive's stored order, skipping fonts (their own type).
        let fonts: Vec<i32> = sprite::FONTS
            .iter()
            .map(|f| archive::jag_hash(&format!("{f}.dat")))
            .collect();
        for e in &a.entries {
            if hashes.contains(&e.hash) && !fonts.contains(&e.hash) {
                out.push((n, a.clone(), e.hash));
            }
        }
    }
    Ok(out)
}

fn sprite_error(archive: u8, e: sprite::SpriteError) -> Error {
    Error::Sprite { archive, source: e }
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

/// Every valid id of a record type, ascending.
pub fn ids(cache: &Cache, t: RecordType) -> Result<Vec<u32>, Error> {
    let config_count = |n: usize| (0..n as u32).collect::<Vec<_>>();
    let cfg = || cache.archive(archive::CONFIG);
    Ok(match t {
        RecordType::Model => present(cache, 1)?,
        RecordType::Anim => present(cache, 2)?,
        RecordType::Midi => present(cache, 3)?,
        RecordType::Map => {
            let mut v: Vec<u32> = cache.map_index()?.iter().map(|e| e.region.into()).collect();
            v.sort_unstable();
            v
        }
        RecordType::Loc => config_count(config::count::<Loc>(&cfg()?)?),
        RecordType::Npc => config_count(config::count::<Npc>(&cfg()?)?),
        RecordType::Obj => config_count(config::count::<Obj>(&cfg()?)?),
        RecordType::Idk => config_count(config::count::<Idk>(&cfg()?)?),
        RecordType::Seq => config_count(config::count::<Seq>(&cfg()?)?),
        RecordType::SpotAnim => config_count(config::count::<SpotAnim>(&cfg()?)?),
        RecordType::Flo => config_count(config::count::<Flo>(&cfg()?)?),
        RecordType::Varp => config_count(config::count::<Varp>(&cfg()?)?),
        RecordType::Varbit => config_count(config::count::<Varbit>(&cfg()?)?),
        RecordType::Texture => (0..50).collect(),
        RecordType::Font => (0..sprite::FONTS.len() as u32).collect(),
        RecordType::Sprite => (0..sprite_entries(cache)?.len() as u32).collect(),
        RecordType::Interface => {
            let set: BTreeSet<u32> = interfaces(cache)?.iter().map(|i| u32::from(i.id)).collect();
            set.into_iter().collect()
        }
    })
}

fn interfaces(cache: &Cache) -> Result<Vec<rs2intermediate::interface::Interface>, Error> {
    let a = cache.archive(archive::INTERFACE)?;
    let data = a.require(archive::INTERFACE, "data")?;
    Ok(interface::decode(data).map_err(Error::Interface)?.1)
}

fn missing(t: RecordType, id: u32) -> Error {
    Error::NoRecord { kind: t, id }
}

fn config_one<T: ConfigType>(cache: &Cache, id: u32) -> Result<T::Def, Error> {
    Ok(config::decode_one::<T>(
        &cache.archive(archive::CONFIG)?,
        id,
    )?)
}

fn region_entry(cache: &Cache, id: u32) -> Result<map::MapIndexEntry, Error> {
    cache
        .map_index()?
        .into_iter()
        .find(|e| u32::from(e.region) == id)
        .ok_or(missing(RecordType::Map, id))
}

fn sheet_of(a: &Archive, n: u8, hash: i32) -> Result<rs2intermediate::sprite::SpriteSheet, Error> {
    let index = a
        .get("index.dat")
        .ok_or(sprite_error(n, sprite::SpriteError::NoIndex))?;
    let (e, end) = sprite::sprite_entries(a)
        .map_err(|e| sprite_error(n, e))?
        .into_iter()
        .find(|(e, _)| e.hash == hash)
        .ok_or(Error::MissingEntry {
            archive: n,
            entry: archive::EntryName::Hash(hash),
        })?;
    sprite::decode_block(&e.data, index, end).map_err(|e| sprite_error(n, e))
}

/// Decode record `id` of type `t`.
pub fn decode_any(cache: &Cache, t: RecordType, id: u32) -> Result<Record, Error> {
    Ok(match t {
        RecordType::Model => Record::Model(cache.model(id)?),
        RecordType::Anim => Record::Anim(cache.frame_pack(id)?),
        RecordType::Midi => Record::Midi(cache.midi(id)?),
        RecordType::Map => Record::Map(cache.region_for(&region_entry(cache, id)?)?),
        RecordType::Loc => Record::Loc(config_one::<Loc>(cache, id)?),
        RecordType::Npc => Record::Npc(config_one::<Npc>(cache, id)?),
        RecordType::Obj => Record::Obj(config_one::<Obj>(cache, id)?),
        RecordType::Idk => Record::Idk(config_one::<Idk>(cache, id)?),
        RecordType::Seq => Record::Seq(config_one::<Seq>(cache, id)?),
        RecordType::SpotAnim => Record::SpotAnim(config_one::<SpotAnim>(cache, id)?),
        RecordType::Flo => Record::Flo(config_one::<Flo>(cache, id)?),
        RecordType::Varp => Record::Varp(config_one::<Varp>(cache, id)?),
        RecordType::Varbit => Record::Varbit(config_one::<Varbit>(cache, id)?),
        RecordType::Texture => {
            if id >= 50 {
                return Err(missing(t, id));
            }
            let a = cache.archive(archive::TEXTURES)?;
            Record::Texture(sheet_of(
                &a,
                archive::TEXTURES,
                archive::jag_hash(&format!("{id}.dat")),
            )?)
        }
        RecordType::Sprite => {
            let list = sprite_entries(cache)?;
            let (n, a, hash) = list.get(id as usize).ok_or(missing(t, id))?;
            Record::Sprite(sheet_of(a, *n, *hash)?)
        }
        RecordType::Font => {
            let name = sprite::FONTS.get(id as usize).ok_or(missing(t, id))?;
            let a = cache.archive(archive::TITLE)?;
            Record::Font(
                sprite::decode_font(&a, name).map_err(|e| sprite_error(archive::TITLE, e))?,
            )
        }
        RecordType::Interface => {
            let id16 = u16::try_from(id).map_err(|_| missing(t, id))?;
            Record::Interface(
                interfaces(cache)?
                    .into_iter()
                    .find(|i| i.id == id16)
                    .ok_or(missing(t, id))?,
            )
        }
    })
}

/// The archive entry and archive number a record type's data lives in, for
/// archive-backed types.
fn archive_entries(
    cache: &Cache,
    t: RecordType,
    id: u32,
) -> Result<Option<(u8, Vec<String>)>, Error> {
    let cfg = |dat: &str, idx: Option<&str>| {
        let mut v = vec![dat.to_owned()];
        v.extend(idx.map(str::to_owned));
        Some((archive::CONFIG, v))
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
        RecordType::Texture => Some((
            archive::TEXTURES,
            vec![format!("{id}.dat"), "index.dat".into()],
        )),
        RecordType::Font => {
            let name = sprite::FONTS.get(id as usize).ok_or(missing(t, id))?;
            Some((
                archive::TITLE,
                vec![format!("{name}.dat"), "index.dat".into()],
            ))
        }
        RecordType::Sprite => {
            let list = sprite_entries(cache)?;
            let (n, a, hash) = list.get(id as usize).ok_or(missing(t, id))?;
            let e = a
                .entries
                .iter()
                .find(|e| e.hash == *hash)
                .expect("listed entry");
            Some((*n, vec![e.entry_name().to_string(), "index.dat".into()]))
        }
        RecordType::Interface => Some((archive::INTERFACE, vec!["data".into()])),
        _ => None,
    })
}

fn index_file(cache: &Cache, index: u8, id: u32) -> Result<FileMeta, Error> {
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

fn entry_file(n: u8, a: &Archive, e: &Entry) -> FileMeta {
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

/// Summarise a decoded record. Cross-reference fields are left empty.
pub fn summarize(record: &Record) -> TypeSummary {
    match record {
        Record::Model(m) => TypeSummary::Model {
            vertex_count: m.vertices.len(),
            face_count: m.faces.len(),
            textured_face_count: m.textured_faces.len(),
            vertex_label_count: m
                .vertex_labels
                .as_ref()
                .map_or(0, |l| l.iter().collect::<BTreeSet<_>>().len()),
            face_label_count: m
                .face_labels
                .as_ref()
                .map_or(0, |l| l.iter().collect::<BTreeSet<_>>().len()),
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
                rs2intermediate::interface::Content::Layer { children, .. } => children.len(),
                _ => 0,
            },
        },
        Record::Midi(m) => TypeSummary::Midi { size: m.len() },
        config => TypeSummary::Config {
            definition: Box::new(config.clone()),
        },
    }
}

/// The metadata sidecar for record `id` of type `t`.
pub fn meta(cache: &Cache, t: RecordType, id: u32) -> Result<Meta, Error> {
    let record = decode_any(cache, t, id)?;
    let files = match t {
        RecordType::Model => vec![index_file(cache, 1, id)?],
        RecordType::Anim => vec![index_file(cache, 2, id)?],
        RecordType::Midi => vec![index_file(cache, 3, id)?],
        RecordType::Map => {
            let e = region_entry(cache, id)?;
            vec![
                index_file(cache, 4, e.tile_file.into())?,
                index_file(cache, 4, e.location_file.into())?,
            ]
        }
        _ => {
            let (n, names) = archive_entries(cache, t, id)?.expect("archive-backed type");
            let a = cache.archive(n)?;
            names
                .iter()
                .map(|name| {
                    let e =
                        a.entry(&archive::EntryName::parse(name))
                            .ok_or(Error::MissingEntry {
                                archive: n,
                                entry: archive::EntryName::parse(name),
                            })?;
                    Ok(entry_file(n, &a, e))
                })
                .collect::<Result<_, Error>>()?
        }
    };
    Ok(Meta {
        record_type: t,
        id,
        files,
        summary: summarize(&record),
    })
}

/// Whether `err` means the record simply is not there (an absent file).
pub fn is_absent(err: &Error) -> bool {
    matches!(
        err,
        Error::Cache(CacheError::Absent { .. }) | Error::NoRecord { .. }
    )
}
