//! The bone overlay (**View › Bones**): a creature's rig drawn with
//! gizmos, a dot on each bone's pivot and a line to its parent's, posed at
//! the playback clock's frame so it moves with the mesh.
//!
//! A converted creature's pose comes from the same library code that
//! skins its mesh (`rs2convert::validate::pose`), so the pivots sit exactly
//! where the skinned vertices rotate about. A cache creature shows the rig
//! conversion would infer for it; its animation moves vertices, not bones,
//! so each pivot is the centroid of its bone's vertices in the posed frame
//! (`rs2convert::rig::InferredRig::posed_pivots`). Each bone gets the colour
//! that **Vertex colours › By bone** paints its vertices with. Anything
//! without a rig draws nothing.

use bevy::camera::visibility::RenderLayers;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use contentpack::records::anim::Clip;
use contentpack::records::rig::{NO_PARENT, Rig};

use crate::explorer::classic::ClassicOn;
use crate::explorer::mesh::label_colour;
use crate::explorer::playback::Playback;
use crate::explorer::renderable::Renderable;
use crate::explorer::selection::{CompareSelected, Loaded};
use crate::explorer::viewport3d::{COMPARE_LAYER, Compare, Overlays, ShowSet};

/// Pivot dot radius, in pack units (128 to a tile).
const PIVOT_RADIUS: f32 = 3.0;

/// Bone gizmos in the main (left) pane.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct BoneGizmos;

/// Bone gizmos in compare mode's right pane, on its render layer.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct CompareBoneGizmos;

/// A rig's bones at one pose: pivots in pack space (the explorer's world
/// space), and each bone's parent.
#[derive(Debug, Clone, PartialEq)]
pub struct Skeleton {
    /// Each bone's pivot.
    pub pivots: Vec<Vec3>,
    /// Each bone's parent, `None` for a root.
    pub parents: Vec<Option<usize>>,
}

impl Skeleton {
    /// The bones at frame `t` of `clip`, or at rest without one.
    pub fn new(rig: &Rig, clip: Option<(&Clip, u8)>) -> Self {
        let f = |x: f64, y: f64, z: f64| Vec3::new(x as f32, y as f32, z as f32);
        let pivots = match clip {
            // A bone's world transform takes its own pivot to the
            // translation part.
            Some((c, t)) => rs2convert::validate::pose(rig, c, t)
                .iter()
                .map(|w| f(w.translation.x, w.translation.y, w.translation.z))
                .collect(),
            None => rs2convert::validate::rest_pivots(rig)
                .iter()
                .map(|p| f(p.x, p.y, p.z))
                .collect(),
        };
        let parents = rig
            .bones
            .iter()
            .map(|b| (b.parent != NO_PARENT).then_some(usize::from(b.parent)))
            .collect();
        Self { pivots, parents }
    }

    /// Draws the pivots and parent lines.
    pub fn draw<C: GizmoConfigGroup>(&self, gizmos: &mut Gizmos<C>) {
        for (b, (&p, parent)) in self.pivots.iter().zip(&self.parents).enumerate() {
            // Bone indices fit a u8: a rig has at most 254.
            let [r, g, bl] = label_colour(b as u8);
            let colour = Color::srgb(r, g, bl);
            gizmos.sphere(Isometry3d::from_translation(p), PIVOT_RADIUS, colour);
            if let Some(&q) = parent.and_then(|i| self.pivots.get(i)) {
                gizmos.line(p, q, colour);
            }
        }
    }
}

/// A renderable's skeleton at frame `frame` of sequence `seq`, matching
/// what [`posed`](crate::explorer::playback::posed) draws: the frame is
/// clamped to the sequence, and a sequence it doesn't have (or none) gives
/// the rest pose.
///
/// A converted creature's bones come from its rig and clip. A cache
/// creature's come from the rig inferred from it: its animation moves
/// vertices rather than bones, so each bone's pivot is the centroid of the
/// vertices it stands for in the posed frame. `None` for anything else.
pub fn skeleton(r: &Renderable, seq: Option<u32>, frame: usize) -> Option<Skeleton> {
    if let Renderable::Creature {
        creature,
        rig: Some(rig),
        ..
    } = r
    {
        let posed = seq.and_then(|s| crate::explorer::playback::posed(r, s, frame));
        let vertices = match &posed {
            Some(crate::explorer::playback::Posed::Cache(v)) => v.as_slice(),
            _ => creature.rest.as_slice(),
        };
        let labels = creature.model.vertex_labels.as_deref().unwrap_or_default();
        let pivots = rig
            .posed_pivots(labels, vertices)
            .into_iter()
            .map(|p| Vec3::from(crate::explorer::mesh::world(p)))
            .collect();
        let parents = rig.bones.iter().map(|b| b.parent).collect();
        return Some(Skeleton { pivots, parents });
    }
    let Renderable::PackCreature { rig, clips, .. } = r else {
        return None;
    };
    let clip = seq
        .and_then(|s| clips.iter().find(|(id, _)| id & 0xffff == s))
        .map(|(_, c)| {
            // Clips hold at most 255 frames, so the index fits a u8.
            (c, frame.min(c.frame_start.len().saturating_sub(1)) as u8)
        });
    Some(Skeleton::new(rig, clip))
}

