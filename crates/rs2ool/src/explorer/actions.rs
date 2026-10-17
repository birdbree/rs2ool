//! Actions: export, dump, convert and bake, through the same library calls
//! the CLIs make (`crate::cache::dump::run`, `rs2convert::run`,
//! `contentpack::loose`), so every output is byte-identical to the CLI's. They
//! run as jobs; the output folder is remembered in `settings.json` in the
//! platform config directory.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::cache::{DumpArgs, IdArgs, OutArgs, SourceArgs};
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use bevy_egui::egui;
use rs2cache::Records;
use rs2cache::types::record::RecordType;
use rs2convert::run;
use serde::{Deserialize, Serialize};

use crate::explorer::jobs::{Done, Jobs};
use crate::explorer::selection::{Sel, Selected};
use crate::explorer::sources::{Kind, Source, Sources};

/// What the explorer remembers between runs.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub out_dir: Option<PathBuf>,
}

/// `settings.json` in the platform config directory.
pub fn settings_path() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "rs2ool").map(|d| d.config_dir().join("settings.json"))
}

impl Settings {
    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(d) = path.parent() {
            std::fs::create_dir_all(d)?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, json)
    }
}

/// A conversion, as an `rs2conv` subcommand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConvertTarget {
    Materials,
    Textures,
    /// One model, or every model.
    Models(Option<u32>),
    /// `npc --id N --with-anims`.
    Npc(u32),
    /// `player-kit --with-anims`.
    PlayerKit,
    /// Every sprite archive.
    Sprites,
    Floors,
    /// One object, or every object.
    Objects(Option<u32>),
}

impl ConvertTarget {
    /// The conversion of the selected record.
    pub fn for_selection(sel: &Sel) -> Option<Self> {
        Some(match sel {
            Sel::Record(RecordType::Model, id) => Self::Models(Some(*id)),
            Sel::Record(RecordType::Npc, id) => Self::Npc(*id),
            Sel::Record(RecordType::Loc, id) => Self::Objects(Some(*id)),
            Sel::Record(RecordType::Texture, _) => Self::Textures,
            Sel::Record(RecordType::Sprite | RecordType::Font, _) => Self::Sprites,
            Sel::Record(RecordType::Flo, _) => Self::Floors,
            Sel::Record(RecordType::Idk, _) | Sel::PlayerKit => Self::PlayerKit,
            _ => return None,
        })
    }

    /// The conversion of every record of a type, where one exists.
    pub fn whole_type(t: RecordType) -> Option<Self> {
        match t {
            RecordType::Model => Some(Self::Models(None)),
            RecordType::Loc => Some(Self::Objects(None)),
            _ => None,
        }
    }

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
        }
    }
}

/// Encode RGBA as a PNG file, as `rs2conv` does.
pub fn png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    image::RgbaImage::from_raw(w, h, rgba.to_vec())
        .ok_or("image size")?
        .save(path)
        .map_err(|e| e.to_string())
}

/// Run a conversion into `dir` exactly as `rs2conv` would: the same source
/// cache CRC, the same `rs2convert::run` call and the same writer. Each
/// written pack is offered as a source.
pub fn convert(records: &Records, target: &ConvertTarget, dir: &Path) -> anyhow::Result<Done> {
    let crc = rs2convert::source_cache_crc(records.cache())?;
    let outcome = match target {
        ConvertTarget::Materials => run::materials(records, crc),
        ConvertTarget::Textures => run::textures(records, crc),
        ConvertTarget::Models(id) => run::models(records, crc, *id),
        ConvertTarget::Npc(id) => run::npc(records, crc, *id, true, &[], None),
        ConvertTarget::PlayerKit => run::player_kit(records, crc, true, None),
        ConvertTarget::Sprites => {
            run::sprite_archives(None).and_then(|a| run::sprites(records, crc, &a))
        }
        ConvertTarget::Floors => run::floors(records, crc),
        ConvertTarget::Objects(id) => run::objects(records, crc, *id),
    }?;
    let written = run::write(&outcome, dir, png)?;
    let mut message = written
        .iter()
        .map(|(kind, n, _)| format!("{kind}: {n} records"))
        .collect::<Vec<_>>()
        .join(", ");
    if outcome.flagged() {
        message.push_str(" (some clips exceed the validation threshold)");
    }
    Ok(Done {
        message,
        opens: written
            .into_iter()
            .map(|(_, _, p)| (Kind::Pack, p))
            .collect(),
    })
}

/// Export records as `cachedump dump` would; the output folder is offered
/// as a dump-directory source.
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

/// `cachedump dump --type T` of one id (`Some`) or of every id, decoded.
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

/// The file bytes of an index-backed record (`cachedump dump --index N
/// --id ID`, inflated), where the record is one file.
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

/// Bake a loose record directory into a pack in `out`, as `rs2conv pack`.
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
    OutDir,
    Loose,
}

/// A job an action runs: given the cache and the output folder.
type ActionFn = Box<dyn FnOnce(Arc<Records<'static>>, PathBuf) -> anyhow::Result<Done> + Send>;

/// Any folder dialog in flight.
#[derive(Resource, Default)]
pub struct Actions {
    picking: Option<(Pick, Task<Option<PathBuf>>)>,
}

impl Actions {
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

fn poll_pick(mut actions: ResMut<Actions>, mut settings: ResMut<Settings>, mut jobs: ResMut<Jobs>) {
    let Some((what, task)) = &mut actions.bypass_change_detection().picking else {
        return;
    };
    let Some(answer) = block_on(poll_once(task)) else {
        return;
    };
    let what = *what;
    actions.picking = None;
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
            let _ = settings.save(&p);
        }
    }
}

/// The Actions menu. Every action needs an output folder; cache actions
/// need a cache selection.
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
        let (t, id) = record.unwrap_or((RecordType::Model, 0));
        run(
            ui,
            record.is_some(),
            format!("Export {t} {id} (decoded)"),
            Box::new(move |r, o| dump(&r, decoded(t, Some(id), &o))),
        );
        run(
            ui,
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
    fn settings_round_trip() {
        let dir = std::env::temp_dir().join(format!("rs2ool-settings-{}", std::process::id()));
        let p = dir.join("settings.json");
        let s = Settings {
            out_dir: Some(PathBuf::from("/tmp/out")),
        };
        s.save(&p).unwrap();
        assert_eq!(Settings::load(&p), s);
        assert_eq!(
            Settings::load(&dir.join("missing.json")),
            Settings::default()
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn selections_map_to_rs2conv_commands() {
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
