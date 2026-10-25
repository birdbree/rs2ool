//! Captioning candidate sequences with a vision model you run yourself,
//! behind `rs2ool cache caption`. Only there with the `caption` feature.
//!
//! Each candidate sequence of an NPC ([`rs2convert::infer`]) is drawn on
//! the NPC's model as a grid of frames ([`frames`]), sent to a model server
//! with a question ([`client`]), and the reply is kept in a local file
//! ([`store`]): a short description, and one class from [`CLASSES`].
//! Captions are evidence for labelling sequences; they're never written
//! into a pack.
//!
//! Nothing is contacted unless captioning is enabled in the
//! [settings](crate::settings::Caption), and the default endpoint is on
//! this machine.

pub mod client;
pub mod frames;
pub mod store;

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use rs2cache::Records;
use rs2cache::types::record::RecordType;
use rs2convert::creature;
use rs2convert::infer::{SequenceIndex, npc_candidates};

use crate::cache::Outcome;
use crate::cache::select::usage;
use crate::settings::Caption;
use client::Client;
use store::{Entry, Store, lock};

/// The classes a caption picks from, each with the gloss the question
/// gives it.
pub const CLASSES: [(&str, &str); 14] = [
    ("idle", "stands, sits or hovers in place"),
    ("walk", "walks, runs, flies or swims along"),
    ("attack", "strikes, bites, kicks, casts or shoots"),
    ("block", "blocks, flinches or recoils from a hit"),
    ("death", "collapses and lies still"),
    ("fall over", "topples or drops to the ground"),
    ("fall away", "drops down out of sight, such as into a gap"),
    ("descend", "floats or lowers itself down gently"),
    ("jump", "leaps up, over or across"),
    ("land", "lands from a fall or a jump"),
    ("vanish", "shrinks, sinks or fades away, as in a teleport"),
    ("gesture", "waves, bows, dances or makes another gesture"),
    ("work", "mines, chops, fishes, cooks or uses a tool"),
    ("other", "none of these"),
];

/// The built-in question. `{frames}` becomes the number of frames shown
/// and `{classes}` the class list with its glosses.
pub const DEFAULT_PROMPT: &str = "\
The image shows {frames} frames of one animation of a game character, in order from left to right and top to bottom. \
Every frame uses the same camera, and the darker band is the ground where the character starts.

Reply with exactly two lines.
Line 1: what the character does, in under 12 words.
Line 2: the one label from this list that fits best, and nothing else:
{classes}";

/// The question for an image of `frames` frames, from the settings'
/// prompt or the built-in one.
pub fn prompt(c: &Caption, frames: usize) -> String {
    let classes: Vec<String> = CLASSES
        .iter()
        .map(|(name, gloss)| format!("- {name}: {gloss}"))
        .collect();
    c.prompt
        .as_deref()
        .unwrap_or(DEFAULT_PROMPT)
        .replace("{frames}", &frames.to_string())
        .replace("{classes}", &classes.join("\n"))
}

/// A reply split into its description and class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    /// The description: the first line, tidied.
    pub text: String,
    /// The class named on the last line that names one, if any.
    pub class: Option<String>,
}

/// Strips list markers, `Line 1:`-style prefixes, quotes and emphasis.
fn tidy(line: &str) -> &str {
    let mut l = line.trim();
    loop {
        let before = l;
        l = l.trim_start_matches(['-', '*', '#', '•', ' ']);
        for p in ["line 1:", "line 2:", "1.", "2.", "label:", "description:"] {
            if l.len() >= p.len() && l[..p.len()].eq_ignore_ascii_case(p) {
                l = l[p.len()..].trim_start();
            }
        }
        l = l.trim_matches(['"', '\'', '*', '`', '.', ' ']);
        if l == before {
            return l;
        }
    }
}

/// The class `line` names: the whole line if it's a class name, or else
/// the longest class name in it as whole words.
fn class_in(line: &str) -> Option<&'static str> {
    let l = tidy(line).to_lowercase();
    if let Some((name, _)) = CLASSES.iter().find(|(n, _)| *n == l) {
        return Some(name);
    }
    let words: Vec<&str> = l
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let joined = format!(" {} ", words.join(" "));
    CLASSES
        .iter()
        .map(|(n, _)| *n)
        .filter(|n| joined.contains(&format!(" {n} ")))
        .max_by_key(|n| n.len())
}

/// Splits a model's reply into a description and a class.
pub fn parse(reply: &str) -> Reply {
    let lines: Vec<&str> = reply.lines().map(tidy).filter(|l| !l.is_empty()).collect();
    let text = lines.first().copied().unwrap_or_default().to_owned();
    // The class line comes last; a one-line reply may name it in the text.
    let class = lines
        .iter()
        .skip(usize::from(lines.len() > 1))
        .rev()
        .find_map(|l| class_in(l))
        .map(str::to_owned);
    Reply { text, class }
}

