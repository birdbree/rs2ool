//! The Actions menu: export, dump, convert and bake.
//!
//! Each action goes through the same library call the command line makes
//! ([`crate::cache::dump::run`], `rs2convert::run`, `contentpack::loose`),
//! so what it writes is byte-identical to the CLI's. Actions run as
//! [jobs](crate::explorer::jobs), and the output folder is remembered in
//! `settings.json` in your platform's config directory.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::cache::{DumpArgs, IdArgs, OutArgs, SourceArgs};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use bevy_egui::egui;
use rs2cache::Records;
use rs2cache::types::record::RecordType;
use rs2convert::run;

use crate::explorer::jobs::{Done, Jobs};
use crate::explorer::selection::{Sel, Selected};
use crate::explorer::sources::{Kind, Source, Sources};

pub use crate::settings::{Settings, settings_path};

/// A conversion, as an `rs2ool pack` subcommand.
///
/// The explorer always converts with animations and without parent
/// overrides. An NPC is converted with its inferred sequences, the ones
/// its playback bar offers, so they play on both sides of compare mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvertTarget {
    /// `materials`: the palette and default material.
    Materials,
    /// `textures`: the atlas and a material per texture.
    Textures,
    /// One model, or every model.
    Models(Option<u32>),
    /// `npc --id N --with-anims --infer`, plus `--describe` with the
    /// `caption` feature.
    Npc(u32),
    /// `player-kit --with-anims`, plus `--describe` with the `caption`
    /// feature.
    PlayerKit,
    /// `sprites`: every sprite archive, and the fonts.
    Sprites,
    /// `floors`.
    Floors,
    /// One object, or every object.
    Objects(Option<u32>),
    /// One item, or every item.
    Items(Option<u32>),
    /// One spot animation, or every one.
    Spotanims(Option<u32>),
}

impl ConvertTarget {
    /// The conversion that covers the selected record, if there is one.
    /// Some cover more than the record: a texture converts all textures,
    /// a sprite or font all sprites, an identity-kit part the whole kit.
    pub fn for_selection(sel: &Sel) -> Option<Self> {
        Some(match sel {
            Sel::Record(RecordType::Model, id) => Self::Models(Some(*id)),
            Sel::Record(RecordType::Npc, id) => Self::Npc(*id),
            Sel::Record(RecordType::Loc, id) => Self::Objects(Some(*id)),
            Sel::Record(RecordType::Obj, id) => Self::Items(Some(*id)),
            Sel::Record(RecordType::SpotAnim, id) => Self::Spotanims(Some(*id)),
            Sel::Record(RecordType::Texture, _) => Self::Textures,
            Sel::Record(RecordType::Sprite | RecordType::Font, _) => Self::Sprites,
            Sel::Record(RecordType::Flo, _) => Self::Floors,
            Sel::Record(RecordType::Idk, _) | Sel::PlayerKit => Self::PlayerKit,
            _ => return None,
        })
    }

    /// The conversion of every record of a type, where one exists (models,
    /// locations, items and spot animations).
    pub fn whole_type(t: RecordType) -> Option<Self> {
        match t {
            RecordType::Model => Some(Self::Models(None)),
            RecordType::Loc => Some(Self::Objects(None)),
            RecordType::Obj => Some(Self::Items(None)),
            RecordType::SpotAnim => Some(Self::Spotanims(None)),
            _ => None,
        }
    }

    /// The menu text after "Convert ".
    pub fn label(&self) -> String {
        match self {
            Self::Materials => "materials (palette)".into(),
            Self::Textures => "textures".into(),
            Self::Models(Some(id)) => format!("model {id}"),
            Self::Models(None) => "every model".into(),
            Self::Npc(id) => format!("npc {id} with animations"),
            Self::PlayerKit => "player kit with animations".into(),
            Self::Sprites => "sprites".into(),
            Self::Floors => "floors".into(),
            Self::Objects(Some(id)) => format!("object {id}"),
            Self::Objects(None) => "every object".into(),
            Self::Items(Some(id)) => format!("item {id}"),
            Self::Items(None) => "every item".into(),
            Self::Spotanims(Some(id)) => format!("spot animation {id}"),
            Self::Spotanims(None) => "every spot animation".into(),
        }
    }
}

