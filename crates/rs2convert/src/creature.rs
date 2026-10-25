//! Creature assembly: an NPC's (or kit part's) models merged into one mesh
//! the way the original client merges them, plus the sequences and frame
//! packs that animate it.
//!
//! A [`Creature`] is what rig inference, clip conversion and validation all
//! work from.

use std::collections::{BTreeMap, BTreeSet};

use rs2cache::Records;
use rs2cache::anim::FrameIndex;
use rs2cache::types::anim::{Frame, FramePack};
use rs2cache::types::config::{Recolour, SeqDef};
use rs2cache::types::model::Model;
use rs2cache::types::record::{Record, RecordType};

use crate::ConvertError;

/// Merge models into one, the way the original client does when it builds
/// a creature from parts.
///
/// Faces are appended in order. Each face vertex is welded to the first
/// existing vertex at the same coordinates (and keeps that vertex's label),
/// or appended if there isn't one. Vertices no face uses are dropped.
/// Optional per-face sections are filled with the original client's
/// defaults (type 0, alpha 0, label 0, the model's own priority) for parts
/// that lack them. A single model comes back unchanged.
pub fn merge(models: &[Model]) -> Model {
    if models.len() == 1 {
        return models[0].clone();
    }
    let any_types = models.iter().any(|m| m.face_types.is_some());
    let any_alpha = models.iter().any(|m| m.face_alphas.is_some());
    let any_face_labels = models.iter().any(|m| m.face_labels.is_some());
    let any_vertex_labels = models.iter().any(|m| m.vertex_labels.is_some());
    // Per-face priorities are needed unless every model shares one uniform
    // priority.
    let uniform: BTreeSet<u8> = models
        .iter()
        .filter(|m| m.face_priorities.is_none())
        .map(|m| m.priority)
        .collect();
    let per_face_priority = models.iter().any(|m| m.face_priorities.is_some()) || uniform.len() > 1;

    let mut out = Model {
        vertices: Vec::new(),
        faces: Vec::new(),
        face_colours: Vec::new(),
        face_types: any_types.then(Vec::new),
        priority: uniform.iter().next().copied().unwrap_or(0),
        face_priorities: per_face_priority.then(Vec::new),
        face_alphas: any_alpha.then(Vec::new),
        face_labels: any_face_labels.then(Vec::new),
        vertex_labels: any_vertex_labels.then(Vec::new),
        textured_faces: Vec::new(),
    };
    // Index in `out` of vertex `i` of `m`: the first vertex already at that
    // position, or a new one.
    let weld = |out: &mut Model, m: &Model, i: u16| -> u16 {
        let p = m.vertices[usize::from(i)];
        if let Some(j) = out.vertices.iter().position(|&q| q == p) {
            return j as u16;
        }
        out.vertices.push(p);
        if let Some(l) = &mut out.vertex_labels {
            l.push(m.vertex_labels.as_ref().map_or(0, |ml| ml[usize::from(i)]));
        }
        (out.vertices.len() - 1) as u16
    };
    for m in models {
        let tex_offset = out.textured_faces.len();
        for f in 0..m.faces.len() {
            if let Some(t) = &mut out.face_types {
                let mut k = m.face_types.as_ref().map_or(0, |mt| mt[f]);
                // Textured: shift the triangle index (bits 2+) past the
                // triangles earlier parts already added. Wraps like a byte.
                if k & 2 == 2 {
                    k = k.wrapping_add((tex_offset as u8) << 2);
                }
                t.push(k);
            }
            if let Some(p) = &mut out.face_priorities {
                p.push(m.face_priorities.as_ref().map_or(m.priority, |mp| mp[f]));
            }
            if let Some(a) = &mut out.face_alphas {
                a.push(m.face_alphas.as_ref().map_or(0, |ma| ma[f]));
            }
            if let Some(l) = &mut out.face_labels {
                l.push(m.face_labels.as_ref().map_or(0, |ml| ml[f]));
            }
            out.face_colours.push(m.face_colours[f]);
            let face = m.faces[f].map(|i| weld(&mut out, m, i));
            out.faces.push(face);
        }
        for t in &m.textured_faces {
            let t = t.map(|i| weld(&mut out, m, i));
            out.textured_faces.push(t);
        }
    }
    out
}

/// Replace face colours, one recolour after another, as the original client
/// does. Order matters: a later pair can recolour an earlier pair's output.
pub fn recolour(m: &mut Model, recolours: &[Recolour]) {
    for r in recolours {
        for c in &mut m.face_colours {
            if *c == r.from {
                *c = r.to;
            }
        }
    }
}

