//! Captions in the explorer: the Captions window with the caption settings, a job that captions the
//! selected NPC's candidate sequences, and each caption beside its
//! sequence in the picker, with a way to check it by eye.
//!
//! It runs [`crate::caption::run`], the code behind `rs2ool cache caption`,
//! on the compute pool. The job and the window share one caption store,
//! locked only briefly by either, so reviewing while a job runs is safe.
//! Nothing is contacted until captioning is enabled, and the caption
//! action stays greyed out until a connection test has passed.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use bevy_egui::egui;
use rs2cache::Records;
use rs2cache::types::record::RecordType;

use crate::caption::client::{Client, is_local};
use crate::caption::store::{Entry, Review, Store, lock};
use crate::caption::{self, CLASSES, DEFAULT_PROMPT, Event, Options, Target};
use crate::explorer::jobs::{Done, Jobs, ProgressHandle};
use crate::explorer::selection::{Sel, Selected};
use crate::explorer::sources::{Source, Sources};
use crate::settings::{self, Backend, Caption, Settings, settings_path};

/// A running caption job's switches.
struct Run {
    /// Set to stop after the image in hand.
    stop: Arc<AtomicBool>,
    /// Set by the job when it ends.
    finished: Arc<AtomicBool>,
}

/// The explorer's caption state.
#[derive(Resource, Default)]
pub struct Captions {
    /// The first open cache's captions, once its CRC is read.
    pub store: Option<Arc<Mutex<Store>>>,
    loading: Option<Task<Option<Store>>>,
    /// The last connection test: the server's models, or why it failed.
    pub test: Option<Result<Vec<String>, String>>,
    testing: Option<Task<Result<Vec<String>, String>>>,
    run: Option<Run>,
    /// Whether the Captions window is open.
    pub open: bool,
    /// At most this many images per run; 0 for every candidate.
    pub limit: usize,
}

impl Captions {
    /// Whether a caption job is running.
    pub fn running(&self) -> bool {
        self.run
            .as_ref()
            .is_some_and(|r| !r.finished.load(Ordering::Relaxed))
    }

    /// Whether the last connection test passed.
    pub fn connected(&self) -> bool {
        matches!(self.test, Some(Ok(_)))
    }
}

/// Captions NPC `npc`'s candidates with the server `c` names, into
/// `store`: the caption job. At most `limit` images (0 for all); `stop`
/// stops it after the image in hand.
pub fn caption_job(
    records: &Records,
    c: &Caption,
    store: &Mutex<Store>,
    npc: u32,
    limit: usize,
    stop: Arc<AtomicBool>,
    progress: &ProgressHandle,
) -> anyhow::Result<Done> {
    let opts = Options {
        limit: (limit > 0).then_some(limit),
        cancel: Some(stop.clone()),
        ..Options::default()
    };
    let (mut planned, mut captioned, mut failed) = (0, 0, Vec::new());
    caption::run(records, c, store, Target::Npc(npc), &opts, |e| match e {
        Event::Planned(n) => {
            planned = n;
            progress.set_total(n as u32);
        }
        Event::Captioned(_) => {
            captioned += 1;
            progress.inc();
        }
        Event::Failed { seq, error, .. } => {
            failed.push(format!("seq {seq}: {error:#}"));
            progress.inc();
        }
        Event::Drawn { .. } => {}
    })?;
    let mut message = if planned == 0 {
        format!("npc {npc}: nothing left to caption")
    } else if stop.load(Ordering::Relaxed) {
        format!("npc {npc}: stopped after {captioned} of {planned} captions")
    } else {
        format!("npc {npc}: {captioned} of {planned} captioned")
    };
    if let Some(first) = failed.first() {
        message.push_str(&format!("; {} failed, first {first}", failed.len()));
    }
    Ok(message.into())
}

