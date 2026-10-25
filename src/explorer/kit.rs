//! The player kit: the default body built from identity-kit parts, with
//! the player stance sequences, dressed through the original client's 12
//! equipment slots.
//!
//! Each slot shows a kit part where the original client has one by
//! default, or an item's equipment model when you type an item id into it.
//! The parts load once, in the background, the first time the kit is
//! selected; after that every change rebuilds the dressed model in the
//! background too.

use std::collections::BTreeMap;
use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use bevy_egui::egui;
use rs2cache::Records;
use rs2cache::types::model::Model;
use rs2convert::creature::{self, Creature};
use rs2convert::pipeline::{self, KitPart};
use rs2convert::slot::SlotGuess;

use crate::explorer::renderable::Renderable;
use crate::explorer::selection::{Loaded, Sel, Selected};
use crate::explorer::sources::{Source, Sources};

/// The original client's equipment slots, in its order.
pub const SLOTS: [&str; 12] = [
    "head", "cape", "amulet", "weapon", "body", "shield", "arms", "legs", "hair", "hands", "feet",
    "jaw",
];

/// The identity-kit body part each equipment slot shows when empty, as in
/// the original client's default appearance. Parallel to [`SLOTS`]:
/// body → 2 (torso), arms → 3, legs → 5, hair → 0 (head), hands → 4,
/// feet → 6, jaw → 1. `None` for slots only items fill.
pub const SLOT_KIT: [Option<u8>; 12] = [
    None,
    None,
    None,
    None,
    Some(2),
    None,
    Some(3),
    Some(5),
    Some(0),
    Some(4),
    Some(6),
    Some(1),
];

/// The kit parts and the player creature they start from, loaded once.
pub struct KitBase {
    /// The cache the parts came from, for looking up item models.
    pub records: Arc<Records<'static>>,
    /// Every identity-kit part with its merged model.
    pub parts: Vec<(KitPart, Model)>,
    /// The default player, whose sequences and frame packs every dressed
    /// model reuses.
    pub creature: Arc<Creature>,
    /// Every item with a wear model, with its inferred slot.
    pub guesses: BTreeMap<u32, SlotGuess>,
}

/// What one equipment slot wears.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Slot {
    /// The identity-kit part (its idk id) shown when no item is worn.
    pub kit: Option<u32>,
    /// An item id; 0 for none.
    pub item: u32,
}

impl KitBase {
    /// The selectable parts that can fill kit body part `part` for a body
    /// type, in cache order.
    pub fn choices(&self, part: u8, female: bool) -> impl Iterator<Item = &KitPart> {
        // Body type 0 is male, 1 female.
        let body = u8::from(female);
        self.parts
            .iter()
            .map(|(p, _)| p)
            .filter(move |p| p.slot == part && p.body_type == body && p.selectable)
    }

    /// The kit view's slot (an index into [`SLOTS`]) item `item` goes in,
    /// by its inferred slot, or why it can't be placed.
    pub fn slot_of(&self, item: u32) -> Result<usize, String> {
        let g = self
            .guesses
            .get(&item)
            .ok_or_else(|| format!("item {item} has no wear model"))?;
        let slot = g
            .slot
            .ok_or_else(|| format!("item {item}'s wear models don't point at a slot"))?;
        SLOTS
            .iter()
            .position(|s| *s == slot.name())
            .ok_or_else(|| format!("no {} slot", slot.name()))
    }

    /// A line on how item `item`'s slot was inferred.
    pub fn explain(&self, item: u32) -> String {
        let Some(g) = self.guesses.get(&item) else {
            return format!("item {item} has no wear model");
        };
        let slot = g.slot.map_or("no slot", |s| s.name());
        let held = if g.wearable {
            ""
        } else {
            "; it has no wear option, so it's only held during animations"
        };
        let hands = if g.both_hands { ", both hands" } else { "" };
        format!(
            "item {item}: {slot}{hands}, from wear-model labels {:?}{held}",
            g.labels
        )
    }

    /// The default appearance: each kit-backed slot's first selectable
    /// part, no items.
    pub fn defaults(&self, female: bool) -> [Slot; 12] {
        SLOT_KIT.map(|k| Slot {
            kit: k.and_then(|part| self.choices(part, female).next().map(|p| p.idk)),
            item: 0,
        })
    }