/// One sequence with its frames resolved.
#[derive(Debug, Clone)]
pub struct Sequence {
    /// The decoded sequence definition.
    pub def: SeqDef,
    /// The pack holding each frame (index matches `def.frames`).
    pub packs: Vec<u32>,
    /// The pack holding each frame's secondary frame, if it has one and a
    /// pack defines it (index matches `def.frames`).
    pub secondary: Vec<Option<u32>>,
}

impl Sequence {
    /// The frame-base slots this sequence leaves to the movement sequence
    /// when it plays while its creature moves, if it lists any.
    pub fn interleave(&self) -> Option<&[u8]> {
        self.def.interleave_labels.as_deref()
    }
}

/// A creature ready for rig inference: the merged mesh, its sequences and
/// every frame pack they use.
#[derive(Debug, Clone)]
pub struct Creature {
    /// The definition's name, or `npc <id>` when it has none.
    pub name: String,
    /// The merged, recoloured mesh, unscaled. Frames apply to this.
    pub model: Model,
    /// The definition's scale per axis, in 1/128 units (128 = 1.0).
    /// The original client applies it after animating.
    pub scale: [i32; 3],
    /// The rest mesh with the scale baked in: what the converted model,
    /// the rig's pivots and validation's skinning start from.
    pub rest: Vec<[i32; 3]>,
    /// The sequences to convert, in the order they were asked for, with
    /// duplicates dropped.
    pub sequences: Vec<Sequence>,
    /// Every frame pack those sequences use, by pack id.
    pub packs: BTreeMap<u32, FramePack>,
    /// The sequences an action with an interleave list mixes with while
    /// the creature moves: the ids of its walk and turn sequences (not its
    /// stand). Empty when it has none, or [`Creature::new`] built it.
    pub moving: Vec<u16>,
}

impl Creature {
    /// Put a creature together and work out its scaled rest mesh.
    pub fn new(
        name: String,
        model: Model,
        scale: [i32; 3],
        sequences: Vec<Sequence>,
        packs: BTreeMap<u32, FramePack>,
    ) -> Self {
        let mut rest = model.vertices.clone();
        crate::deform::apply_scale(&mut rest, scale);
        Self {
            name,
            model,
            scale,
            rest,
            sequences,
            packs,
            moving: Vec::new(),
        }
    }

    /// The merged model with the scale baked into its vertices.
    pub fn rest_model(&self) -> Model {
        Model {
            vertices: self.rest.clone(),
            ..self.model.clone()
        }
    }

    /// The decoded frame of sequence `s` at index `i`.
    pub fn frame(&self, s: &Sequence, i: usize) -> Option<(&FramePack, &Frame)> {
        let pack = self.packs.get(s.packs.get(i)?)?;
        let id = s.def.frames.get(i)?.frame;
        Some((pack, pack.frames.iter().find(|f| f.id == id)?))
    }

    /// Everything frame `i` of sequence `s` applies, in order: its frame,
    /// then its secondary frame if it has one. A secondary frame that no
    /// pack defines is left out, as the original client skips it.
    ///
    /// Only models drawn in interfaces (chat heads) use secondary frames,
    /// each frame applied in full after the other; no sequence that plays
    /// in the world has them.
    pub fn frames(&self, s: &Sequence, i: usize) -> Option<Vec<(&FramePack, &Frame)>> {
        let mut out = vec![self.frame(s, i)?];
        let second = s.def.frames.get(i).and_then(|f| f.secondary);
        if let (Some(id), Some(Some(p))) = (second, s.secondary.get(i)) {
            let pack = self.packs.get(p)?;
            out.extend(pack.frames.iter().find(|f| f.id == id).map(|f| (pack, f)));
        }
        Some(out)
    }

    /// The vertices at frame `i` of sequence `s`, deformed exactly as the
    /// original client deforms them (secondary frame included), then
    /// scaled. This is validation's reference. `None` when the sequence or
    /// frame isn't there.
    pub fn posed(&self, s: usize, i: usize) -> Option<Vec<[i32; 3]>> {
        self.reference(self.sequences.get(s)?, i)
    }

    /// [`Creature::posed`] for a sequence given by itself.
    pub fn reference(&self, s: &Sequence, i: usize) -> Option<Vec<[i32; 3]>> {
        let groups = self.groups();
        let mut v = self.model.vertices.clone();
        for (pack, frame) in self.frames(s, i)? {
            crate::deform::apply_frame(&mut v, &groups, &pack.base, frame);
        }
        crate::deform::apply_scale(&mut v, self.scale);
        Some(v)
    }