/// Which sequences to caption.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// Every candidate of one NPC.
    Npc(u32),
    /// Every candidate sequence of every NPC, each once, on the first NPC
    /// (by id) it's a candidate for.
    All,
}

/// How a run goes.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Draw and print what would be sent, without sending anything.
    pub dry_run: bool,
    /// Caption again what's already captioned.
    pub redo: bool,
    /// Stop after this many images.
    pub limit: Option<usize>,
    /// Also write each image here, as `npc<N>-seq<S>.png`.
    pub images: Option<PathBuf>,
    /// Only these sequences, if given.
    pub seqs: Option<BTreeSet<u32>>,
    /// Set from another thread to stop after the image in hand.
    pub cancel: Option<Arc<AtomicBool>>,
}

/// What a run reports as it goes.
pub enum Event<'a> {
    /// The plan: this many sequences to caption.
    Planned(usize),
    /// A dry run drew an image of this many bytes for the sequence.
    Drawn {
        /// The NPC.
        npc: u32,
        /// The sequence.
        seq: u32,
        /// Width and height.
        size: (u32, u32),
        /// PNG size in bytes.
        bytes: usize,
    },
    /// A caption came back and was stored.
    Captioned(&'a Entry),
    /// A sequence failed; the run goes on.
    Failed {
        /// The NPC.
        npc: u32,
        /// The sequence.
        seq: u32,
        /// Why.
        error: anyhow::Error,
    },
}

