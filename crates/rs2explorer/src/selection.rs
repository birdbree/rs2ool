//! What is selected, and its decoded form. Selecting spawns a decode on the
//! compute pool; the result lands in [`Loaded`].

use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use rs2intermediate::record::{Meta, Record, RecordType};

use crate::renderable::{Renderable, States, load_renderable, load_states};
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

/// The right-hand pane's selection in compare mode.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct CompareSelected(pub Selected);

/// A selection resource that a [`Loaded`] follows.
pub trait Slot: Resource {
    fn selection(&self) -> &Selected;
}

impl Slot for Selected {
    fn selection(&self) -> &Selected {
        self
    }
}

impl Slot for CompareSelected {
    fn selection(&self) -> &Selected {
        &self.0
    }
}

/// A decoded selection.
pub enum LoadedData {
    Record {
        record: Box<Record>,
        meta: Box<Meta>,
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

type LoadTask = Task<Result<(LoadedData, Option<Renderable>, Option<States>), String>>;

/// The decoded form of the selection in slot `S` (the main selection by
/// default; [`CompareSelected`] for compare mode's right pane).
#[derive(Resource)]
pub struct Loaded<S = Selected> {
    /// The selection the current data belongs to.
    pub for_sel: Option<(usize, Sel)>,
    pub data: Option<Arc<LoadedData>>,
    /// What the viewports draw for it, if anything.
    pub renderable: Option<Arc<Renderable>>,
    /// An object's states, when it has more than its default; `state` is
    /// the one shown (its renderable is `renderable`).
    pub states: Option<Arc<States>>,
    pub state: usize,
    pub error: Option<String>,
    pending: Option<((usize, Sel), LoadTask)>,
    slot: PhantomData<fn() -> S>,
}

impl<S> Default for Loaded<S> {
    fn default() -> Self {
        Self {
            for_sel: None,
            data: None,
            renderable: None,
            states: None,
            state: 0,
            error: None,
            pending: None,
            slot: PhantomData,
        }
    }
}

impl<S> Loaded<S> {
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

/// Decode a selection for the inspector and resolve what it draws.
pub fn load(source: &Source, sel: &Sel) -> Result<(LoadedData, Option<Renderable>), String> {
    let data = decode(source, sel)?;
    let record = match &data {
        LoadedData::Record { record, .. } => Some(&**record),
        _ => None,
    };
    let renderable = load_renderable(source, sel, record)?;
    Ok((data, renderable))
}

fn decode(source: &Source, sel: &Sel) -> Result<LoadedData, String> {
    match (source, sel) {
        (Source::Cache { records, .. }, Sel::Record(t, id)) => {
            let record = records.decode(*t, *id).map_err(|e| e.to_string())?;
            let meta = records.meta(*t, *id).map_err(|e| e.to_string())?;
            Ok(LoadedData::Record {
                record: Box::new(record),
                meta: Box::new(meta),
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

fn start<S: Slot>(selected: Res<S>, sources: Res<Sources>, mut loaded: ResMut<Loaded<S>>) {
    if !selected.is_changed() {
        return;
    }
    let selected = selected.selection();
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
    let task = AsyncComputeTaskPool::get().spawn(async move {
        let (data, r) = load(&source, &sel)?;
        let record = match &data {
            LoadedData::Record { record, .. } => Some(&**record),
            _ => None,
        };
        let states = load_states(&source, &sel, record);
        Ok((data, r, states))
    });
    loaded.pending = Some((key, task));
}

fn poll<S: Slot>(mut loaded: ResMut<Loaded<S>>) {
    // Poll without marking `Loaded` changed: systems that react to a new
    // selection must only see a change when the result lands.
    let Some((_, task)) = &mut loaded.bypass_change_detection().pending else {
        return;
    };
    let Some(result) = block_on(poll_once(task)) else {
        return;
    };
    let (key, _) = loaded.pending.take().expect("pending");
    loaded.for_sel = Some(key);
    match result {
        Ok((d, r, states)) => {
            loaded.data = Some(Arc::new(d));
            // An object with states shows its own model, or when it has none
            // (a morph parent is often invisible) its first state that draws.
            let first = states
                .as_ref()
                .and_then(|s| s.renderables.iter().position(Option::is_some));
            loaded.state = if r.is_some() { 0 } else { first.unwrap_or(0) };
            loaded.renderable = r.map(Arc::new).or_else(|| {
                states
                    .as_ref()
                    .and_then(|s| s.renderables.get(loaded.state).cloned().flatten())
            });
            loaded.states = states.map(Arc::new);
            loaded.error = None;
        }
        Err(e) => {
            loaded.data = None;
            loaded.renderable = None;
            loaded.states = None;
            loaded.error = Some(e);
        }
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Selected>()
        .init_resource::<Loaded>()
        .init_resource::<CompareSelected>()
        .init_resource::<Loaded<CompareSelected>>()
        .add_systems(
            Update,
            (
                (start::<Selected>, poll::<Selected>).chain(),
                (start::<CompareSelected>, poll::<CompareSelected>).chain(),
            ),
        );
}