    /// The vertices at frame `i` of action sequence `s` while the creature
    /// moves, at frame `j` of movement sequence `m`: the two mixed through
    /// the action's interleave list (see
    /// [`deform::apply_interleaved`](crate::deform::apply_interleaved)),
    /// then scaled. An action without a list plays alone, as in the
    /// original client. `None` when a sequence or frame isn't there.
    pub fn posed_moving(&self, s: usize, i: usize, m: usize, j: usize) -> Option<Vec<[i32; 3]>> {
        self.reference_moving(self.sequences.get(s)?, i, self.sequences.get(m)?, j)
    }

    /// [`Creature::posed_moving`] for sequences given by themselves.
    pub fn reference_moving(
        &self,
        s: &Sequence,
        i: usize,
        m: &Sequence,
        j: usize,
    ) -> Option<Vec<[i32; 3]>> {
        let Some(slots) = s.interleave() else {
            return self.reference(s, i);
        };
        let (pack, action) = self.frame(s, i)?;
        let (_, moving) = self.frame(m, j)?;
        let mut v = self.model.vertices.clone();
        crate::deform::apply_interleaved(&mut v, &self.groups(), &pack.base, action, moving, slots);
        crate::deform::apply_scale(&mut v, self.scale);
        Some(v)
    }

    /// The face transparency (0 opaque, 255 invisible) at frame `i` of
    /// sequence `s`, as the original client fades it: the model's own,
    /// or 0 where it has none, then every alpha transform the frame (and
    /// its secondary frame) applies. `None` when nothing fades: the
    /// sequence or frame isn't there, the frame has no alpha transforms, or
    /// the model has no face labels, which the original client needs to
    /// fade anything.
    pub fn faded(&self, s: usize, i: usize) -> Option<Vec<u8>> {
        self.reference_alphas(self.sequences.get(s)?, i)
    }

    /// [`Creature::faded`] for a sequence given by itself.
    pub fn reference_alphas(&self, s: &Sequence, i: usize) -> Option<Vec<u8>> {
        let frames = self.frames(s, i)?;
        let faces = self.face_groups()?;
        if !frames.iter().any(|(p, f)| fades(p, f)) {
            return None;
        }
        let mut alphas = self.base_alphas();
        for (pack, frame) in frames {
            crate::deform::apply_alpha(&mut alphas, &faces, &pack.base, frame);
        }
        Some(alphas)
    }

    /// [`Creature::faded`] while the creature moves, mixed as
    /// [`Creature::posed_moving`] mixes the vertices: the action's alpha
    /// transforms outside the interleave list, then the movement's inside
    /// it.
    pub fn faded_moving(&self, s: usize, i: usize, m: usize, j: usize) -> Option<Vec<u8>> {
        self.reference_alphas_moving(self.sequences.get(s)?, i, self.sequences.get(m)?, j)
    }

    /// [`Creature::faded_moving`] for sequences given by themselves.
    pub fn reference_alphas_moving(
        &self,
        s: &Sequence,
        i: usize,
        m: &Sequence,
        j: usize,
    ) -> Option<Vec<u8>> {
        let Some(slots) = s.interleave() else {
            return self.reference_alphas(s, i);
        };
        let (pack, action) = self.frame(s, i)?;
        let (_, moving) = self.frame(m, j)?;
        let faces = self.face_groups()?;
        let parts = [
            crate::deform::interleave_part(&pack.base, action, slots, false),
            crate::deform::interleave_part(&pack.base, moving, slots, true),
        ];
        if !parts.iter().any(|f| fades(pack, f)) {
            return None;
        }
        let mut alphas = self.base_alphas();
        for part in &parts {
            crate::deform::apply_alpha(&mut alphas, &faces, &pack.base, part);
        }
        Some(alphas)
    }

    /// Faces grouped by face label, if the model has face labels.
    pub fn face_groups(&self) -> Option<crate::deform::Groups> {
        self.model
            .face_labels
            .as_deref()
            .map(crate::deform::Groups::new)
    }

    /// The model's face transparency, 0 where it has none.
    pub fn base_alphas(&self) -> Vec<u8> {
        self.model
            .face_alphas
            .clone()
            .unwrap_or_else(|| vec![0; self.model.faces.len()])
    }

