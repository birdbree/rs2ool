//! How far frame bases alone narrow the sequences that could animate an
//! NPC (issue #61). A measurement, not a check: it prints a report.
//!
//! ```text
//! RS2_CACHE=… cargo test --test infer_frame_bases -- --ignored --nocapture
//! ```
//!
//! Frame packs each carry their own frame base, so packs are grouped by
//! identical base content: one group is one skeleton family. A sequence
//! belongs to the families of the packs its frames come from. For each NPC
//! that names a stand sequence, the candidates are the sequences in the
//! stand sequence's family; the label filter then keeps those whose moved
//! labels are (nearly all) labels the NPC's models have.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use rs2cache::types::anim::{FramePack, kind};
use rs2cache::types::config::{NpcDef, SeqDef};
use rs2cache::types::record::{Record, RecordType};
use rs2cache::{Cache, Records};

/// Label filters tried, as the share needed to pass: of the sequence's
/// moved labels that the NPC's models have ("moved on model"), and of the
/// models' labels that the sequence moves ("model moved").
const THRESHOLDS: [f64; 3] = [0.5, 0.75, 0.95];

/// A summary of a list of counts: median, 90th percentile and maximum.
fn spread(mut v: Vec<usize>) -> String {
    if v.is_empty() {
        return "none".into();
    }
    v.sort_unstable();
    let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    format!(
        "median {}, p90 {}, max {}",
        at(0.5),
        at(0.9),
        v[v.len() - 1]
    )
}