    /// The player dressed as `slots`: each slot's item equipment model, or
    /// else its kit part, merged as the original client merges a player's
    /// models, with the player sequences.
    ///
    /// Fails if an item won't decode. An item with no equipment model for
    /// the body type just leaves the slot empty.
    pub fn build(&self, slots: &[Slot; 12], female: bool) -> Result<Renderable, String> {
        let mut models = Vec::new();
        for s in slots {
            if s.item != 0 {
                if let Some(m) = creature::equip_model(&self.records, s.item, female)
                    .map_err(|e| format!("item {}: {e}", s.item))?
                {
                    models.push(m);
                }
            } else if let Some(k) = s.kit
                && let Some((_, m)) = self.parts.iter().find(|(p, _)| p.idk == k)
            {
                models.push(m.clone());
            }
        }
        let merged = creature::merge(&models);
        let c = Creature::new(
            "player".into(),
            merged,
            // Scale 128 = 1.0 on every axis.
            [128; 3],
            self.creature.sequences.clone(),
            self.creature.packs.clone(),
        );
        let model = c.rest_model();
        let rig = rs2convert::rig::infer(&c, None).ok().map(Arc::new);
        Ok(Renderable::Creature {
            creature: Arc::new(c),
            model,
            inferred: Vec::new(),
            rig,
        })
    }
}

/// Loads the kit parts and the player creature. Blocking; the explorer
/// runs it as a task.
pub fn load_base(records: Arc<Records<'static>>) -> Result<KitBase, String> {
    let index = records.cache().frame_index().map_err(|e| e.to_string())?;
    let kit = pipeline::player_kit(&records, &index).map_err(|e| e.to_string())?;
    // Every dressed model reuses the default body's sequences, so either
    // body would do.
    let (parts, creature) = (kit.parts, kit.male);
    let guesses = rs2convert::slot::guesses(&records).map_err(|e| e.to_string())?;
    Ok(KitBase {
        records,
        parts,
        creature: Arc::new(creature),
        guesses,
    })
}

/// The player kit's state while it is selected.
#[derive(Resource, Default)]
pub struct Kit {
    /// The loaded parts; `None` until they've loaded.
    pub base: Option<Arc<KitBase>>,
    /// What each slot wears, parallel to [`SLOTS`].
    pub slots: [Slot; 12],
    /// Body type: `false` male, `true` female.
    pub female: bool,
    /// Why the last load or build failed, if it did.
    pub error: Option<String>,
    /// Rebuild the model on the next frame.
    pub dirty: bool,
    /// The item id typed into "Wear item".
    pub wear_item: u32,
    /// What the last "Wear item" did.
    pub wear_note: Option<String>,
    // Whether the kit is the current selection.
    active: bool,
    /// The last dressed model, reapplied if the selection's own load lands
    /// after it (the kit has no record to draw by itself).
    built: Option<Arc<Renderable>>,
    // The one-off parts load, while it runs.
    loading: Option<Task<Result<KitBase, String>>>,
    // A rebuild of the dressed model, while it runs.
    building: Option<Task<Result<Renderable, String>>>,
}

impl Kit {
    /// Whether the player kit is the current selection.
    pub fn active(&self) -> bool {
        self.active
    }
}

/// Selecting the player kit loads its parts once; selecting anything else
/// leaves the kit.
fn follow(selected: Res<Selected>, sources: Res<Sources>, mut kit: ResMut<Kit>) {
    if !selected.is_changed() {
        return;
    }
    let records = match (
        selected.sel.as_ref(),
        selected.source.and_then(|s| sources.list.get(s)),
    ) {
        (Some(Sel::PlayerKit), Some(Source::Cache { records, .. })) => records.clone(),
        _ => {
            kit.active = false;
            kit.building = None;
            kit.built = None;
            return;
        }
    };
    kit.active = true;
    kit.dirty = true;
    if kit.base.is_none() && kit.loading.is_none() {
        kit.loading = Some(AsyncComputeTaskPool::get().spawn(async move { load_base(records) }));
    }
}

