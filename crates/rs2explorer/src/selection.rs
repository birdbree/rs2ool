//! What is selected, and its decoded form. Selecting spawns a decode on the
//! compute pool; the result lands in [`Loaded`].

use std::path::PathBuf;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use rs2intermediate::record::{Meta, Record, RecordType};

use crate::sources::{Source, Sources, detach};

/// One selectable thing within a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sel {
    /// A record of a cache (or a decoded record in a dump directory).
    Record(RecordType, u32),
    /// A record of a pack, by id.
    PackRecord(u32),
    /// A file in a dump directory, relative to it.
    DumpFile(PathBuf),
}

#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    pub source: Option<usize>,
    pub sel: Option<Sel>,
}

/// A decoded selection.
pub enum LoadedData {
    Record {
        record: Box<Record>,
        meta: Box<Meta>,
        /// The model the record describes, assembled as the client draws
        /// it (`rs2convert::preview`), for the viewers.
        preview: Option<Box<rs2intermediate::model::Model>>,
    },
    Pack {
        kind: rs2pack::Kind,
        id: u32,
        bytes: Vec<u8>,
        decoded: String,
    },
    Dump {
        path: PathBuf,
        json: Option<serde_json::Value>,
        bytes: Vec<u8>,
    },
}

type LoadTask = Task<Result<LoadedData, String>>;

#[derive(Resource, Default)]
pub struct Loaded {
    /// The selection the current data belongs to.
    pub for_sel: Option<(usize, Sel)>,
    pub data: Option<Arc<LoadedData>>,
    pub error: Option<String>,
    pending: Option<((usize, Sel), LoadTask)>,
}

impl Loaded {
    pub fn is_loading(&self) -> bool {
        self.pending.is_some()
    }
}

/// Decode a pack record for display, by kind.
pub fn describe_pack_record(kind: rs2pack::Kind, id: u32, bytes: &[u8]) -> String {
    use rs2pack::Kind;
    use rs2pack::records::*;
    let r = match kind {
        Kind::Models => model::PackModel::decode(bytes).map(|m| format!("{m:#?}")),
        Kind::Materials if id == material::PALETTE_ID => {
            material::decode_palette(bytes).map(|p| format!("palette, {} entries", p.len()))
        }
        Kind::Materials => material::Material::decode(bytes).map(|m| format!("{m:#?}")),
        Kind::Textures if id & texture::ATLAS_BASE != 0 => {
            texture::Atlas::decode(bytes).map(|a| format!("atlas {}x{}", a.width, a.height))
        }
        Kind::Textures => texture::TextureRect::decode(bytes).map(|t| format!("{t:#?}")),
        Kind::Rigs => rig::Rig::decode(bytes).map(|r| format!("{:#?}", r.bones)),
        Kind::Anims => anim::Clip::decode(bytes).map(|c| {
            format!(
                "rig {}, {} frames, duration {}, {} keys",
                c.rig,
                c.frame_start.len(),
                c.duration,
                c.keys.len()
            )
        }),
        Kind::Sprites => sprite::SpriteRecord::decode(bytes)
            .map(|s| {
                format!(
                    "sprite {:#010x}.{} {}x{} at {},{}",
                    s.name_hash, s.sub_index, s.width, s.height, s.x_offset, s.y_offset
                )
            })
            .or_else(|_| sprite::FontMetrics::decode(bytes).map(|_| "font metrics".to_owned())),
        Kind::Objects => object::ObjectRecord::decode(bytes).map(|o| format!("{o:#?}")),
        Kind::Kits => object::KitRecord::decode(bytes).map(|k| format!("{k:#?}")),
        Kind::Strings => strings::decode(bytes)
            .map(|s| format!("{} strings: {:?}", s.len(), &s[..s.len().min(20)])),
        _ => Ok(format!("{} bytes", bytes.len())),
    };
    r.unwrap_or_else(|e| format!("cannot decode: {e}"))
}

fn load(source: &Source, sel: &Sel) -> Result<LoadedData, String> {
    match (source, sel) {
        (Source::Cache { records, .. }, Sel::Record(t, id)) => {
            let record = records.decode(*t, *id).map_err(|e| e.to_string())?;
            let meta = records.meta(*t, *id).map_err(|e| e.to_string())?;
            let preview = rs2convert::preview::model_for(records, &record)
                .map_err(|e| e.to_string())?
                .map(Box::new);
            Ok(LoadedData::Record {
                record: Box::new(record),
                meta: Box::new(meta),
                preview,
            })
        }
        (Source::Pack { reader, .. }, Sel::PackRecord(id)) => {
            let bytes = reader
                .get(*id)
                .ok_or("no such record")?
                .map_err(|e| e.to_string())?;
            let kind = reader.manifest.kind;
            Ok(LoadedData::Pack {
                kind,
                id: *id,
                decoded: describe_pack_record(kind, *id, &bytes),
                bytes,
            })
        }
        (Source::DumpDir { path }, Sel::DumpFile(rel)) => {
            let bytes = std::fs::read(path.join(rel)).map_err(|e| e.to_string())?;
            let json = if rel.extension().is_some_and(|e| e == "json") {
                serde_json::from_slice(&bytes).ok()
            } else {
                None
            };
            Ok(LoadedData::Dump {
                path: rel.clone(),
                json,
                bytes,
            })
        }
        _ => Err("selection does not belong to this source".into()),
    }
}

fn start(selected: Res<Selected>, sources: Res<Sources>, mut loaded: ResMut<Loaded>) {
    if !selected.is_changed() {
        return;
    }
    let (Some(si), Some(sel)) = (selected.source, selected.sel.clone()) else {
        return;
    };
    let Some(source) = sources.list.get(si) else {
        return;
    };
    if loaded.for_sel.as_ref() == Some(&(si, sel.clone())) && loaded.data.is_some() {
        return;
    }
    let source = detach(source);
    let key = (si, sel.clone());
    let task = AsyncComputeTaskPool::get().spawn(async move { load(&source, &sel) });
    loaded.pending = Some((key, task));
}

fn poll(mut loaded: ResMut<Loaded>) {
    let Some((_, task)) = &mut loaded.pending else {
        return;
    };
    let Some(result) = block_on(poll_once(task)) else {
        return;
    };
    let (key, _) = loaded.pending.take().expect("pending");
    loaded.for_sel = Some(key);
    match result {
        Ok(d) => {
            loaded.data = Some(Arc::new(d));
            loaded.error = None;
        }
        Err(e) => {
            loaded.data = None;
            loaded.error = Some(e);
        }
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Selected>()
        .init_resource::<Loaded>()
        .add_systems(Update, (start, poll).chain());
}
