//! Learning sequence roles from labels: `rs2ool cache classify`.
//!
//! The labels are reviewed captions, plus any from a local labels file,
//! mapped from caption classes onto [`Role`]s ([`role_of`]). Each labelled
//! sequence is posed on its NPC's model and measured
//! ([`Motion::measure`]), and a [`Tree`] is grown on the measures. The
//! report sets the tree against the hand-tuned rules ([`Motion::role`]) on
//! the same sequences, scoring the tree leave-one-out so it's never marked
//! on a sequence it learned from.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use rs2cache::Records;
use rs2convert::classify::{Params, Sample, Tree, features, leave_one_out};
use rs2convert::creature;
use rs2convert::infer::{Motion, Role};

use super::store::Store;

/// The role a caption class (or a labels-file class) amounts to. `None`
/// for a class it doesn't know.
///
/// Gaits, attacks, blocks, gestures, work and jumps all move and come
/// back, so they're actions; falling over and floating down are downs;
/// landing is other, since it starts in the air.
pub fn role_of(class: &str) -> Option<Role> {
    Some(match class {
        "idle" => Role::Idle,
        "walk" | "attack" | "block" | "gesture" | "work" | "jump" | "action" => Role::Action,
        "death" => Role::Death,
        "fall over" | "descend" => Role::Down,
        "fall away" => Role::Drop,
        "vanish" => Role::Vanish,
        "land" | "other" => Role::Other,
        _ => return None,
    })
}

/// Where a label came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Origin {
    /// A caption reviewed in the explorer.
    Review,
    /// The labels file.
    Labels,
    /// An unreviewed caption's own class: a weak label.
    Caption,
}

/// One labelled sequence.
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    /// The NPC it's measured on.
    pub npc: u32,
    /// The sequence.
    pub seq: u32,
    /// Its class.
    pub class: String,
    /// Where the class came from.
    pub from: Origin,
}

/// Reads a labels file: `[{"npc": 1, "seq": 2820, "class": "death"}, …]`.
pub fn read_labels(path: &Path) -> Result<Vec<Label>> {
    #[derive(serde::Deserialize)]
    struct Row {
        npc: u32,
        seq: u32,
        class: String,
    }
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let rows: Vec<Row> =
        serde_json::from_str(&text).with_context(|| format!("reading {}", path.display()))?;
    Ok(rows
        .into_iter()
        .map(|r| Label {
            npc: r.npc,
            seq: r.seq,
            class: r.class,
            from: Origin::Labels,
        })
        .collect())
}

/// Every label, one per NPC and sequence: reviewed captions first, then
/// the labels file, then (if `weak`) unreviewed captions' own classes.
/// Returns them, and how many labels-file entries a review overrode with a
/// different class.
pub fn gather(store: &Store, file: &[Label], weak: bool) -> (Vec<Label>, usize) {
    let mut out: BTreeMap<(u32, u32), Label> = BTreeMap::new();
    for e in store.entries() {
        if let Some(class) = e.reviewed_class() {
            out.entry((e.npc, e.seq)).or_insert(Label {
                npc: e.npc,
                seq: e.seq,
                class: class.to_owned(),
                from: Origin::Review,
            });
        }
    }
    let mut overridden = 0;
    for l in file {
        match out.get(&(l.npc, l.seq)) {
            Some(r) => overridden += usize::from(role_of(&r.class) != role_of(&l.class)),
            None => {
                out.insert((l.npc, l.seq), l.clone());
            }
        }
    }
    if weak {
        for e in store.entries() {
            if let (None, Some(class)) = (&e.review, &e.class) {
                out.entry((e.npc, e.seq)).or_insert(Label {
                    npc: e.npc,
                    seq: e.seq,
                    class: class.clone(),
                    from: Origin::Caption,
                });
            }
        }
    }
    (out.into_values().collect(), overridden)
}

/// One labelled sequence, measured and judged.
#[derive(Debug, Clone, PartialEq)]
pub struct Judged {
    /// The label.
    pub label: Label,
    /// The role the label amounts to.
    pub truth: Role,
    /// The hand-tuned rules' role.
    pub rules: Role,
    /// The tree's role, from a tree grown without this sequence.
    pub tree: Role,
}

/// What [`run`] found.
pub struct Report {
    /// Every labelled sequence that could be measured, checked ones and
    /// weak ones.
    pub judged: Vec<Judged>,
    /// Labels left out, and why: an unknown class, or a sequence that
    /// won't load or pose.
    pub skipped: Vec<(Label, String)>,
    /// The tree grown on every measured label.
    pub tree: Tree,
}

impl Report {
    /// The checked sequences: reviewed or from the labels file.
    pub fn checked(&self) -> impl Iterator<Item = &Judged> {
        self.judged
            .iter()
            .filter(|j| j.label.from != Origin::Caption)
    }

