//! Animation playback: a frame clock driven by sequence delays (20 ms per
//! cycle, as in the original client).
//!
//! There's one [`Clock`], shared by both compare panes, so a cache creature
//! and its converted version play the same sequence in step. Cache
//! creatures pose through the original client's deformation
//! (`Creature::posed`), converted ones through their rig and clip
//! (`rs2convert::validate::skinned`). Posing only happens when the frame
//! actually changes, and it writes straight into the meshes the 3D
//! viewport already spawned.

use std::sync::Arc;

use bevy::prelude::*;
use bevy_egui::egui;
use contentpack::records::anim::NO_LOOP;
use rs2convert::infer::{Candidate, Role};

use crate::explorer::classic::ClassicView;
use crate::explorer::renderable::Renderable;
use crate::explorer::selection::{CompareSelected, Loaded};
use crate::explorer::viewport3d::{Overlays, Pose, Shown, ShownModel, VertexColours, posed_parts};

/// Milliseconds per cycle of the original client.
pub const CYCLE_MS: f32 = 20.0;

/// Steps through frames using per-frame delays in cycles.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Clock {
    /// Each frame's length in cycles; 0 counts as 1. Empty means nothing
    /// to play.
    pub delays: Vec<u16>,
    /// Frames from the end to replay when wrapping; `None` restarts at 0.
    /// So does an offset of 0 or one longer than the sequence.
    pub loop_offset: Option<u16>,
    /// The frame showing, an index into `delays`.
    pub frame: usize,
    /// Time spent on the current frame so far, in ms.
    pub elapsed_ms: f32,
    /// Whether it's running.
    pub playing: bool,
    /// Whether to wrap at the end; without it, it stops on the last frame.
    pub looping: bool,
}

impl Clock {
    /// A clock at frame 0, playing and looping.
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

    /// How long the current frame lasts, in ms.
    fn frame_ms(&self) -> f32 {
        f32::from(self.delays.get(self.frame).copied().unwrap_or(1).max(1)) * CYCLE_MS
    }

    /// Advances by `dt_ms`, stepping frames as their delays run out. A big
    /// `dt_ms` can step several frames at once.
    pub fn advance(&mut self, dt_ms: f32) {
        if !self.playing || self.delays.is_empty() {
            return;
        }
        self.elapsed_ms += dt_ms;
        // Carry the leftover into the next frame so timing doesn't drift.
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
                // Wrap: step back `loop_offset` frames from the end, as
                // the original client does, or start over.
                self.frame = match self.loop_offset {
                    Some(o) if o > 0 && usize::from(o) <= n => n - usize::from(o),
                    _ => 0,
                };
            }
        }
    }

    /// Jumps to the start of `frame`, clamped to the last frame.
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
        // Every whole frame before this one, plus time into this one.
        self.delays[..self.frame.min(self.delays.len())]
            .iter()
            .map(|&d| f32::from(d.max(1)) * CYCLE_MS)
            .sum::<f32>()
            + self.elapsed_ms
    }
}

/// The sequence ids a renderable can play, in order. Empty for anything
/// that doesn't animate.
pub fn sequences(r: &Renderable) -> Vec<u32> {
    match r {
        Renderable::Creature { creature, .. } => {
            creature.sequences.iter().map(|s| s.def.id).collect()
        }
        // Clip ids are `rig << 16 | seq`; keep the sequence half.
        Renderable::PackCreature { clips, .. } => clips.iter().map(|(id, _)| id & 0xffff).collect(),
        _ => Vec::new(),
    }
}

/// Sequence `seq`'s frame delays (cycles) and loop offset for a
/// renderable, or `None` if it doesn't play `seq`.
///
/// For a cache creature a sequence delay of 0 falls back to the frame's
/// own delay, and then to 1. For a converted creature the delays are the
/// gaps between the clip's frame start times.
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
            // Each frame ends where the next starts; the last at `duration`.
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

/// A renderable's vertices at one frame, from [`posed`].
pub enum Posed {
    /// A cache creature's vertices, in cache space, scaled.
    Cache(Vec<[i32; 3]>),
    /// A converted creature's skinned vertices, in pack space.
    Pack(Vec<[f32; 3]>),
}

impl Posed {
    /// Borrows it as the mesh builders' [`Pose`].
    pub fn pose(&self) -> Pose<'_> {
        match self {
            Posed::Cache(v) => Pose::Cache(v),
            Posed::Pack(v) => Pose::Pack(v),
        }
    }
}

/// Vertices for a renderable at frame `frame` of sequence `seq` (clamped
/// to its length), if it plays that sequence.
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
            // Clips hold at most 255 frames, so the index fits a u8.
            let t = frame.min(c.frame_start.len().saturating_sub(1)) as u8;
            Some(Posed::Pack(rs2convert::validate::skinned(model, rig, c, t)))
        }
        _ => None,
    }
}

/// What's playing.
#[derive(Resource, Default)]
pub struct Playback {
    /// Sequence ids the main renderable can play.
    pub seqs: Vec<u32>,
    /// The chosen sequence, an index into `seqs`.
    pub current: usize,
    /// The frame clock both panes follow.
    pub clock: Clock,
    // Identity of the main renderable `seqs` was listed for (see `ptr`).
    for_main: usize,
    // What was last posed, so an unchanged frame isn't posed again.
    shown: Option<PoseKey>,
}

/// Everything that decides what [`apply`] would draw. When it's equal to
/// last time, there's nothing to redo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PoseKey {
    main: usize,
    compare: usize,
    seq: u32,
    frame: usize,
    colours: VertexColours,
    main_parts: usize,
    compare_parts: usize,
}