/// Poll the loads and rebuild the dressed model when the slots change; the
/// result becomes the loaded selection's renderable, so every viewport and
/// the playback bar pick it up.
fn update(mut kit: ResMut<Kit>, mut loaded: ResMut<Loaded>) {
    // The UI polls `Kit` every frame anyway, so skip change tracking.
    let kit = kit.bypass_change_detection();
    if let Some(t) = &mut kit.loading
        && let Some(r) = block_on(poll_once(t))
    {
        kit.loading = None;
        match r {
            Ok(base) => {
                kit.slots = base.defaults(kit.female);
                kit.base = Some(Arc::new(base));
                kit.dirty = true;
            }
            Err(e) => kit.error = Some(e),
        }
    }
    if !kit.active {
        return;
    }
    // The selection's own load clears the renderable when it lands; put
    // the last dressed model back.
    if loaded.renderable.is_none()
        && matches!(loaded.for_sel, Some((_, Sel::PlayerKit)))
        && let Some(b) = &kit.built
    {
        loaded.renderable = Some(b.clone());
    }
    // One build at a time; changes made meanwhile wait for the next one.
    if kit.dirty
        && kit.building.is_none()
        && let Some(base) = kit.base.clone()
    {
        kit.dirty = false;
        let (slots, female) = (kit.slots, kit.female);
        kit.building =
            Some(AsyncComputeTaskPool::get().spawn(async move { base.build(&slots, female) }));
    }
    if let Some(t) = &mut kit.building
        && let Some(r) = block_on(poll_once(t))
    {
        kit.building = None;
        match r {
            Ok(r) => {
                kit.error = None;
                let r = Arc::new(r);
                kit.built = Some(r.clone());
                loaded.renderable = Some(r);
            }
            Err(e) => kit.error = Some(e),
        }
    }
}

/// The kit's controls: body type, Reset, and per slot its kit part and
/// item id. Draws nothing unless the kit is selected.
pub fn kit_ui(ui: &mut egui::Ui, kit: &mut Kit) {
    if !kit.active {
        return;
    }
    ui.strong("Player kit");
    let Some(base) = kit.base.clone() else {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("loading kit parts…");
        });
        return;
    };
    let mut changed = false;
    ui.horizontal(|ui| {
        let before = kit.female;
        ui.radio_value(&mut kit.female, false, "male");
        ui.radio_value(&mut kit.female, true, "female");
        if kit.female != before {
            // Parts are per body type, so switching starts over.
            kit.slots = base.defaults(kit.female);
            changed = true;
        }
        if ui.button("Reset").clicked() {
            kit.slots = base.defaults(kit.female);
            changed = true;
        }
    });
    // An item into whichever slot its wear models point at.
    ui.horizontal(|ui| {
        ui.add(
            egui::DragValue::new(&mut kit.wear_item)
                .range(0..=65535)
                .prefix("item "),
        );
        if ui
            .button("Wear in its slot")
            .on_hover_text("put the item in the slot its wear models point at")
            .clicked()
        {
            let item = kit.wear_item;
            match base.slot_of(item) {
                Ok(i) => {
                    kit.slots[i].item = item;
                    changed = true;
                }
                Err(e) => kit.error = Some(e),
            }
            kit.wear_note = Some(base.explain(item));
        }
    });
    if let Some(note) = &kit.wear_note {
        ui.weak(note);
    }
    egui::Grid::new("kit_slots").num_columns(3).show(ui, |ui| {
        for (i, name) in SLOTS.iter().enumerate() {
            ui.label(*name);
            match SLOT_KIT[i] {
                Some(part) => {
                    let mut k = kit.slots[i].kit;
                    egui::ComboBox::from_id_salt(("kit", i))
                        .selected_text(k.map_or("none".into(), |k| format!("kit {k}")))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut k, None, "none");
                            for p in base.choices(part, kit.female) {
                                ui.selectable_value(&mut k, Some(p.idk), format!("kit {}", p.idk));
                            }
                        });
                    if k != kit.slots[i].kit {
                        kit.slots[i].kit = k;
                        changed = true;
                    }
                }
                None => {
                    ui.weak("—");
                }
            }
            let mut item = kit.slots[i].item;
            if ui
                .add(
                    egui::DragValue::new(&mut item)
                        .range(0..=65535)
                        .prefix("item "),
                )
                .on_hover_text(if item == 0 {
                    "an item id to wear in this slot (0 for none)".to_owned()
                } else {
                    base.explain(item)
                })
                .changed()
            {
                kit.slots[i].item = item;
                changed = true;
            }
            ui.end_row();
        }
    });
    if let Some(e) = &kit.error {
        ui.colored_label(egui::Color32::LIGHT_RED, e);
    }
    if changed {
        kit.dirty = true;
    }
}

/// Adds [`Kit`] and its systems, which run after the selection loaders.
pub fn plugin(app: &mut App) {
    app.init_resource::<Kit>().add_systems(
        Update,
        (follow, update)
            .chain()
            .after(crate::explorer::selection::LoadSet),
    );
}