/// The sequences to caption, NPC by NPC, before skipping what's done.
fn plan(records: &Records, target: Target) -> Result<Vec<(u32, Vec<u32>)>> {
    let frames = records.cache().frame_index()?;
    let seqs = SequenceIndex::build(records.cache(), records, &frames)?;
    let npcs = match target {
        Target::Npc(id) => vec![id],
        Target::All => records.ids(RecordType::Npc)?,
    };
    let mut claimed = BTreeSet::new();
    let mut out = Vec::new();
    for id in npcs {
        let named = match creature::npc(records, &frames, id, &[]) {
            Ok(c) => c,
            Err(e) if target == Target::All => {
                tracing::debug!("npc {id}: {e}");
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        let list: Vec<u32> = match target {
            // Only the candidates that pose, in the picker's order.
            Target::Npc(_) => npc_candidates(&seqs, records, &frames, id, named)
                .1
                .iter()
                .map(|c| c.seq)
                .collect(),
            // Posing every NPC's candidates would take a while; a candidate
            // that won't pose fails when it's drawn instead.
            Target::All => {
                let names: Vec<u32> = named.sequences.iter().map(|s| s.def.id).collect();
                seqs.candidates(&names, &named.mesh_labels())
                    .iter()
                    .map(|c| c.seq)
                    .filter(|&s| claimed.insert(s))
                    .collect()
            }
        };
        if !list.is_empty() {
            out.push((id, list));
        }
    }
    Ok(out)
}

/// Captions `target`'s candidate sequences with the server `c` names,
/// storing each caption in `store` as it comes. Fails at once if
/// captioning is off (unless it's a dry run) or the client can't be set up;
/// a sequence that fails is reported and the run goes on, ending
/// [`Outcome::Partial`].
///
/// The store is only locked to look up and add captions, never while
/// waiting on the server, so others can read and review it meanwhile.
pub fn run(
    records: &Records,
    c: &Caption,
    store: &Mutex<Store>,
    target: Target,
    opts: &Options,
    mut report: impl FnMut(Event),
) -> Result<Outcome> {
    if !opts.dry_run && !c.enabled {
        return Err(usage(
            "captioning is off: turn it on in the explorer's caption settings, or pass --enable",
        ));
    }
    // A dry run can draw without a model or a reachable server.
    let client = match Client::new(c) {
        Ok(client) => Some(client),
        Err(_) if opts.dry_run => None,
        Err(e) => return Err(e),
    };
    let model = c.model.trim().to_owned();
    let mut todo: Vec<(u32, Vec<u32>)> = plan(records, target)?
        .into_iter()
        .map(|(npc, seqs)| {
            let st = lock(store);
            let done = |seq: u32| match target {
                Target::Npc(_) => st.has(npc, seq, &model),
                Target::All => st.has_seq(seq, &model),
            };
            let wanted = |seq: &u32| opts.seqs.as_ref().is_none_or(|w| w.contains(seq));
            let left: Vec<u32> = seqs
                .into_iter()
                .filter(wanted)
                .filter(|&s| opts.redo || !done(s))
                .collect();
            (npc, left)
        })
        .filter(|(_, s)| !s.is_empty())
        .collect();
    if let Some(limit) = opts.limit {
        let mut left = limit;
        for (_, seqs) in &mut todo {
            seqs.truncate(left);
            left -= seqs.len();
        }
        todo.retain(|(_, s)| !s.is_empty());
    }
    report(Event::Planned(todo.iter().map(|(_, s)| s.len()).sum()));
    if let Some(dir) = &opts.images {
        std::fs::create_dir_all(dir)?;
    }

    let frames_index = records.cache().frame_index()?;
    let textures = frames::textures(records);
    let ask = Ask {
        client: client.as_ref(),
        c,
        textures: &textures,
        n: c.frames.clamp(1, 16) as usize,
        opts,
    };
    let mut failed = false;
    let stopped = || {
        opts.cancel
            .as_ref()
            .is_some_and(|c| c.load(Ordering::Relaxed))
    };
    for (npc, seqs) in todo {
        if stopped() {
            break;
        }
        let extra: Vec<u16> = seqs.iter().filter_map(|&s| u16::try_from(s).ok()).collect();
        let creature = match creature::npc(records, &frames_index, npc, &extra) {
            Ok(cr) => cr,
            Err(e) => {
                for seq in seqs {
                    let error = anyhow::anyhow!("npc {npc} won't load: {e}");
                    report(Event::Failed { npc, seq, error });
                }
                failed = true;
                continue;
            }
        };
        for seq in seqs {
            if stopped() {
                break;
            }
            match ask.one(&creature, npc, seq, &mut report) {
                Ok(Some(entry)) => {
                    {
                        let mut st = lock(store);
                        st.insert(entry.clone());
                        st.save()?;
                    }
                    report(Event::Captioned(&entry));
                }
                Ok(None) => {}
                Err(error) => {
                    failed = true;
                    report(Event::Failed { npc, seq, error });
                }
            }
        }
    }
    Ok(if failed {
        Outcome::Partial
    } else {
        Outcome::Ok
    })
}

/// What drawing and asking about one sequence needs.
struct Ask<'a> {
    /// `None` only on a dry run.
    client: Option<&'a Client>,
    c: &'a Caption,
    textures: &'a rs2classic::Textures,
    /// Frames per image.
    n: usize,
    opts: &'a Options,
}

impl Ask<'_> {
    /// Draws sequence `seq` of `creature` (NPC `npc`) and asks about it.
    /// On a dry run it reports the image instead and returns `None`.
    fn one(
        &self,
        creature: &creature::Creature,
        npc: u32,
        seq: u32,
        report: &mut impl FnMut(Event),
    ) -> Result<Option<Entry>> {
        let s = creature
            .sequences
            .iter()
            .position(|q| q.def.id == seq)
            .ok_or_else(|| anyhow::anyhow!("sequence {seq} won't load"))?;
        let img = frames::sheet(creature, s, self.n, self.textures)
            .ok_or_else(|| anyhow::anyhow!("sequence {seq} won't pose on npc {npc}"))?;
        let png = frames::png(&img);
        if let Some(dir) = &self.opts.images {
            std::fs::write(dir.join(format!("npc{npc}-seq{seq}.png")), &png)?;
        }
        let client = match self.client {
            Some(client) if !self.opts.dry_run => client,
            _ => {
                let size = img.dimensions();
                report(Event::Drawn {
                    npc,
                    seq,
                    size,
                    bytes: png.len(),
                });
                return Ok(None);
            }
        };
        let shown = frames::sample(&creature.delays(s), self.n).len();
        let reply = client.caption(&png, &prompt(self.c, shown))?;
        let Reply { text, class } = parse(&reply);
        Ok(Some(Entry {
            npc,
            seq,
            model: client.model().to_owned(),
            frames: shown as u32,
            text,
            class,
            reply,
            review: None,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_split_into_text_and_class() {
        let r = parse("Falls backwards and lies still.\nfall over");
        assert_eq!(r.text, "Falls backwards and lies still");
        assert_eq!(r.class.as_deref(), Some("fall over"));
        let r = parse("Line 1: The spider shrinks away.\n\nLine 2: **Vanish**\n");
        assert_eq!(r.text, "The spider shrinks away");
        assert_eq!(r.class.as_deref(), Some("vanish"));
        // The longest class named wins: "fall away", not "fall".
        let r = parse("drops into a hole\nI'd say fall away, or maybe death");
        assert_eq!(r.class.as_deref(), Some("fall away"));
        // One line: the class comes from the text, if it names one.
        assert_eq!(parse("a jump").class.as_deref(), Some("jump"));
        // No class, and words that merely contain one don't count.
        let r = parse("swings around\nidleness");
        assert_eq!(r.class, None);
        assert_eq!(parse("").text, "");
    }

    #[test]
    fn the_prompt_fills_its_placeholders() {
        let p = prompt(&Caption::default(), 6);
        assert!(p.contains("shows 6 frames"));
        assert!(p.contains("- fall away: drops down out of sight"));
        assert!(!p.contains('{'));
        let own = Caption {
            prompt: Some("{frames}: {classes}".into()),
            ..Caption::default()
        };
        assert!(prompt(&own, 2).starts_with("2: - idle:"));
    }
}