/// A caption's tag in the picker: its class, marked ✔ once reviewed right,
/// or ✎ with the corrected class.
pub fn tag(e: &Entry) -> String {
    let class = e.class.as_deref().unwrap_or("?");
    match &e.review {
        Some(Review { right: true, .. }) => format!("✔ {class}"),
        Some(Review {
            right: false,
            class: Some(c),
        }) => format!("✎ {c}"),
        Some(Review {
            right: false,
            class: None,
        }) => format!("✘ {class}"),
        None => format!("💬 {class}"),
    }
}

/// Opens the first cache's caption store off the UI thread, once.
fn load_store(sources: Res<Sources>, mut captions: ResMut<Captions>) {
    if captions.store.is_some() {
        return;
    }
    if let Some(task) = &mut captions.loading {
        if let Some(store) = block_on(poll_once(task)) {
            captions.loading = None;
            captions.store = store.map(|s| Arc::new(Mutex::new(s)));
        }
        return;
    }
    let Some(Source::Cache { records, .. }) = sources
        .list
        .iter()
        .find(|s| matches!(s, Source::Cache { .. }))
    else {
        return;
    };
    let records = records.clone();
    captions.loading = Some(AsyncComputeTaskPool::get().spawn(async move {
        let crc = rs2convert::source_cache_crc(records.cache()).ok()?;
        match Store::open(&settings::data_dir()?, crc) {
            Ok(s) => Some(s),
            Err(e) => {
                tracing::warn!("captions: {e:#}");
                None
            }
        }
    }));
}

/// Picks up a finished connection test.
fn poll_test(mut captions: ResMut<Captions>) {
    let Some(task) = &mut captions.bypass_change_detection().testing else {
        return;
    };
    if let Some(r) = block_on(poll_once(task)) {
        captions.testing = None;
        captions.test = Some(r);
    }
}

/// The selected NPC and its cache's records, if a cache NPC is selected.
fn selected_npc(selected: &Selected, sources: &Sources) -> Option<(u32, Arc<Records<'static>>)> {
    let Some(Sel::Record(RecordType::Npc, id)) = selected.sel else {
        return None;
    };
    match sources.list.get(selected.source?)? {
        Source::Cache { records, .. } => Some((id, records.clone())),
        _ => None,
    }
}

