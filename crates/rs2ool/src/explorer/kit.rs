//! The player kit: the default body built from identity-kit parts, with
//! the player stance sequences, dressed through the client's 12 equipment
//! slots. Each slot shows a kit part where the client has one by default,
//! or an item's equipment model when an item id is typed into it.

use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use bevy_egui::egui;
use rs2cache::Records;
use rs2cache::types::model::Model;
use rs2convert::creature::{self, Creature};
use rs2convert::pipeline::{self, KitPart};

use crate::explorer::renderable::Renderable;
use crate::explorer::selection::{Loaded, Sel, Selected};
use crate::explorer::sources::{Source, Sources};

/// The client's equipment slots.
pub const SLOTS: [&str; 12] = [
    "head", "cape", "amulet", "weapon", "body", "shield", "arms", "legs", "hair", "hands", "feet",
    "jaw",
];

/// The identity-kit body part each equipment slot shows when empty, as the
/// client's default appearance: body → torso, arms, legs, hair → head,
/// hands, feet, jaw.
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
    pub records: Arc<Records<'static>>,
    pub parts: Vec<(KitPart, Model)>,
    pub creature: Arc<Creature>,
}

/// What one equipment slot wears.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Slot {
    /// The identity-kit part shown when no item is worn.
    pub kit: Option<u32>,
    /// An item id; 0 for none.
    pub item: u32,
}

impl KitBase {
    /// The parts that can fill kit body part `part` for a body type.
    pub fn choices(&self, part: u8, female: bool) -> impl Iterator<Item = &KitPart> {
        let body = u8::from(female);
        self.parts
            .iter()
            .map(|(p, _)| p)
            .filter(move |p| p.slot == part && p.body_type == body && p.selectable)
    }

    /// The client's default appearance: each kit-backed slot's first
    /// selectable part, no items.
    pub fn defaults(&self, female: bool) -> [Slot; 12] {
        SLOT_KIT.map(|k| Slot {
            kit: k.and_then(|part| self.choices(part, female).next().map(|p| p.idk)),
            item: 0,
        })
    }

    /// The player dressed as `slots`: each slot's item equipment model, or
    /// else its kit part, merged as the client merges a player's models,
    /// with the player sequences.
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
            [128; 3],
            self.creature.sequences.clone(),
            self.creature.packs.clone(),
        );
        let model = c.rest_model();
        Ok(Renderable::Creature {
            creature: Arc::new(c),
            model,
        })
    }
}

/// Load the kit parts and the player creature.
pub fn load_base(records: Arc<Records<'static>>) -> Result<KitBase, String> {
    let index = records.cache().frame_index().map_err(|e| e.to_string())?;
    let (parts, creature) = pipeline::player_kit(&records, &index).map_err(|e| e.to_string())?;
    Ok(KitBase {
        records,
        parts,
        creature: Arc::new(creature),
    })
}

/// The player kit's state while it is selected.
#[derive(Resource, Default)]
pub struct Kit {
    pub base: Option<Arc<KitBase>>,
    pub slots: [Slot; 12],
    pub female: bool,
    pub error: Option<String>,
    /// Rebuild the model on the next frame.
    pub dirty: bool,
    active: bool,
    /// The last dressed model, reapplied if the selection's own load lands
    /// after it (the kit has no record to draw by itself).
    built: Option<Arc<Renderable>>,
    loading: Option<Task<Result<KitBase, String>>>,
    building: Option<Task<Result<Renderable, String>>>,
}

impl Kit {
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
    if loaded.renderable.is_none()
        && matches!(loaded.for_sel, Some((_, Sel::PlayerKit)))
        && let Some(b) = &kit.built
    {
        loaded.renderable = Some(b.clone());
    }
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

/// The kit's controls: body type, and per slot its kit part and item id.
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
            kit.slots = base.defaults(kit.female);
            changed = true;
        }
        if ui.button("Reset").clicked() {
            kit.slots = base.defaults(kit.female);
            changed = true;
        }
    });
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
                .on_hover_text("an item id to wear in this slot (0 for none)")
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

pub fn plugin(app: &mut App) {
    app.init_resource::<Kit>().add_systems(
        Update,
        (follow, update)
            .chain()
            .after(crate::explorer::selection::LoadSet),
    );
}
