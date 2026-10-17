//! Phase D6 object states: a location with a morph table lists its
//! default and child states, each drawing its own model, and the same
//! object converted into an objects pack lists the same states with the
//! same options, drawn from the models pack beside it.

use std::path::PathBuf;

use rs2explorer::renderable::{Renderable, load_states};
use rs2explorer::selection::{Sel, load};
use rs2explorer::sources::{Source, open_cache, open_pack};
use rs2intermediate::record::{Record, RecordType};
use rs2pack::Kind;

#[test]
fn morphing_locations_switch_states_in_cache_and_pack() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = open_cache(&dir).unwrap();
    let Source::Cache { records, .. } = &cache else {
        unreachable!()
    };
    // The first location with options whose morph has at least two states
    // that draw (many morph children are deliberately empty).
    let (id, record, states) = records
        .ids(RecordType::Loc)
        .unwrap()
        .into_iter()
        .find_map(|id| {
            let r = records.decode(RecordType::Loc, id).ok()?;
            let Record::Loc(d) = &r else { return None };
            let children = d.morph.as_ref()?.children.iter().flatten().count();
            if children < 1 || d.actions.iter().all(Option::is_none) {
                return None;
            }
            let s = load_states(&cache, &Sel::Record(RecordType::Loc, id), Some(&r))?;
            let drawn = s.renderables.iter().filter(|r| r.is_some()).count();
            (s.labels.len() == children + 1 && drawn >= 2).then_some((id, r, s))
        })
        .expect("a morphing location");
    let Record::Loc(d) = &record else {
        unreachable!()
    };
    assert_eq!(
        states.options[0],
        d.actions.iter().flatten().cloned().collect::<Vec<_>>()
    );
    // The explorer opens on the location's own model, or on its first
    // state that draws.
    let (_, r) = load(&cache, &Sel::Record(RecordType::Loc, id)).unwrap();
    assert!(r.is_some() || states.renderables.iter().any(Option::is_some));

    // Convert it as `rs2conv objects --id` would, with the palette.
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("d6_states");
    let _ = std::fs::remove_dir_all(&out);
    let png = |_: &std::path::Path, _: u32, _: u32, _: &[u8]| Ok(());
    let objects = rs2convert::run::objects(records, 0, Some(id)).unwrap();
    let written = rs2convert::run::write(&objects, &out, png).unwrap();
    rs2convert::run::write(&rs2convert::run::materials(records, 0).unwrap(), &out, png).unwrap();
    let (_, _, path) = written.iter().find(|(k, ..)| *k == Kind::Objects).unwrap();
    let pack = open_pack(path).unwrap();
    let sel = Sel::PackRecord(id);
    let ps = load_states(&pack, &sel, None).expect("pack object states");
    assert_eq!(ps.labels.len(), states.labels.len(), "same states");
    assert_eq!(ps.options, states.options, "same options per state");
    for (i, (r, cache_r)) in ps.renderables.iter().zip(&states.renderables).enumerate() {
        if cache_r.is_none() {
            continue;
        }
        assert!(
            matches!(
                r.as_deref(),
                Some(Renderable::PackModel {
                    palette: Some(_),
                    ..
                })
            ),
            "state {i} draws its converted model with the palette"
        );
    }
}
