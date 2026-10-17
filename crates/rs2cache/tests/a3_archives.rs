//! Phase A3 check: every archive decodes, and the config archive holds the
//! twelve named entries.

mod common;

use rs2cache::archive::{self, ARCHIVES};

#[test]
fn every_archive_decodes() {
    let Some(cache) = common::cache() else { return };
    for &(n, name) in ARCHIVES {
        let a = cache.archive(n).unwrap_or_else(|e| panic!("{name}: {e}"));
        let unknown: Vec<_> = a
            .entries
            .iter()
            .filter(|e| e.name.is_none())
            .map(|e| e.hash)
            .collect();
        eprintln!(
            "{n} {name}: {} entries, whole={}, {} unnamed {unknown:?}",
            a.entries.len(),
            a.whole_compressed,
            unknown.len()
        );
        // Every name the client looks up for this archive is present.
        for want in archive::known_names(n) {
            assert!(a.get(want).is_some(), "{name} lacks {want}");
        }
    }
}

#[test]
fn config_has_the_named_entries() {
    let Some(cache) = common::cache() else { return };
    let config = cache.archive(archive::CONFIG).unwrap();
    for name in [
        "loc.dat",
        "loc.idx",
        "npc.dat",
        "npc.idx",
        "obj.dat",
        "obj.idx",
        "idk.dat",
        "flo.dat",
        "seq.dat",
        "spotanim.dat",
        "varp.dat",
        "varbit.dat",
    ] {
        assert!(
            config.get(name).is_some_and(|d| !d.is_empty()),
            "config lacks {name}"
        );
    }
}
