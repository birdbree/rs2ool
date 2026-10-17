//! Phase A4 check: every definition of every config type decodes; unknown
//! opcodes are reported with their definition ids; spot checks on well-known
//! records.

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

#[test]
fn spot_checks() {
    let Some(cache) = common::cache() else { return };
    let archive = cache.archive(config::ARCHIVE).unwrap();

    let coins = config::decode_one::<Obj>(&archive, 995).unwrap();
    assert_eq!(coins.name.as_deref(), Some("Coins"));
    assert!(coins.stackable);
    assert!(
        !coins.stack_variants.is_empty(),
        "coins have stack-size models"
    );

    let phat = config::decode_one::<Obj>(&archive, 1038).unwrap();
    assert_eq!(phat.name.as_deref(), Some("Red partyhat"));
    assert!(phat.male_equip1.is_some() && phat.female_equip1.is_some());

    let man = config::decode_one::<Npc>(&archive, 1).unwrap();
    assert_eq!(man.name.as_deref(), Some("Man"));
    assert!(!man.models.is_empty());
    assert!(man.stand_anim.is_some() && man.walk_anim.is_some());
    eprintln!(
        "npc 1: models {:?}, stand {:?}, walk {:?}",
        man.models, man.stand_anim, man.walk_anim
    );

    // Every sequence frame list is non-empty for sequences NPCs stand in.
    let seqs = config::decode_all::<Seq>(&archive).unwrap();
    let stand = &seqs[usize::from(man.stand_anim.unwrap())];
    assert!(!stand.frames.is_empty());

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