/// What [`draw`] reads.
#[derive(SystemParam)]
struct BoneInputs<'w> {
    overlays: Res<'w, Overlays>,
    classic: ClassicOn<'w>,
    compare: Res<'w, Compare>,
    loaded: Res<'w, Loaded>,
    right: Res<'w, Loaded<CompareSelected>>,
    pb: Res<'w, Playback>,
}

/// Draws both panes' skeletons while the overlay is on and the modern
/// renderer is showing.
fn draw(
    input: BoneInputs,
    mut left_gizmos: Gizmos<BoneGizmos>,
    mut right_gizmos: Gizmos<CompareBoneGizmos>,
) {
    if !input.overlays.bones || input.classic.active() {
        return;
    }
    let (seq, frame) = (input.pb.seq(), input.pb.clock.frame);
    let of = |r: Option<&Renderable>| r.and_then(|r| skeleton(r, seq, frame));
    if let Some(s) = of(input.loaded.renderable.as_deref()) {
        s.draw(&mut left_gizmos);
    }
    if input.compare.enabled
        && let Some(s) = of(input.right.renderable.as_deref())
    {
        s.draw(&mut right_gizmos);
    }
}

/// Adds the bone overlay. Its gizmos draw over the model, and the right
/// pane's go on compare mode's layer so only its camera sees them.
pub fn plugin(app: &mut App) {
    let config = |layer| GizmoConfig {
        depth_bias: -1.0,
        render_layers: RenderLayers::layer(layer),
        ..default()
    };
    app.insert_gizmo_config(BoneGizmos, config(0))
        .insert_gizmo_config(CompareBoneGizmos, config(COMPARE_LAYER))
        .add_systems(Update, draw.after(ShowSet));
}

#[cfg(test)]
mod tests {
    use super::*;
    use contentpack::records::anim::{Key, NO_LOOP};
    use contentpack::records::rig::{Bone, UNBOUND};

    fn rig() -> Rig {
        let bone = |parent, rest_pos| Bone {
            name_hash: 0,
            parent,
            rest_pos,
            rest_rot: [0; 3],
        };
        Rig {
            bones: vec![bone(NO_PARENT, [0, -10, 0]), bone(0, [5, 0, 0])],
            label_bone: [UNBOUND; 256],
        }
    }

    #[test]
    fn rest_pivots_add_up_down_the_chain() {
        let s = Skeleton::new(&rig(), None);
        assert_eq!(
            s.pivots,
            [Vec3::new(0.0, -10.0, 0.0), Vec3::new(5.0, -10.0, 0.0)]
        );
        assert_eq!(s.parents, [None, Some(0)]);
    }

    #[test]
    fn a_keyed_bone_carries_its_children() {
        // Frame 1 moves the root up 4; frame 0 has no keys.
        let clip = Clip {
            rig: 0,
            frame_start: vec![0, 1],
            duration: 2,
            loop_offset: NO_LOOP,
            priority: 0,
            replay_mode: 0,
            keys: vec![Key {
                t: 1,
                bone: 0,
                rot: [0; 3],
                pos: [0, -4, 0],
                scale: [contentpack::records::anim::SCALE_ONE; 3],
            }],
        };
        let rig = rig();
        assert_eq!(
            Skeleton::new(&rig, Some((&clip, 0))),
            Skeleton::new(&rig, None)
        );
        let s = Skeleton::new(&rig, Some((&clip, 1)));
        assert_eq!(
            s.pivots,
            [Vec3::new(0.0, -14.0, 0.0), Vec3::new(5.0, -14.0, 0.0)]
        );
    }
}
