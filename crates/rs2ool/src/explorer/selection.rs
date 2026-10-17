//! What's selected, and its decoded form.
//!
//! There are two selection slots: [`Selected`], the main one, and
//! [`CompareSelected`], compare mode's right pane. Changing either spawns a
//! decode on the compute pool, and the result lands in the matching
//! [`Loaded`] a few frames later. Everything that draws or inspects reads
//! `Loaded`, never the source directly.

use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use rs2cache::types::record::{Meta, Record, RecordType};

use crate::explorer::renderable::{Renderable, States, load_renderable, load_states};
use crate::explorer::sources::{Source, Sources, detach};

/// One selectable thing within a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sel {
    /// A record of a cache, by type and id.
    Record(RecordType, u32),
    /// A record of a pack, by id.
    PackRecord(u32),
    /// A file in a dump directory, relative to it.
    DumpFile(PathBuf),
    /// A cache's player kit (see [`crate::explorer::kit`]).
    PlayerKit,
}

/// The systems that start and finish selection decodes. The kit's
/// systems run after them.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct LoadSet;

/// The main selection: a source and something in it.
///
/// Set both fields together; a load starts only once both are `Some`.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct Selected {
    /// Index into [`Sources::list`].
    pub source: Option<usize>,
    /// What's selected in that source.
    pub sel: Option<Sel>,
}

/// The right-hand pane's selection in compare mode.
#[derive(Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct CompareSelected(pub Selected);

/// A selection resource that a [`Loaded`] follows.
pub trait Slot: Resource {
    /// The selection this slot holds.
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

/// A decoded selection, what the inspector shows.
pub enum LoadedData {
    /// A cache record.
    Record {
        /// The decoded record.
        record: Box<Record>,
        /// Its metadata sidecar, as `rs2ool cache info` prints it.
        meta: Box<Meta>,
    },
    /// A pack record.
    Pack {
        /// The pack's kind.
        kind: contentpack::Kind,
        /// The record id.
        id: u32,
        /// The record's bytes as stored.
        bytes: Vec<u8>,
        /// A readable summary, from [`describe_pack_record`].
        decoded: String,
    },
    /// A file in a dump directory.
    Dump {
        /// Relative to the dump directory.
        path: PathBuf,
        /// The parsed contents, for a `.json` file that parses.
        json: Option<serde_json::Value>,
        /// The file's bytes.
        bytes: Vec<u8>,
    },
    /// Something with no record of its own, described by rows.
    Note {
        /// `(key, value)` rows for the inspector.
        rows: Vec<(String, String)>,
    },
}

/// A decode in flight: what the inspector shows, what the viewport draws,
/// and the object states, or the error text.
type LoadTask = Task<Result<(LoadedData, Option<Renderable>, Option<States>), String>>;

/// The decoded form of the selection in slot `S` (the main selection by
/// default; [`CompareSelected`] for compare mode's right pane).
#[derive(Resource)]
pub struct Loaded<S = Selected> {
    /// The selection the current data belongs to, as `(source, sel)`.
    pub for_sel: Option<(usize, Sel)>,
    /// What the inspector shows; `None` before the first load or after an
    /// error.
    pub data: Option<Arc<LoadedData>>,
    /// What the viewports draw for it, if anything.
    pub renderable: Option<Arc<Renderable>>,
    /// An object's states, when it has more than its default; `state` is
    /// the one shown (its renderable is `renderable`).
    pub states: Option<Arc<States>>,
    /// Index of the state shown, into `states`; 0 when there are none.
    pub state: usize,
    /// Why the last load failed, if it did.
    pub error: Option<String>,
    // The decode in flight and the selection it's for.
    pending: Option<((usize, Sel), LoadTask)>,
    // Ties the resource to its slot type without storing one.
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
    /// Whether a decode is still running. The old data stays put until it
    /// finishes.
    pub fn is_loading(&self) -> bool {
        self.pending.is_some()
    }
}

/// Decodes a pack record into a readable summary, by kind.
///
/// Small records come out as `{:#?}`; big ones (palettes, atlases, clips)
/// as a one-line summary. A record that won't decode says so instead of
/// failing.
pub fn describe_pack_record(kind: contentpack::Kind, id: u32, bytes: &[u8]) -> String {
    use contentpack::Kind;
    use contentpack::records::*;
    let r = match kind {
        Kind::Models => model::PackModel::decode(bytes).map(|m| format!("{m:#?}")),
        Kind::Materials if id == material::PALETTE_ID => {
            material::decode_palette(bytes).map(|p| format!("palette, {} entries", p.len()))
        }
        Kind::Materials => material::Material::decode(bytes).map(|m| format!("{m:#?}")),
        // Atlas ids have the ATLAS_BASE bit set; the rest are texture rects.
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
        // A sprites pack also holds font metrics; try a sprite first.
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

/// Decodes a selection for the inspector and works out what it draws.
///
/// Blocking; the explorer calls it from a task.
pub fn load(source: &Source, sel: &Sel) -> Result<(LoadedData, Option<Renderable>), String> {
    let data = decode(source, sel)?;
    let record = match &data {
        LoadedData::Record { record, .. } => Some(&**record),
        _ => None,
    };
    let renderable = load_renderable(source, sel, record)?;
    Ok((data, renderable))
}

/// The inspector half of [`load`].
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
        (Source::Cache { .. }, Sel::PlayerKit) => Ok(LoadedData::Note {
            rows: vec![
                ("kind".into(), "player kit".into()),
                (
                    "built from".into(),
                    "identity-kit parts and item equipment models".into(),
                ),
                (
                    "sequences".into(),
                    format!("{:?}", rs2convert::pipeline::PLAYER_SEQUENCES),
                ),
            ],
        }),
        _ => Err("selection does not belong to this source".into()),
    }
}

/// Starts a decode when slot `S` changes, unless that selection is already
/// loaded. A newer selection replaces a pending one, whose result is
/// dropped.
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

/// Moves a finished decode into `Loaded<S>`.
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

/// Adds both selection slots and their loaders.
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
            )
                .in_set(LoadSet),
        );
}