#[test]
#[ignore = "a measurement; run with --ignored --nocapture"]
fn frame_base_families() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = Cache::open(&dir).unwrap();
    let records = Records::new(&cache);

    // Packs, grouped into families by identical frame base.
    let index = cache.frame_index().unwrap();
    let packs: BTreeMap<u32, FramePack> = index
        .pack_of
        .values()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter_map(|&f| Some((f, cache.frame_pack(f).ok()?)))
        .collect();
    let mut family_of_base: BTreeMap<String, usize> = BTreeMap::new();
    let mut family_of_pack: BTreeMap<u32, usize> = BTreeMap::new();
    for (&f, p) in &packs {
        let key = format!("{:?}", p.base);
        let next = family_of_base.len();
        family_of_pack.insert(f, *family_of_base.entry(key).or_insert(next));
    }

    // Sequences: their families, and the labels their frames move.
    let seqs: BTreeMap<u32, SeqDef> = records
        .ids(RecordType::Seq)
        .unwrap()
        .into_iter()
        .filter_map(|id| match records.decode(RecordType::Seq, id) {
            Ok(Record::Seq(s)) => Some((id, s)),
            _ => None,
        })
        .collect();
    let frames: BTreeMap<u16, (u32, usize)> = packs
        .iter()
        .flat_map(|(&f, p)| {
            p.frames
                .iter()
                .enumerate()
                .map(move |(i, fr)| (fr.id, (f, i)))
        })
        .collect();
    let mut families: BTreeMap<u32, BTreeSet<usize>> = BTreeMap::new();
    let mut moved: BTreeMap<u32, BTreeSet<u8>> = BTreeMap::new();
    let mut unresolved = 0;
    for (&id, s) in &seqs {
        for sf in &s.frames {
            let Some(&(pack, i)) = frames.get(&sf.frame) else {
                unresolved += 1;
                continue;
            };
            families
                .entry(id)
                .or_default()
                .insert(family_of_pack[&pack]);
            let p = &packs[&pack];
            for t in &p.frames[i].transforms {
                let Some(b) = p.base.get(usize::from(t.transform)) else {
                    continue;
                };
                let moves = match b.kind {
                    kind::TRANSLATE | kind::ROTATE => (t.dx, t.dy, t.dz) != (0, 0, 0),
                    kind::SCALE => (t.dx, t.dy, t.dz) != (128, 128, 128),
                    _ => false,
                };
                if moves {
                    moved.entry(id).or_default().extend(&b.labels);
                }
            }
        }
    }
    let family_seqs = |fam: usize| -> BTreeSet<u32> {
        families
            .iter()
            .filter(|(_, f)| f.contains(&fam))
            .map(|(&s, _)| s)
            .collect()
    };

    println!("== frame bases ==");
    println!(
        "{} packs, {} distinct frame bases (families); {} sequences, {} with frames resolved",
        packs.len(),
        family_of_base.len(),
        seqs.len(),
        families.len()
    );
    println!("{unresolved} sequence frames name a frame no pack defines");
    let mixed = families.values().filter(|f| f.len() > 1).count();
    println!("{mixed} sequences mix frame bases");
    let mut sizes: Vec<(usize, usize)> = (0..family_of_base.len())
        .map(|f| (family_seqs(f).len(), f))
        .collect();
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    println!(
        "sequences per family, largest first: {:?}",
        sizes.iter().take(15).map(|s| s.0).collect::<Vec<_>>()
    );
    let buckets = [1, 2, 5, 10, 20, 50, 100, usize::MAX];
    let mut lo = 0;
    for hi in buckets {
        let n = sizes.iter().filter(|s| s.0 > lo && s.0 <= hi).count();
        let label = if hi == usize::MAX {
            format!(">{lo}")
        } else {
            format!("{}–{hi}", lo + 1)
        };
        println!("  families with {label} sequences: {n}");
        lo = hi;
    }

    // Are small families just shorter or longer versions of bigger bases?
    let bases: Vec<&Vec<rs2cache::types::anim::BaseTransform>> = {
        let mut seen = BTreeMap::new();
        for p in packs.values() {
            seen.entry(format!("{:?}", p.base)).or_insert(&p.base);
        }
        seen.into_values().collect()
    };
    let prefix_of_other = bases
        .iter()
        .filter(|a| {
            bases
                .iter()
                .any(|b| b.len() > a.len() && b[..a.len()] == a[..])
        })
        .count();
    println!(
        "{prefix_of_other} of {} bases are a strict prefix of another base",
        bases.len()
    );

    // NPCs: candidates by family, then with the label filters.
    let npcs: Vec<NpcDef> = records
        .ids(RecordType::Npc)
        .unwrap()
        .into_iter()
        .filter_map(|id| match records.decode(RecordType::Npc, id) {
            Ok(Record::Npc(n)) => Some(n),
            _ => None,
        })
        .collect();
    let mut model_labels: BTreeMap<u16, Option<BTreeSet<u8>>> = BTreeMap::new();
    let mut labels_of = |models: &[u16]| -> Option<BTreeSet<u8>> {
        let mut all = BTreeSet::new();
        for &m in models {
            let l = model_labels.entry(m).or_insert_with(|| {
                match records.decode(RecordType::Model, m.into()) {
                    Ok(Record::Model(md)) => {
                        Some(md.vertex_labels.unwrap_or_default().into_iter().collect())
                    }
                    _ => None,
                }
            });
            all.extend(l.as_ref()?);
        }
        Some(all)
    };
    let real = |a: Option<u16>| a.filter(|&a| a != u16::MAX).map(u32::from);
    let mut by_family = Vec::new();
    // Per filter and threshold: candidate counts, walks kept, stands kept.
    let mut filtered: BTreeMap<(&str, usize), (Vec<usize>, usize, usize)> = BTreeMap::new();
    let (mut walk_in_family, mut with_walk, mut considered) = (0, 0, 0);
    // The biggest family, and how NPCs in it and out of it fare.
    let biggest = sizes[0].1;
    let (mut in_biggest, mut small_cands) = (0, Vec::new());
    // Where the walk sequence ranks among the candidates by distance in id
    // from the stand sequence (0 = nearest), in and out of the biggest.
    let (mut rank_big, mut rank_small) = (Vec::new(), Vec::new());
    let mut stand_families: BTreeSet<usize> = BTreeSet::new();
    for n in &npcs {
        let Some(stand) = real(n.stand_anim) else {
            continue;
        };
        let Some(fams) = families.get(&stand) else {
            continue;
        };
        let Some(labels) = labels_of(&n.models) else {
            continue;
        };
        considered += 1;
        stand_families.extend(fams);
        let cands: BTreeSet<u32> = fams.iter().flat_map(|&f| family_seqs(f)).collect();
        let empty = BTreeSet::new();
        let share = |s: &u32, which: &str| {
            let m = moved.get(s).unwrap_or(&empty);
            let both = m.intersection(&labels).count() as f64;
            let whole = if which == "moved on model" {
                m.len()
            } else {
                labels.len()
            };
            if whole == 0 { 1.0 } else { both / whole as f64 }
        };
        by_family.push(cands.len());
        let big = fams.contains(&biggest);
        if big {
            in_biggest += 1;
        } else {
            small_cands.push(cands.len());
        }
        let walk = real(n.walk_anim).filter(|&w| w != stand);
        if let Some(w) = walk {
            with_walk += 1;
            walk_in_family += usize::from(cands.contains(&w));
            let mut by_id: Vec<u32> = cands.iter().copied().filter(|&c| c != stand).collect();
            by_id.sort_by_key(|&c| (c.abs_diff(stand), c));
            if let Some(r) = by_id.iter().position(|&c| c == w) {
                if big {
                    rank_big.push(r)
                } else {
                    rank_small.push(r)
                }
            }
        }
        for which in ["moved on model", "model moved"] {
            for (t, &th) in THRESHOLDS.iter().enumerate() {
                let keep: BTreeSet<u32> = cands
                    .iter()
                    .copied()
                    .filter(|s| share(s, which) >= th)
                    .collect();
                let e = filtered.entry((which, t)).or_default();
                e.0.push(keep.len());
                e.1 += usize::from(walk.is_some_and(|w| keep.contains(&w)));
                e.2 += usize::from(keep.contains(&stand));
            }
        }
    }
    println!("\n== NPCs ==");
    println!(
        "{considered} NPCs with a resolvable stand sequence and models; their stand sequences span {} families",
        stand_families.len()
    );
    println!(
        "candidates sharing the stand sequence's family: {}",
        spread(by_family)
    );
    println!(
        "  walk sequence among them: {walk_in_family} of {with_walk} NPCs with a separate walk"
    );
    println!(
        "{in_biggest} NPCs are in the biggest family ({} sequences); the rest have {}",
        sizes[0].0,
        spread(small_cands)
    );
    let within = |v: &[usize], k: usize| v.iter().filter(|&&r| r < k).count();
    for (name, v) in [
        ("biggest family", &rank_big),
        ("other families", &rank_small),
    ] {
        println!(
            "walk's rank by id distance from stand, {name}: {}; in the nearest 1 / 5 / 20: {} / {} / {} of {}",
            spread(v.clone()),
            within(v, 1),
            within(v, 5),
            within(v, 20),
            v.len()
        );
    }
    for ((which, t), (counts, walks, stands)) in filtered {
        println!(
            "filter {which} ≥ {:.0}%: {}; walk kept {walks} of {with_walk}, stand kept {stands} of {considered}",
            THRESHOLDS[t] * 100.0,
            spread(counts)
        );
    }
}