    /// The mesh's vertices grouped by label.
    pub fn groups(&self) -> crate::deform::Groups {
        crate::deform::Groups::new(self.model.vertex_labels.as_deref().unwrap_or_default())
    }

    /// How long each frame of sequence `s` shows, in client cycles
    /// (20 ms): the sequence's delay for it, or the frame's own when that's
    /// 0, and at least 1. Empty when the sequence isn't there.
    pub fn delays(&self, s: usize) -> Vec<u32> {
        let Some(seq) = self.sequences.get(s) else {
            return Vec::new();
        };
        (0..seq.def.frames.len())
            .map(|i| match seq.def.frames[i].delay {
                0 => self
                    .frame(seq, i)
                    .map_or(1, |(_, f)| u32::from(f.delay.max(1))),
                d => u32::from(d),
            })
            .collect()
    }

    /// Every label the merged mesh uses.
    pub fn mesh_labels(&self) -> BTreeSet<u8> {
        self.model.vertex_labels.iter().flatten().copied().collect()
    }

    /// Every label the frame bases this creature uses refer to.
    pub fn pack_labels(&self) -> BTreeSet<u8> {
        self.packs
            .values()
            .flat_map(|p| p.base.iter().flat_map(|b| b.labels.iter().copied()))
            .collect()
    }
}

/// Whether `frame` has an alpha transform that changes anything.
fn fades(pack: &FramePack, frame: &Frame) -> bool {
    frame.transforms.iter().any(|t| {
        t.dx != 0
            && pack
                .base
                .get(usize::from(t.transform))
                .is_some_and(|b| b.kind == rs2cache::types::anim::kind::ALPHA)
    })
}

fn model(records: &Records, id: u16) -> Result<Model, ConvertError> {
    match records.decode(RecordType::Model, id.into())? {
        Record::Model(m) => Ok(m),
        _ => unreachable!(),
    }
}

/// Resolve sequences and load their frame packs.
///
/// Repeated ids are skipped. Fails if a sequence names a frame that no
/// frame pack defines.
pub fn sequences(
    records: &Records,
    index: &FrameIndex,
    ids: &[u16],
) -> Result<(Vec<Sequence>, BTreeMap<u32, FramePack>), ConvertError> {
    let mut seqs = Vec::new();
    let mut packs = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for &id in ids {
        if !seen.insert(id) {
            continue;
        }
        let Record::Seq(def) = records.decode(RecordType::Seq, id.into())? else {
            unreachable!()
        };
        let mut pack_ids = Vec::with_capacity(def.frames.len());
        let mut secondary = Vec::with_capacity(def.frames.len());
        for f in &def.frames {
            let second = f.secondary.and_then(|g| index.pack_for(g));
            secondary.push(second);
            if let Some(p) = second
                && let std::collections::btree_map::Entry::Vacant(e) = packs.entry(p)
            {
                e.insert(records.cache().frame_pack(p)?);
            }
            let p = index.pack_for(f.frame).ok_or_else(|| {
                ConvertError::Invalid(format!("sequence {id} frame {} is in no pack", f.frame))
            })?;
            pack_ids.push(p);
            if let std::collections::btree_map::Entry::Vacant(e) = packs.entry(p) {
                e.insert(records.cache().frame_pack(p)?);
            }
        }
        seqs.push(Sequence {
            def,
            packs: pack_ids,
            secondary,
        });
    }
    Ok((seqs, packs))
}

/// Item `obj`'s equipment model for a body type, as the original client
/// builds it for a player wearing it: the item's equip models (up to three)
/// merged, moved by its y offset, and recoloured. `None` when the item has
/// no equip model for that body type.
pub fn equip_model(
    records: &Records,
    obj: u32,
    female: bool,
) -> Result<Option<Model>, ConvertError> {
    let Record::Obj(d) = records.decode(RecordType::Obj, obj)? else {
        unreachable!()
    };
    let (ids, offset) = if female {
        (
            [d.female_equip1, d.female_equip2, d.female_equip3],
            d.female_offset,
        )
    } else {
        ([d.male_equip1, d.male_equip2, d.male_equip3], d.male_offset)
    };
    // As in the original client: nothing without the first model, and the
    // third only counts when there's a second.
    let ids: Vec<u16> = match ids {
        [None, ..] => return Ok(None),
        [Some(a), Some(b), c] => [Some(a), Some(b), c].into_iter().flatten().collect(),
        [Some(a), None, _] => vec![a],
    };
    let models = ids
        .into_iter()
        .map(|m| model(records, m))
        .collect::<Result<Vec<_>, _>>()?;
    let mut merged = merge(&models);
    if offset != 0 {
        for v in &mut merged.vertices {
            v[1] += i32::from(offset);
        }
    }
    recolour(&mut merged, &d.recolours);
    Ok(Some(merged))
}