impl Playback {
    /// The chosen sequence id, if there's anything to play.
    pub fn seq(&self) -> Option<u32> {
        self.seqs.get(self.current).copied()
    }

    /// Plays sequence index `i` from its start, keeping the play and loop
    /// settings.
    pub fn choose(&mut self, i: usize, r: &Renderable) {
        self.current = i.min(self.seqs.len().saturating_sub(1));
        let (delays, lo) = self.seq().and_then(|s| timing(r, s)).unwrap_or_default();
        // A fresh (empty) clock means a new selection: start playing and
        // looping. Otherwise keep whatever you'd set.
        let (playing, looping) = (
            self.clock.playing || self.clock.delays.is_empty(),
            self.clock.looping || self.clock.delays.is_empty(),
        );
        self.clock = Clock::new(delays, lo);
        self.clock.playing = playing;
        self.clock.looping = looping;
    }
}

/// A renderable's identity: its `Arc` address, or 0 for none. A new load
/// is always a new `Arc`, so this spots changes without comparing models.
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

/// Runs the clock on real time.
fn tick(time: Res<Time>, mut pb: ResMut<Playback>) {
    // Checked first so a paused clock doesn't mark `Playback` changed.
    if pb.clock.playing && !pb.clock.delays.is_empty() {
        // Cap a long stall (window drag, breakpoint) at a quarter second so
        // it doesn't race through frames.
        pb.clock.advance(time.delta_secs().min(0.25) * 1000.0);
    }
}

/// Poses the shown models at the clock's frame when it moves: both panes
/// from the one clock. The classic renderer gets the main pose, relit as
/// the original client relights an animated model.
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
        colours: overlays.colours,
        main_parts: shown.main.parts.len(),
        compare_parts: shown.compare.parts.len(),
    };
    if pb.shown == Some(key) {
        return;
    }
    pb.shown = Some(key);
    let frame = pb.clock.frame;
    // Poses one pane: rebuilds both lighting variants of each part's mesh
    // in place. Returns the renderable and its pose for the classic view.
    let mut pose_side = |r: Option<&Arc<Renderable>>, side: &ShownModel| {
        let r = r?;
        let p = posed(r, seq, frame)?;
        // `client` is the 317-lit variant, `real` the engine-lit one.
        let (client, real, _) = posed_parts(r, overlays.colours, p.pose());
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
    // Only cache creatures have a classic form; the classic renderer needs
    // the model relit with the posed vertices.
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

/// Shows state `i` of a loaded object, if it has that state.
pub fn apply_state<S>(l: &mut Loaded<S>, i: usize) {
    if let Some(s) = l.states.clone()
        && i < s.renderables.len()
    {
        l.state = i;
        l.renderable = s.renderables[i].clone();
    }
}

/// Sequence `seq`'s inference, if a cache creature plays it as an inferred
/// sequence rather than one its definition names.
pub fn inferred(r: &Renderable, seq: u32) -> Option<&Candidate> {
    match r {
        Renderable::Creature { inferred, .. } => inferred.iter().find(|c| c.seq == seq),
        _ => None,
    }
}

/// The picker's name for a sequence: inferred ones show their score.
fn seq_label(r: &Renderable, seq: u32) -> String {
    let Some(c) = inferred(r, seq) else {
        return format!("seq {seq}");
    };
    let role = c.role.map_or("", |r| r.name());
    if c.shared() {
        // Picked by role, not by score: the score means nothing here.
        format!("seq {seq} · inferred {role}, shared pool")
    } else {
        format!("seq {seq} · inferred {role} {:.2}", c.score)
    }
}

/// An inferred sequence's evidence, for its tooltip.
fn evidence(c: &Candidate) -> String {
    let mut s = format!(
        "Inferred, not named by the NPC: same skeleton as its named sequences, moves {:.0}% of its model's labels, {} ids from the nearest named sequence. {} sequences share that skeleton.",
        c.coverage * 100.0,
        c.distance,
        c.pool
    );
    if c.shared() {
        s.push_str(
            " That's a shared pool, where id distance means little, so it was picked from the whole pool by its role: the most collapsed deaths, then short actions with the most motion.",
        );
    }
    if let Some(r) = c.role {
        s.push_str(match r {
            Role::Idle => " It barely moves the model: idle.",
            Role::Action => " It moves the model and returns to its first pose: an action, such as an attack or a block.",
            Role::Death => " It ends with the model collapsed: probably a death.",
            Role::Other => " It moves the model and ends somewhere else.",
        });
    }
    s
}

/// The playback bar: sequence, play/pause, loop and a frame scrubber.
/// Draws nothing unless the main selection can play something.
pub fn playback_ui(ui: &mut egui::Ui, pb: &mut Playback, main: Option<&Renderable>) {
    let Some(r) = main.filter(|_| !pb.seqs.is_empty()) else {
        return;
    };
    ui.horizontal_wrapped(|ui| {
        let mut chosen = pb.current;
        egui::ComboBox::from_id_salt("sequence")
            .selected_text(pb.seq().map_or(String::new(), |s| seq_label(r, s)))
            .show_ui(ui, |ui| {
                for (i, &s) in pb.seqs.iter().enumerate() {
                    let item = ui.selectable_value(&mut chosen, i, seq_label(r, s));
                    if let Some(c) = inferred(r, s) {
                        item.on_hover_text(evidence(c));
                    }
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
            // Play after a non-looping run stopped at the end starts over.
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

/// Adds [`Playback`] and its systems, which run after the 3D viewport has
/// spawned the meshes they pose.
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