/// The Captions window: the settings, a connection test, and the caption
/// action for the selected NPC. Saves the settings when they change. A
/// window rather than a menu, since a menu closes on the first click
/// inside it.
pub fn captions_window(
    ctx: &egui::Context,
    captions: &mut Captions,
    settings: &mut Settings,
    selected: &Selected,
    sources: &Sources,
    jobs: &mut Jobs,
) {
    let mut open = captions.open;
    egui::Window::new("Captions")
        .open(&mut open)
        .default_width(420.0)
        .resizable(true)
        .show(ctx, |ui| {
        let before = settings.caption.clone();
        let c = &mut settings.caption;
        ui.checkbox(&mut c.enabled, "Captioning on")
            .on_hover_text("Nothing is sent anywhere until this is on.");
        ui.weak("Captions describe candidate sequences, to check their roles by eye. They stay on this machine and never go into a pack.");
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("API");
            ui.radio_value(&mut c.backend, Backend::Ollama, "Ollama");
            ui.radio_value(&mut c.backend, Backend::OpenAi, "OpenAI-compatible");
        });
        egui::Grid::new("caption settings")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("Server");
                ui.text_edit_singleline(&mut c.endpoint);
                ui.end_row();
                ui.label("Model");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut c.model);
                    if let Some(Ok(models)) = &captions.test {
                        egui::ComboBox::from_id_salt("caption model")
                            .selected_text("pick")
                            .show_ui(ui, |ui| {
                                for m in models {
                                    ui.selectable_value(&mut c.model, m.clone(), m);
                                }
                            });
                    }
                });
                ui.end_row();
                ui.label("API key variable");
                let mut var = c.api_key_env.clone().unwrap_or_default();
                if ui
                    .text_edit_singleline(&mut var)
                    .on_hover_text("The environment variable holding a key, if the server wants one. The key itself isn't stored.")
                    .changed()
                {
                    c.api_key_env = (!var.trim().is_empty()).then(|| var.trim().to_owned());
                }
                ui.end_row();
                ui.label("Timeout");
                ui.add(egui::DragValue::new(&mut c.timeout_secs).range(1..=3600).suffix(" s"));
                ui.end_row();
                ui.label("Frames per image");
                ui.add(egui::Slider::new(&mut c.frames, 1..=16));
                ui.end_row();
            });
        if !is_local(&c.endpoint) {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                "⚠ This server isn't this machine: images will be sent to it.",
            );
        }
        let mut own = c.prompt.is_some();
        if ui.checkbox(&mut own, "Ask my own question").changed() {
            c.prompt = own.then(|| DEFAULT_PROMPT.to_owned());
        }
        if let Some(p) = &mut c.prompt {
            ui.add(
                egui::TextEdit::multiline(p)
                    .desired_rows(6)
                    .desired_width(f32::INFINITY),
            );
            ui.weak("{frames} and {classes} are filled in.");
        }
        // A different server or key makes the last test meaningless.
        let reach = |c: &Caption| (c.backend, c.endpoint.clone(), c.api_key_env.clone());
        if reach(c) != reach(&before) {
            captions.test = None;
        }
        if *c != before
            && let Some(p) = settings_path()
        {
            // Not being able to remember it isn't worth bothering you about.
            let _ = settings.save(&p);
        }
        let c = settings.caption.clone();

        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    c.enabled && captions.testing.is_none(),
                    egui::Button::new("Test connection"),
                )
                .on_disabled_hover_text("Turn captioning on first.")
                .clicked()
            {
                // Listing models doesn't need one chosen.
                let probe = Caption {
                    model: if c.model.trim().is_empty() { "-".into() } else { c.model.clone() },
                    ..c.clone()
                };
                captions.testing = Some(AsyncComputeTaskPool::get().spawn(async move {
                    Client::new(&probe)
                        .and_then(|cl| cl.models())
                        .map_err(|e| format!("{e:#}"))
                }));
            }
            match (&captions.testing, &captions.test) {
                (Some(_), _) => {
                    ui.spinner();
                }
                (None, Some(Ok(models))) if !c.model.is_empty() && !models.contains(&c.model) => {
                    ui.colored_label(
                        ui.visuals().warn_fg_color,
                        format!("connected, but there's no model {}", c.model),
                    );
                }
                (None, Some(Ok(models))) => {
                    ui.label(format!("✔ connected: {} models", models.len()));
                }
                (None, Some(Err(e))) => {
                    ui.add(
                        egui::Label::new(egui::RichText::new(e).color(egui::Color32::LIGHT_RED))
                            .wrap(),
                    );
                }
                (None, None) => {}
            }
        });

        ui.separator();
        let npc = selected_npc(selected, sources);
        ui.horizontal(|ui| {
            ui.label("At most");
            ui.add(egui::DragValue::new(&mut captions.limit).range(0..=100_000));
            ui.weak(if captions.limit == 0 { "(every candidate)" } else { "images" });
        });
        let label = npc
            .as_ref()
            .map_or("Caption the selected NPC's candidates".into(), |(id, _)| {
                format!("Caption npc {id}'s candidates")
            });
        // Everything still in the way, in the order to do it.
        let missing: Vec<&str> = [
            (!c.enabled, "turn captioning on"),
            (c.model.trim().is_empty(), "choose a model"),
            (!captions.connected(), "pass a connection test"),
            (npc.is_none(), "select an NPC in a cache"),
            (captions.store.is_none(), "wait for the caption file to open"),
            (captions.running(), "wait for the running job"),
        ]
        .into_iter()
        .filter_map(|(blocked, what)| blocked.then_some(what))
        .collect();
        if ui
            .add_enabled(missing.is_empty(), egui::Button::new(&label))
            .clicked()
            && let (Some((id, records)), Some(store)) = (npc, captions.store.clone())
        {
            let stop = Arc::new(AtomicBool::new(false));
            let finished = Arc::new(AtomicBool::new(false));
            captions.run = Some(Run {
                stop: stop.clone(),
                finished: finished.clone(),
            });
            let limit = captions.limit;
            jobs.spawn_job(label, 0, move |progress| {
                let r = caption_job(&records, &c, &store, id, limit, stop, &progress);
                finished.store(true, Ordering::Relaxed);
                r
            });
        }
        if !missing.is_empty() {
            ui.weak(format!("To caption: {}.", missing.join(", ")));
        }
        if let Some(run) = captions.run.as_ref().filter(|_| captions.running())
            && ui.button("Stop captioning").clicked()
        {
            run.stop.store(true, Ordering::Relaxed);
        }
        if let Some(store) = &captions.store {
            let st = lock(store);
            ui.weak(format!(
                "{} captions, kept in {}",
                st.entries().count(),
                st.path().display()
            ));
        }
    });
    captions.open = open;
}