/// Assemble spot animation `id`: its model recoloured, its scale, and its
/// sequence with the sequence's frame packs. A spot animation with no
/// sequence comes back with none.
pub fn spotanim(records: &Records, index: &FrameIndex, id: u32) -> Result<Creature, ConvertError> {
    let Record::SpotAnim(d) = records.decode(RecordType::SpotAnim, id)? else {
        unreachable!()
    };
    let m = d
        .model
        .ok_or_else(|| ConvertError::Invalid(format!("spotanim {id} has no model")))?;
    let mut m = model(records, m)?;
    recolour(&mut m, &d.recolours);
    let seqs: Vec<u16> = d.sequence.into_iter().collect();
    let (sequences, packs) = sequences(records, index, &seqs)?;
    Ok(Creature::new(
        format!("spotanim {id}"),
        m,
        [
            i32::from(d.scale_xy),
            i32::from(d.scale_z),
            i32::from(d.scale_xy),
        ],
        sequences,
        packs,
    ))
}

/// Assemble NPC `id`: its models merged and recoloured, its stand and walk
/// sequences (plus `extra` sequence ids, since 317 `npc.dat` names no combat
/// animations) and their frame packs. Its walk and turn sequences are the
/// ones it [moves with](Creature::moving).
pub fn npc(
    records: &Records,
    index: &FrameIndex,
    id: u32,
    extra: &[u16],
) -> Result<Creature, ConvertError> {
    let Record::Npc(d) = records.decode(RecordType::Npc, id)? else {
        unreachable!()
    };
    let models = d
        .models
        .iter()
        .map(|&m| model(records, m))
        .collect::<Result<Vec<_>, _>>()?;
    let mut merged = merge(&models);
    recolour(&mut merged, &d.recolours);
    let mut ids: Vec<u16> = [
        d.stand_anim,
        d.walk_anim,
        d.walk_back_anim,
        d.walk_left_anim,
        d.walk_right_anim,
    ]
    .into_iter()
    .flatten()
    .collect();
    // What it plays while it moves: everything but its stand.
    let mut moving: Vec<u16> = ids[..]
        .iter()
        .copied()
        .filter(|&s| Some(s) != d.stand_anim)
        .collect();
    moving.dedup();
    ids.extend_from_slice(extra);
    let (sequences, packs) = sequences(records, index, &ids)?;
    let mut c = Creature::new(
        d.name.unwrap_or_else(|| format!("npc {id}")),
        merged,
        [
            i32::from(d.scale_xz),
            i32::from(d.scale_y),
            i32::from(d.scale_xz),
        ],
        sequences,
        packs,
    );
    c.moving = moving;
    Ok(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri(offset: i32, label: u8, colour: u16) -> Model {
        Model {
            vertices: vec![
                [offset, 0, 0],
                [offset + 10, 0, 0],
                [offset, 10, 0],
                [99, 99, 99],
            ],
            faces: vec![[0, 1, 2]],
            face_colours: vec![colour],
            face_types: None,
            priority: 0,
            face_priorities: None,
            face_alphas: None,
            face_labels: None,
            vertex_labels: Some(vec![label; 4]),
            textured_faces: vec![],
        }
    }

    #[test]
    fn merge_welds_shared_vertices_and_drops_unused() {
        // The second triangle shares the first's (10,0,0) corner.
        let m = merge(&[tri(0, 1, 5), tri(10, 2, 6)]);
        assert_eq!(
            m.vertices.len(),
            5,
            "one corner welded, unused (99,99,99) dropped"
        );
        assert_eq!(m.faces, [[0, 1, 2], [1, 3, 4]]);
        assert_eq!(
            m.vertex_labels.as_deref(),
            Some(&[1, 1, 1, 2, 2][..]),
            "welded vertex keeps the first label"
        );
        assert_eq!(m.face_colours, [5, 6]);
    }

    #[test]
    fn recolour_replaces_in_order() {
        let mut m = tri(0, 0, 5);
        recolour(
            &mut m,
            &[Recolour { from: 5, to: 7 }, Recolour { from: 7, to: 9 }],
        );
        assert_eq!(m.face_colours, [9]);
    }
}
