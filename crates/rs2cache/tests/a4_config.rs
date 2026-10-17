//! Every definition of every config type decodes, unknown opcodes are
//! reported with their definition ids, and a few definitions chosen by
//! their structure are spot-checked.

mod common;

use rs2cache::config::{
    self, ConfigType, flo::Flo, idk::Idk, loc::Loc, npc::Npc, obj::Obj, seq::Seq,
    spotanim::SpotAnim,
};
use rs2cache::io::Cursor;

fn report<T: ConfigType>(defs: &[T::Def], unknown: impl Fn(&T::Def) -> (u32, &[u8])) -> usize {
    let bad: Vec<_> = defs
        .iter()
        .map(&unknown)
        .filter(|(_, u)| !u.is_empty())
        .collect();
    eprintln!(
        "{}: {} definitions, {} with unknown opcodes {:?}",
        T::KIND,
        defs.len(),
        bad.len(),
        &bad[..bad.len().min(10)]
    );
    bad.len()
}

#[test]
fn every_definition_decodes() {
    let Some(cache) = common::cache() else { return };
    let mut unknown = 0;
    unknown += report::<Loc>(&cache.config::<Loc>().unwrap(), |d| {
        (d.id, &d.unknown_opcodes)
    });
    unknown += report::<Npc>(&cache.config::<Npc>().unwrap(), |d| {
        (d.id, &d.unknown_opcodes)
    });
    unknown += report::<Obj>(&cache.config::<Obj>().unwrap(), |d| {
        (d.id, &d.unknown_opcodes)
    });
    unknown += report::<Idk>(&cache.config::<Idk>().unwrap(), |d| {
        (d.id, &d.unknown_opcodes)
    });
    unknown += report::<Seq>(&cache.config::<Seq>().unwrap(), |d| {
        (d.id, &d.unknown_opcodes)
    });
    unknown += report::<SpotAnim>(&cache.config::<SpotAnim>().unwrap(), |d| {
        (d.id, &d.unknown_opcodes)
    });
    unknown += report::<Flo>(&cache.config::<Flo>().unwrap(), |d| {
        (d.id, &d.unknown_opcodes)
    });
    assert_eq!(unknown, 0, "definitions with unknown opcodes");
}

/// The first NPC with at least two models, stand and walk sequences, and a
/// one-tile footprint: a jointed figure that animates in place and moves.
fn rigged_npc(npcs: &[rs2cache::types::config::NpcDef]) -> &rs2cache::types::config::NpcDef {
    npcs.iter()
        .find(|n| {
            n.models.len() >= 2 && n.stand_anim.is_some() && n.walk_anim.is_some() && n.size == 1
        })
        .expect("no NPC with two models, stand and walk sequences, and size 1")
}

#[test]
fn spot_checks() {
    let Some(cache) = common::cache() else { return };
    let archive = cache.archive(config::ARCHIVE).unwrap();
    let objs = config::decode_all::<Obj>(&archive).unwrap();
    let has_name = |n: &Option<String>| n.as_deref().is_some_and(|s| !s.is_empty());

    // A stackable obj with stack-size variants: each variant names another
    // defined obj whose inventory model decodes.
    let stack = objs
        .iter()
        .find(|o| o.stackable && !o.stack_variants.is_empty())
        .expect("no stackable obj with stack-size variants");
    assert!(has_name(&stack.name), "obj {} has no name", stack.id);
    for v in &stack.stack_variants {
        assert!(v.count > 0, "obj {} variant {v:?}", stack.id);
        let other = objs
            .get(usize::from(v.item))
            .unwrap_or_else(|| panic!("obj {} variant names undefined obj {}", stack.id, v.item));
        cache
            .model(other.inventory_model.into())
            .unwrap_or_else(|e| panic!("obj {} model: {e}", other.id));
    }

    // An obj worn by both body types: both equip models decode.
    let worn = objs
        .iter()
        .find(|o| o.male_equip1.is_some() && o.female_equip1.is_some())
        .expect("no obj with male and female equip models");
    assert!(has_name(&worn.name), "obj {} has no name", worn.id);
    for m in [worn.male_equip1, worn.female_equip1].into_iter().flatten() {
        cache
            .model(m.into())
            .unwrap_or_else(|e| panic!("obj {} equip model {m}: {e}", worn.id));
    }

    let npcs = config::decode_all::<Npc>(&archive).unwrap();
    let npc = rigged_npc(&npcs);
    assert!(has_name(&npc.name), "npc {} has no name", npc.id);
    eprintln!(
        "obj {} (stack variants), obj {} (worn), npc {}: models {:?}, stand {:?}, walk {:?}",
        stack.id, worn.id, npc.id, npc.models, npc.stand_anim, npc.walk_anim
    );
    for &m in &npc.models {
        cache
            .model(m.into())
            .unwrap_or_else(|e| panic!("npc {} model {m}: {e}", npc.id));
    }

    // The sequences an NPC stands and walks in have frames.
    let seqs = config::decode_all::<Seq>(&archive).unwrap();
    for anim in [npc.stand_anim, npc.walk_anim].into_iter().flatten() {
        let seq = seqs
            .get(usize::from(anim))
            .unwrap_or_else(|| panic!("npc {} names undefined sequence {anim}", npc.id));
        assert!(!seq.frames.is_empty(), "sequence {anim} has no frames");
    }

    // Every flo colour is 24-bit and every idk body part is a slot 0–13.
    for f in config::decode_all::<Flo>(&archive).unwrap() {
        assert!(f.rgb.is_none_or(|c| c <= 0xff_ffff));
    }
    for k in config::decode_all::<Idk>(&archive).unwrap() {
        assert!(
            k.body_part.is_none_or(|b| b < 14),
            "idk {} part {:?}",
            k.id,
            k.body_part
        );
    }
}

/// Decode every definition with `T::decode` directly and return where each
/// one ended.
fn ends<T: ConfigType>(archive: &rs2cache::Archive) -> (Vec<usize>, Vec<usize>, usize) {
    let dat = archive.get(T::DAT).unwrap();
    let starts = match T::IDX {
        Some(idx) => config::offsets::<T>(archive, idx).unwrap(),
        None => Vec::new(),
    };
    let mut ends = Vec::new();
    let mut c = Cursor::new(dat);
    let count = c.u16().unwrap() as usize;
    for id in 0..count {
        if let Some(&at) = starts.get(id) {
            c.seek(at).unwrap();
        }
        T::decode(id as u32, &mut c).unwrap();
        ends.push(c.pos());
    }
    (starts, ends, dat.len())
}

#[test]
fn every_definition_ends_where_the_next_begins() {
    // A field read at the wrong width shows up as a definition that runs
    // into the next one or stops short of it.
    let Some(cache) = common::cache() else { return };
    let archive = cache.archive(config::ARCHIVE).unwrap();
    fn check<T: ConfigType>(archive: &rs2cache::Archive) {
        let (starts, ends, len) = ends::<T>(archive);
        if starts.is_empty() {
            assert_eq!(
                *ends.last().unwrap(),
                len,
                "{} leaves trailing bytes",
                T::KIND
            );
        } else {
            for (id, end) in ends.iter().enumerate() {
                let next = starts.get(id + 1).copied().unwrap_or(len);
                assert_eq!(
                    *end,
                    next,
                    "{} {id} ends at {end}, next starts at {next}",
                    T::KIND
                );
            }
        }
    }
    check::<Loc>(&archive);
    check::<Npc>(&archive);
    check::<Obj>(&archive);
    check::<Idk>(&archive);
    check::<Seq>(&archive);
    check::<SpotAnim>(&archive);
    check::<Flo>(&archive);
}
