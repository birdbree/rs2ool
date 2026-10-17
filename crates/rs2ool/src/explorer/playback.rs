//! Animation playback: a frame clock driven by sequence delays (20 ms per
//! client cycle), shared by both compare panes so a cache creature and its
//! converted version play the same clip in step. Cache creatures pose
//! through the client's deformation (`Creature::posed`), converted ones
//! through their rig and clip (`validate::skinned`).

use std::sync::Arc;

use bevy::prelude::*;
use bevy_egui::egui;
use contentpack::records::anim::NO_LOOP;

use crate::explorer::classic::ClassicView;
use crate::explorer::renderable::Renderable;
use crate::explorer::selection::{CompareSelected, Loaded};
use crate::explorer::viewport3d::{Overlays, Pose, Shown, ShownModel, posed_parts};

/// Milliseconds per client cycle.
pub const CYCLE_MS: f32 = 20.0;

/// Steps through frames using per-frame delays in client cycles.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Clock {
    pub delays: Vec<u16>,
    /// Frames from the end to replay when wrapping; `None` restarts at 0.
    pub loop_offset: Option<u16>,
    pub frame: usize,
    pub elapsed_ms: f32,
    pub playing: bool,
    pub looping: bool,
}

impl Clock {
    pub fn new(delays: Vec<u16>, loop_offset: Option<u16>) -> Self {
        Self {
            delays,
            loop_offset,
            frame: 0,
            elapsed_ms: 0.0,
            playing: true,
            looping: true,
        }
    }

    fn frame_ms(&self) -> f32 {
        f32::from(self.delays.get(self.frame).copied().unwrap_or(1).max(1)) * CYCLE_MS
    }

    /// Advance by `dt_ms`, stepping frames as their delays run out.
    pub fn advance(&mut self, dt_ms: f32) {
        if !self.playing || self.delays.is_empty() {
            return;
        }
        self.elapsed_ms += dt_ms;
        while self.elapsed_ms >= self.frame_ms() {
            self.elapsed_ms -= self.frame_ms();
            self.frame += 1;
            if self.frame >= self.delays.len() {
                let n = self.delays.len();
                if !self.looping {
                    self.frame = n - 1;
                    self.playing = false;
                    self.elapsed_ms = 0.0;
                    return;
                }
                self.frame = match self.loop_offset {
                    Some(o) if o > 0 && usize::from(o) <= n => n - usize::from(o),
                    _ => 0,
                };
            }
        }
    }

    /// Jump to `frame` (clamped).
    pub fn scrub(&mut self, frame: usize) {
        self.frame = frame.min(self.delays.len().saturating_sub(1));
        self.elapsed_ms = 0.0;
    }

    /// Total length in milliseconds.
    pub fn duration_ms(&self) -> f32 {
        self.delays
            .iter()
            .map(|&d| f32::from(d.max(1)) * CYCLE_MS)
            .sum()
    }

    /// Milliseconds from the start to the current position.
    pub fn position_ms(&self) -> f32 {
        self.delays[..self.frame.min(self.delays.len())]
            .iter()
            .map(|&d| f32::from(d.max(1)) * CYCLE_MS)
            .sum::<f32>()
            + self.elapsed_ms
    }
}

/// The sequence ids a renderable can play, in order.
pub fn sequences(r: &Renderable) -> Vec<u32> {
    match r {
        Renderable::Creature { creature, .. } => {
            creature.sequences.iter().map(|s| s.def.id).collect()
        }
        Renderable::PackCreature { clips, .. } => clips.iter().map(|(id, _)| id & 0xffff).collect(),
        _ => Vec::new(),
    }
}

/// Sequence `seq`'s frame delays (client cycles) and loop offset for a
/// renderable. A cache delay of 0 falls back to the frame's own delay.
pub fn timing(r: &Renderable, seq: u32) -> Option<(Vec<u16>, Option<u16>)> {
    match r {
        Renderable::Creature { creature, .. } => {
            let s = creature.sequences.iter().find(|s| s.def.id == seq)?;
            let delays = (0..s.def.frames.len())
                .map(|i| match s.def.frames[i].delay {
                    0 => creature.frame(s, i).map_or(1, |(_, f)| u16::from(f.delay)),
                    d => d,
                })
                .collect();
            Some((delays, s.def.loop_offset))
        }
        Renderable::PackCreature { clips, .. } => {
            let (_, c) = clips.iter().find(|(id, _)| id & 0xffff == seq)?;
            let ends = c.frame_start.iter().skip(1).copied().chain([c.duration]);
            let delays = c
                .frame_start
                .iter()
                .zip(ends)
                .map(|(a, b)| b.saturating_sub(*a))
                .collect();
            let lo = (c.loop_offset != NO_LOOP).then_some(c.loop_offset);
            Some((delays, lo))
        }
        _ => None,
    }
}