/// The chosen sequence's caption under the playback bar, with buttons to
/// mark it right or wrong and to correct its class. Draws nothing without
/// a caption for it.
pub fn review_ui(ui: &mut egui::Ui, captions: &Captions, model: &str, npc: u32, seq: u32) {
    let Some(store) = &captions.store else { return };
    let mut st = lock(store);
    let Some(e) = st.lookup(npc, seq, model).cloned() else {
        return;
    };
    let mut review = e.review.clone();
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(tag(&e)).strong());
        ui.add(egui::Label::new(&e.text).wrap())
            .on_hover_text(format!(
                "{}\n\n{} frames, by {}",
                e.reply, e.frames, e.model
            ));
        if e.npc != npc {
            ui.weak(format!("(drawn on npc {})", e.npc));
        }
        if ui
            .selectable_label(
                matches!(review, Some(Review { right: true, .. })),
                "✔ Right",
            )
            .clicked()
        {
            review = Some(Review {
                right: true,
                class: None,
            });
        }
        let wrong = matches!(review, Some(Review { right: false, .. }));
        if ui.selectable_label(wrong, "✘ Wrong").clicked() {
            review = Some(Review {
                right: false,
                class: None,
            });
        }
        if wrong {
            let mut class = review.as_ref().and_then(|r| r.class.clone());
            egui::ComboBox::from_id_salt("caption class")
                .selected_text(class.as_deref().unwrap_or("it's really…"))
                .show_ui(ui, |ui| {
                    for (name, gloss) in CLASSES {
                        ui.selectable_value(&mut class, Some(name.to_owned()), name)
                            .on_hover_text(gloss);
                    }
                });
            review = Some(Review {
                right: false,
                class,
            });
        }
        if review.is_some() && ui.small_button("clear").clicked() {
            review = None;
        }
    });
    if review != e.review {
        st.review(e.npc, e.seq, &e.model, review);
        if let Err(err) = st.save() {
            tracing::warn!("captions: {err:#}");
        }
    }
}

/// Adds [`Captions`] and the systems that open the store and collect the
/// connection test.
pub fn plugin(app: &mut App) {
    app.init_resource::<Captions>()
        .add_systems(Update, (load_store, poll_test));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_show_the_review() {
        let mut e = Entry {
            npc: 1,
            seq: 2,
            model: "m".into(),
            frames: 6,
            text: "falls".into(),
            class: Some("fall over".into()),
            reply: String::new(),
            review: None,
        };
        assert_eq!(tag(&e), "💬 fall over");
        e.review = Some(Review {
            right: true,
            class: None,
        });
        assert_eq!(tag(&e), "✔ fall over");
        e.review = Some(Review {
            right: false,
            class: Some("death".into()),
        });
        assert_eq!(tag(&e), "✎ death");
        e.review = Some(Review {
            right: false,
            class: None,
        });
        assert_eq!(tag(&e), "✘ fall over");
    }
}