    /// How many checked sequences the rules and the tree get right.
    pub fn scores(&self) -> (usize, usize, usize) {
        self.checked().fold((0, 0, 0), |(n, r, t), j| {
            (
                n + 1,
                r + usize::from(j.rules == j.truth),
                t + usize::from(j.tree == j.truth),
            )
        })
    }
}

/// Measures every label's sequence on its NPC and grows a tree. Weak
/// labels count `weak` each; checked ones count 1.
pub fn run(records: &Records, labels: Vec<Label>, weak: f64, p: Params) -> Result<Report> {
    let frames = records.cache().frame_index()?;
    let mut by_npc: BTreeMap<u32, Vec<Label>> = BTreeMap::new();
    let mut skipped = Vec::new();
    for l in labels {
        if role_of(&l.class).is_none() {
            let why = format!("unknown class {:?}", l.class);
            skipped.push((l, why));
        } else {
            by_npc.entry(l.npc).or_default().push(l);
        }
    }
    let mut measured: Vec<(Label, Motion)> = Vec::new();
    // Measures `label` on creature `c`, or says why it can't.
    let measure = |c: &creature::Creature, l: &Label| {
        c.sequences
            .iter()
            .position(|s| s.def.id == l.seq)
            .and_then(|s| Motion::measure(c, s))
            .ok_or_else(|| "won't pose".to_owned())
    };
    for (npc, ls) in by_npc {
        let seqs: Vec<u16> = ls
            .iter()
            .filter_map(|l| u16::try_from(l.seq).ok())
            .collect();
        // All of an NPC's labels at once; if one sequence won't load, each
        // on its own, so it doesn't take the others with it.
        let all = creature::npc(records, &frames, npc, &seqs);
        for l in ls {
            let m = match &all {
                Ok(c) => measure(c, &l),
                Err(_) => u16::try_from(l.seq)
                    .map_err(|_| "not a sequence id".to_owned())
                    .and_then(|s| {
                        creature::npc(records, &frames, npc, &[s])
                            .map_err(|e| format!("won't load: {e}"))
                    })
                    .and_then(|c| measure(&c, &l)),
            };
            match m {
                Ok(m) => measured.push((l, m)),
                Err(why) => skipped.push((l, why)),
            }
        }
    }
    let samples: Vec<Sample> = measured
        .iter()
        .map(|(l, m)| Sample {
            x: features(m),
            role: role_of(&l.class).expect("checked above"),
            weight: if l.from == Origin::Caption { weak } else { 1.0 },
        })
        .collect();
    let loo = leave_one_out(&samples, p);
    let judged = measured
        .into_iter()
        .zip(samples.iter().zip(loo))
        .map(|((label, m), (s, tree))| Judged {
            label,
            truth: s.role,
            rules: m.role(),
            tree,
        })
        .collect();
    Ok(Report {
        judged,
        skipped,
        tree: Tree::train(&samples, p),
    })
}

#[cfg(test)]
mod tests {
    use super::super::CLASSES;
    use super::super::store::{Entry, Review};
    use super::*;

    #[test]
    fn every_caption_class_has_a_role() {
        for (name, _) in CLASSES {
            assert!(role_of(name).is_some(), "{name}");
        }
        assert_eq!(role_of("action"), Some(Role::Action));
        assert_eq!(role_of("dance"), None);
    }

    #[test]
    fn reviews_win_then_the_file_then_captions() {
        let dir = std::env::temp_dir().join(format!("rs2ool-classify-{}", std::process::id()));
        let mut store = Store::open(&dir, 1).unwrap();
        let entry = |seq, class: &str, review| Entry {
            npc: 1,
            seq,
            model: "m".into(),
            frames: 6,
            text: String::new(),
            class: Some(class.into()),
            reply: String::new(),
            review,
        };
        let right = Some(Review {
            right: true,
            class: None,
        });
        store.insert(entry(10, "death", right));
        store.insert(entry(11, "jump", None));
        store.insert(entry(12, "idle", None));
        let file = |seq, class: &str| Label {
            npc: 1,
            seq,
            class: class.into(),
            from: Origin::Labels,
        };
        let (labels, overridden) = gather(&store, &[file(10, "vanish"), file(11, "land")], false);
        let got: Vec<(u32, &str, Origin)> = labels
            .iter()
            .map(|l| (l.seq, l.class.as_str(), l.from))
            .collect();
        assert_eq!(
            got,
            [(10, "death", Origin::Review), (11, "land", Origin::Labels)]
        );
        assert_eq!(overridden, 1);
        let (labels, _) = gather(&store, &[], true);
        assert_eq!(labels.len(), 3);
        assert_eq!(labels[2].from, Origin::Caption);
    }
}