/// Vertices for a renderable at frame `frame` of sequence `seq` (clamped
/// to its length), if it plays that sequence.
pub enum Posed {
    Cache(Vec<[i32; 3]>),
    Pack(Vec<[f32; 3]>),
}

impl Posed {
    pub fn pose(&self) -> Pose<'_> {
        match self {
            Posed::Cache(v) => Pose::Cache(v),
            Posed::Pack(v) => Pose::Pack(v),
        }
    }
}

pub fn posed(r: &Renderable, seq: u32, frame: usize) -> Option<Posed> {
    match r {
        Renderable::Creature { creature, .. } => {
            let s = creature.sequences.iter().position(|s| s.def.id == seq)?;
            let n = creature.sequences[s].def.frames.len();
            creature
                .posed(s, frame.min(n.saturating_sub(1)))
                .map(Posed::Cache)
        }
        Renderable::PackCreature {
            model, rig, clips, ..
        } => {
            let (_, c) = clips.iter().find(|(id, _)| id & 0xffff == seq)?;
            let t = frame.min(c.frame_start.len().saturating_sub(1)) as u8;
            Some(Posed::Pack(rs2convert::validate::skinned(model, rig, c, t)))
        }
        _ => None,
    }
}

/// What is playing.
#[derive(Resource, Default)]
pub struct Playback {
    /// Sequence ids the main renderable can play.
    pub seqs: Vec<u32>,
    pub current: usize,
    pub clock: Clock,
    for_main: usize,
    shown: Option<PoseKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PoseKey {
    main: usize,
    compare: usize,
    seq: u32,
    frame: usize,
    labels: bool,
    main_parts: usize,
    compare_parts: usize,
}

impl Playback {
    pub fn seq(&self) -> Option<u32> {
        self.seqs.get(self.current).copied()
    }