/// Writes a `w × h` RGBA buffer to `path` as a PNG, making parent
/// directories as needed. The same writer `rs2ool pack` hands to
/// `run::write`.
pub fn png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    image::RgbaImage::from_raw(w, h, rgba.to_vec())
        .ok_or("image size")?
        .save(path)
        .map_err(|e| e.to_string())
}

/// `outcome` with its clips' captions when `target` is an NPC or the
/// player kit, as `--describe` writes them (with the `caption` feature).
fn described(
    outcome: run::Outcome,
    records: &Records,
    target: &ConvertTarget,
) -> anyhow::Result<run::Outcome> {
    #[cfg(feature = "caption")]
    {
        let npc = match target {
            ConvertTarget::Npc(id) => Some(Some(*id)),
            ConvertTarget::PlayerKit => Some(None),
            _ => None,
        };
        if let Some(npc) = npc {
            let mut outcome = outcome;
            let store = crate::caption::describe::store_for(records.cache())?;
            crate::caption::describe::describe(&mut outcome, &store, npc);
            return Ok(outcome);
        }
    }
    let _ = (records, target);
    Ok(outcome)
}

/// Runs a conversion into `dir` exactly as `rs2ool pack` would: the same
/// source cache CRC, the same `rs2convert::run` call and the same writer.
/// Each written pack is offered as a source.
pub fn convert(records: &Records, target: &ConvertTarget, dir: &Path) -> anyhow::Result<Done> {
    let crc = rs2convert::source_cache_crc(records.cache())?;
    let outcome = match target {
        ConvertTarget::Materials => run::materials(records, crc),
        ConvertTarget::Textures => run::textures(records, crc),
        ConvertTarget::Models(id) => run::models(records, crc, *id),
        ConvertTarget::Npc(id) => {
            let extra = rs2convert::infer::npc_extras(records, *id)?;
            run::npc(records, crc, *id, true, &extra, None)
        }
        ConvertTarget::PlayerKit => run::player_kit(records, crc, true, None),
        ConvertTarget::Sprites => {
            run::sprite_archives(None).and_then(|a| run::sprites(records, crc, &a))
        }
        ConvertTarget::Floors => run::floors(records, crc),
        ConvertTarget::Objects(id) => run::objects(records, crc, *id),
        ConvertTarget::Items(id) => run::items(records, crc, *id),
        ConvertTarget::Spotanims(id) => run::spotanims(records, crc, *id),
    }?;
    let outcome = described(outcome, records, target)?;
    let written = run::write(&outcome, dir, png)?;
    let mut message = written
        .iter()
        .map(|(kind, n, _)| format!("{kind}: {n} records"))
        .collect::<Vec<_>>()
        .join(", ");
    if outcome.flagged() {
        message.push_str(" (some clips exceed the validation threshold)");
    }
    // The models pack first: for a creature it's the one to open, since it
    // finds its rigs and anims beside it.
    let mut opens: Vec<_> = written.into_iter().map(|(k, _, p)| (k, p)).collect();
    opens.sort_by_key(|(k, _)| *k != contentpack::Kind::Models);
    Ok(Done {
        message,
        opens: opens.into_iter().map(|(_, p)| (Kind::Pack, p)).collect(),
    })
}

/// Exports records as `rs2ool cache dump` would, minus the progress bars;
/// the output folder is offered as a dump-directory source.
pub fn dump(records: &Records, mut args: DumpArgs) -> anyhow::Result<Done> {
    args.out.quiet = true;
    let outcome = crate::cache::dump::run(records, &args)?;
    let message = match outcome {
        crate::cache::Outcome::Ok => "written".to_owned(),
        crate::cache::Outcome::Partial => {
            "written; some records failed (see manifest.json)".to_owned()
        }
    };
    Ok(Done {
        message,
        opens: vec![(Kind::DumpDir, args.out.dir.clone())],
    })
}