    /// Play sequence index `i` from its start.
    pub fn choose(&mut self, i: usize, r: &Renderable) {
        self.current = i.min(self.seqs.len().saturating_sub(1));
        let (delays, lo) = self.seq().and_then(|s| timing(r, s)).unwrap_or_default();
        let (playing, looping) = (
            self.clock.playing || self.clock.delays.is_empty(),
            self.clock.looping || self.clock.delays.is_empty(),
        );
        self.clock = Clock::new(delays, lo);
        self.clock.playing = playing;
        self.clock.looping = looping;
    }
}

fn ptr(r: Option<&Arc<Renderable>>) -> usize {
    r.map_or(0, |r| Arc::as_ptr(r) as usize)
}

/// A new main selection lists its sequences and starts the first.
fn follow(loaded: Res<Loaded>, mut pb: ResMut<Playback>) {
    let key = ptr(loaded.renderable.as_ref());
    if pb.for_main == key {
        return;
    }
    pb.for_main = key;
    pb.seqs = loaded
        .renderable
        .as_deref()
        .map(sequences)
        .unwrap_or_default();
    pb.clock = Clock::default();
    if let Some(r) = loaded.renderable.as_deref() {
        pb.choose(0, r);
    }
}

fn tick(time: Res<Time>, mut pb: ResMut<Playback>) {
    if pb.clock.playing && !pb.clock.delays.is_empty() {
        pb.clock.advance(time.delta_secs().min(0.25) * 1000.0);
    }
}

/// Pose the shown models at the clock's frame when it moves: both panes
/// from the one clock; the classic renderer gets the main pose, relit as
/// the client relights an animated model.
fn apply(
    loaded: Res<Loaded>,
    right: Res<Loaded<CompareSelected>>,
    overlays: Res<Overlays>,
    shown: Res<Shown>,
    mut pb: ResMut<Playback>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut classic: ResMut<ClassicView>,
) {
    let Some(seq) = pb.seq() else {
        if classic.pose.is_some() {
            classic.pose = None;
        }
        return;
    };
    let key = PoseKey {
        main: ptr(loaded.renderable.as_ref()),
        compare: ptr(right.renderable.as_ref()),
        seq,
        frame: pb.clock.frame,
        labels: overlays.labels,
        main_parts: shown.main.parts.len(),
        compare_parts: shown.compare.parts.len(),
    };
    if pb.shown == Some(key) {
        return;
    }
    pb.shown = Some(key);
    let frame = pb.clock.frame;
    let mut pose_side = |r: Option<&Arc<Renderable>>, side: &ShownModel| {
        let r = r?;
        let p = posed(r, seq, frame)?;
        let (client, real, _) = posed_parts(r, overlays.labels, p.pose());
        for ((part, c), rl) in side.parts.iter().zip(client).zip(real) {
            if let Some(mut m) = meshes.get_mut(&part.client) {
                *m = c.mesh;
            }
            if let Some(mut m) = meshes.get_mut(&part.real) {
                *m = rl.mesh;
            }
        }
        Some((r.clone(), p))
    };
    let main = pose_side(loaded.renderable.as_ref(), &shown.main);
    pose_side(right.renderable.as_ref(), &shown.compare);
    classic.pose = main.and_then(|(r, p)| match (r.model(), p) {
        (Some(m), Posed::Cache(v)) => {
            let model = rs2cache::types::model::Model {
                vertices: v.clone(),
                ..m.clone()
            };
            Some((
                Arc::new(rs2classic::light(&model, &rs2classic::light::NPC)),
                v,
            ))
        }
        _ => None,
    });
}

/// The state picker for an object with states, and the shown state's
/// options. Returns the newly chosen state.
pub fn states_ui(ui: &mut egui::Ui, main: &Loaded) -> Option<usize> {
    let states = main.states.as_ref()?;
    let mut chosen = main.state;
    ui.horizontal_wrapped(|ui| {
        egui::ComboBox::from_id_salt("object_state")
            .selected_text(states.labels.get(main.state).cloned().unwrap_or_default())
            .show_ui(ui, |ui| {
                for (i, l) in states.labels.iter().enumerate() {
                    ui.selectable_value(&mut chosen, i, l);
                }
            });
        let options = states
            .options
            .get(main.state)
            .map_or(&[][..], Vec::as_slice);
        if options.is_empty() {
            ui.weak("no options");
        } else {
            ui.label(format!("Options: {}", options.join(", ")));
        }
        if states
            .renderables
            .get(main.state)
            .is_some_and(Option::is_none)
        {
            ui.weak("(this state draws nothing)");
        }
    });
    (chosen != main.state).then_some(chosen)
}

/// Show state `i` of a loaded object, if it has that state.
pub fn apply_state<S>(l: &mut Loaded<S>, i: usize) {
    if let Some(s) = l.states.clone()
        && i < s.renderables.len()
    {
        l.state = i;
        l.renderable = s.renderables[i].clone();
    }
}

/// The playback bar: sequence, play/pause, loop and a frame scrubber.
pub fn playback_ui(ui: &mut egui::Ui, pb: &mut Playback, main: Option<&Renderable>) {
    let Some(r) = main.filter(|_| !pb.seqs.is_empty()) else {
        return;
    };
    ui.horizontal_wrapped(|ui| {
        let mut chosen = pb.current;
        egui::ComboBox::from_id_salt("sequence")
            .selected_text(pb.seq().map_or(String::new(), |s| format!("seq {s}")))
            .show_ui(ui, |ui| {
                for (i, s) in pb.seqs.iter().enumerate() {
                    ui.selectable_value(&mut chosen, i, format!("seq {s}"));
                }
            });
        if chosen != pb.current {
            pb.choose(chosen, r);
        }
        let label = if pb.clock.playing {
            "⏸ Pause"
        } else {
            "▶ Play"
        };
        if ui.button(label).clicked() {
            if !pb.clock.playing && !pb.clock.looping && pb.clock.frame + 1 >= pb.clock.delays.len()
            {
                pb.clock.scrub(0);
            }
            pb.clock.playing = !pb.clock.playing;
        }
        ui.checkbox(&mut pb.clock.looping, "Loop");
        let n = pb.clock.delays.len();
        let mut f = pb.clock.frame;
        if ui
            .add(egui::Slider::new(&mut f, 0..=n.saturating_sub(1)).text(format!("of {n}")))
            .changed()
        {
            pb.clock.scrub(f);
            pb.clock.playing = false;
        }
        ui.weak(format!(
            "{:.0} / {:.0} ms",
            pb.clock.position_ms(),
            pb.clock.duration_ms()
        ));
    });
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Playback>().add_systems(
        Update,
        (follow, tick, apply)
            .chain()
            .after(crate::explorer::viewport3d::ShowSet),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_steps_by_delays_and_loops() {
        let mut c = Clock::new(vec![2, 3, 1], None);
        c.advance(39.0);
        assert_eq!(c.frame, 0);
        c.advance(1.0);
        assert_eq!(c.frame, 1);
        c.advance(60.0);
        assert_eq!(c.frame, 2);
        c.advance(20.0);
        assert_eq!(c.frame, 0, "wraps to the start");
        assert_eq!(c.duration_ms(), 120.0);

        let mut c = Clock::new(vec![1, 1, 1, 1], Some(2));
        c.advance(80.0);
        assert_eq!(c.frame, 2, "loop offset 2 replays the last two frames");

        let mut c = Clock::new(vec![1, 1], None);
        c.looping = false;
        c.advance(100.0);
        assert_eq!((c.frame, c.playing), (1, false));
    }
}