/// The arguments for `rs2ool cache dump --type T --meta` with `--id` (for
/// `Some`) or `--all`.
pub fn decoded(t: RecordType, id: Option<u32>, dir: &Path) -> DumpArgs {
    DumpArgs {
        source: SourceArgs {
            kind: Some(t),
            ..Default::default()
        },
        ids: IdArgs {
            id,
            all: id.is_none(),
            ..Default::default()
        },
        decode: true,
        out: OutArgs {
            dir: dir.to_path_buf(),
            meta: true,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// The arguments for `rs2ool cache dump --index N --id ID --meta`, which
/// writes the inflated file, for record types that are one index file
/// each (model, anim, midi). `None` for the rest.
pub fn file_bytes(t: RecordType, id: u32, dir: &Path) -> Option<DumpArgs> {
    let index = match t {
        RecordType::Model => 1,
        RecordType::Anim => 2,
        RecordType::Midi => 3,
        _ => return None,
    };
    Some(DumpArgs {
        source: SourceArgs {
            index: Some(index),
            ..Default::default()
        },
        ids: IdArgs {
            id: Some(id),
            ..Default::default()
        },
        out: OutArgs {
            dir: dir.to_path_buf(),
            meta: true,
            ..Default::default()
        },
        ..Default::default()
    })
}

/// Bakes a loose record directory into a pack in `out`, as
/// `rs2ool pack build` does.
pub fn bake(dir: &Path, out: &Path) -> anyhow::Result<Done> {
    let path = contentpack::loose::bake(dir, out)?;
    Ok(Done {
        message: format!("baked {}", path.display()),
        opens: vec![(Kind::Pack, path)],
    })
}

/// What a folder dialog's answer is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pick {
    /// The output folder.
    OutDir,
    /// A loose directory to bake.
    Loose,
}

/// A job an action runs: given the cache and the output folder.
type ActionFn = Box<dyn FnOnce(Arc<Records<'static>>, PathBuf) -> anyhow::Result<Done> + Send>;

/// The Actions menu's state: any folder dialog in flight.
#[derive(Resource, Default)]
pub struct Actions {
    picking: Option<(Pick, Task<Option<PathBuf>>)>,
}

impl Actions {
    /// Opens a folder dialog off the UI thread. A second one replaces the
    /// first.
    fn pick(&mut self, what: Pick) {
        let task = AsyncComputeTaskPool::get().spawn(async move {
            rfd::AsyncFileDialog::new()
                .pick_folder()
                .await
                .map(|h| h.path().to_path_buf())
        });
        self.picking = Some((what, task));
    }
}

/// Acts on a folder dialog's answer: stores and saves a new output folder,
/// or starts baking the chosen loose directory.
fn poll_pick(mut actions: ResMut<Actions>, mut settings: ResMut<Settings>, mut jobs: ResMut<Jobs>) {
    // Peek without flagging `Actions` as changed every frame.
    let Some((what, task)) = &mut actions.bypass_change_detection().picking else {
        return;
    };
    let Some(answer) = block_on(poll_once(task)) else {
        return;
    };
    let what = *what;
    actions.picking = None;
    // `None`: the dialog was cancelled.
    let Some(path) = answer else { return };
    if what == Pick::Loose {
        if let Some(out) = settings.out_dir.clone() {
            jobs.spawn_job(format!("bake {}", path.display()), 0, move |_| {
                bake(&path, &out)
            });
        }
    } else {
        settings.out_dir = Some(path);
        if let Some(p) = settings_path() {
            // Not being able to remember it isn't worth bothering you about.
            let _ = settings.save(&p);
        }
    }
}

/// Draws the Actions menu and starts the job for whichever entry is
/// clicked.
///
/// Every action needs an output folder. All but baking also need the
/// selection to be in a cache, and the per-record ones need a cache
/// record selected.
pub fn actions_menu(
    ui: &mut egui::Ui,
    actions: &mut Actions,
    settings: &Settings,
    selected: &Selected,
    sources: &Sources,
    jobs: &mut Jobs,
) {
    ui.menu_button("Actions", |ui| {
        ui.horizontal(|ui| {
            ui.label("Output folder:");
            let shown = settings
                .out_dir
                .as_ref()
                .map_or("none".into(), |p| p.display().to_string());
            ui.add(egui::Label::new(shown).truncate());
        });
        if ui.button("Choose output folder…").clicked() {
            actions.pick(Pick::OutDir);
        }
        ui.separator();
        let out = settings.out_dir.clone();
        // The selected source's records, if it's a cache.
        let records: Option<Arc<Records<'static>>> = selected
            .source
            .and_then(|s| sources.list.get(s))
            .and_then(|s| match s {
                Source::Cache { records, .. } => Some(records.clone()),
                _ => None,
            });
        let ready = out.is_some() && records.is_some();
        let sel = selected.sel.clone();
        let record = match &sel {
            Some(Sel::Record(t, id)) => Some((*t, *id)),
            _ => None,
        };
        // One menu entry: a button that runs `f` as a job when clicked,
        // greyed out unless `enabled` and there's a cache and an output.
        let mut run = |ui: &mut egui::Ui, enabled: bool, label: String, f: ActionFn| {
            if ui
                .add_enabled(enabled && ready, egui::Button::new(&label))
                .clicked()
                && let (Some(r), Some(o)) = (records.clone(), out.clone())
            {
                jobs.spawn_job(label, 0, move |_| f(r, o));
                ui.close();
            }
        };
        // Placeholder values for the labels when nothing's selected; those
        // entries are disabled anyway.
        let (t, id) = record.unwrap_or((RecordType::Model, 0));
        run(
            ui,
            record.is_some(),
            format!("Export {t} {id} (decoded)"),
            Box::new(move |r, o| dump(&r, decoded(t, Some(id), &o))),
        );
        run(
            ui,
            // Only types that are one index file each.
            record.is_some_and(|(t, _)| file_bytes(t, 0, Path::new("")).is_some()),
            format!("Export {t} {id} (file bytes)"),
            Box::new(move |r, o| {
                let args = file_bytes(t, id, &o).ok_or_else(|| anyhow::anyhow!("not one file"))?;
                dump(&r, args)
            }),
        );
        run(
            ui,
            record.is_some(),
            format!("Dump every {t}"),
            Box::new(move |r, o| dump(&r, decoded(t, None, &o))),
        );
        ui.separator();
        let target = sel.as_ref().and_then(ConvertTarget::for_selection);
        let label = target.as_ref().map_or("Convert selection".into(), |t| {
            format!("Convert {}", t.label())
        });
        let tg = target.clone();
        run(
            ui,
            target.is_some(),
            label,
            Box::new(move |r, o| convert(&r, tg.as_ref().expect("enabled"), &o)),
        );
        let whole = record.and_then(|(t, _)| ConvertTarget::whole_type(t));
        let label = whole
            .as_ref()
            .map_or("Convert every record of the type".into(), |t| {
                format!("Convert {}", t.label())
            });
        let wt = whole.clone();
        run(
            ui,
            whole.is_some(),
            label,
            Box::new(move |r, o| convert(&r, wt.as_ref().expect("enabled"), &o)),
        );
        run(
            ui,
            true,
            "Convert materials (palette)".into(),
            Box::new(|r, o| convert(&r, &ConvertTarget::Materials, &o)),
        );
        ui.separator();
        if ui
            .add_enabled(out.is_some(), egui::Button::new("Bake a loose directory…"))
            .on_disabled_hover_text("choose an output folder first")
            .clicked()
        {
            actions.pick(Pick::Loose);
            ui.close();
        }
        if actions.picking.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label("waiting for the folder dialog…");
            });
        }
        if !ready {
            ui.weak("Select a cache record and choose an output folder.");
        }
    });
}

/// Loads the saved settings and adds the Actions resources.
pub fn plugin(app: &mut App) {
    let settings = settings_path()
        .map(|p| Settings::load(&p))
        .unwrap_or_default();
    app.insert_resource(settings)
        .init_resource::<Actions>()
        .add_systems(Update, poll_pick);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selections_map_to_pack_commands() {
        assert_eq!(
            ConvertTarget::for_selection(&Sel::Record(RecordType::Npc, 1)),
            Some(ConvertTarget::Npc(1))
        );
        assert_eq!(
            ConvertTarget::for_selection(&Sel::PlayerKit),
            Some(ConvertTarget::PlayerKit)
        );
        assert_eq!(
            ConvertTarget::for_selection(&Sel::Record(RecordType::Seq, 1)),
            None
        );
        assert!(file_bytes(RecordType::Model, 5, Path::new("o")).is_some());
        assert!(file_bytes(RecordType::Npc, 5, Path::new("o")).is_none());
    }
}
