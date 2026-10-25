//! A location with a morph table lists its default and child states, each
//! drawing its own model, and the same object converted into an objects
//! pack lists the same states with the same options, drawn from the models
//! pack beside it.

#![cfg(feature = "explorer")]

use std::path::PathBuf;

use contentpack::Kind;
use rs2cache::types::record::{Record, RecordType};
use rs2ool::explorer::renderable::{Renderable, load_states};
use rs2ool::explorer::selection::{Sel, load};
use rs2ool::explorer::sources::{Source, open_cache, open_pack};

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

    // Convert it as `rs2ool pack objects --id` would, with the palette.
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

#[test]
fn varp_values_pick_states_as_the_client_does() {
    use std::sync::Arc;

    use rs2convert::morph::{Variable, pick};
    use rs2ool::explorer::playback::{apply_state, apply_varp};
    use rs2ool::explorer::selection::Loaded;

    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = open_cache(&dir).unwrap();
    let Source::Cache { records, .. } = &cache else {
        unreachable!()
    };
    // The first location whose morph table reads a varbit and has two
    // children that load.
    let (record, states) = records
        .ids(RecordType::Loc)
        .unwrap()
        .into_iter()
        .find_map(|id| {
            let r = records.decode(RecordType::Loc, id).ok()?;
            let Record::Loc(d) = &r else { return None };
            d.morph.as_ref()?.varbit?;
            let s = load_states(&cache, &Sel::Record(RecordType::Loc, id), Some(&r))?;
            let m = s.morph.as_ref()?;
            (m.variable.is_some() && m.slots.iter().flatten().count() >= 2).then_some((r, s))
        })
        .expect("a location with a varbit morph table");
    let Record::Loc(d) = &record else {
        unreachable!()
    };
    let table = d.morph.as_ref().unwrap();
    let states = Arc::new(states);
    let m = states.morph.clone().unwrap();
    // The variable is the varbit the table names, with its bit range.
    let Some(Variable::Varbit { id, varp, lsb, msb }) = m.variable else {
        panic!("a varbit")
    };
    let Record::Varbit(vb) = records.decode(RecordType::Varbit, id.into()).unwrap() else {
        unreachable!()
    };
    assert_eq!(
        (Some(id), varp, lsb, msb),
        (table.varbit, vb.varp, vb.lsb, vb.msb)
    );
    let var = m.variable.unwrap();

    let mut loaded = Loaded::<rs2ool::explorer::selection::Selected>::default();
    loaded.states = Some(states.clone());
    // Every value the varbit can hold, with the varp's other bits set: the
    // state shown is the child the table names, or nothing.
    for value in 0..=var.max().min(64) {
        let contents = var.write(-1, value);
        let shown = apply_varp(&mut loaded, contents);
        assert_eq!(loaded.morph_varp, Some(contents));
        match table.children.get(value as usize).copied().flatten() {
            Some(child) if m.slots[value as usize].is_some() => {
                let i = shown.expect("a state");
                assert_eq!(states.labels[i], format!("loc {child}"));
                assert_eq!(pick(table, value), Some(value as usize));
            }
            _ => {
                assert_eq!(shown, None);
                assert!(loaded.renderable.is_none(), "value {value} draws nothing");
            }
        }
    }
    // Picking a state from the list stops following the varp.
    apply_state(&mut loaded, 0);
    assert_eq!(loaded.morph_varp, None);
}

#[test]
fn morphing_npcs_list_their_children_as_creatures() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = open_cache(&dir).unwrap();
    let Source::Cache { records, .. } = &cache else {
        unreachable!()
    };
    let (id, states) = records
        .ids(RecordType::Npc)
        .unwrap()
        .into_iter()
        .find_map(|id| {
            let r = records.decode(RecordType::Npc, id).ok()?;
            let Record::Npc(d) = &r else { return None };
            d.morph.as_ref()?;
            let s = load_states(&cache, &Sel::Record(RecordType::Npc, id), Some(&r))?;
            s.renderables
                .iter()
                .any(|r| matches!(r.as_deref(), Some(Renderable::Creature { .. })))
                .then_some((id, s))
        })
        .expect("an NPC whose morph children animate");
    assert_eq!(states.labels[0], format!("default (npc {id})"));
    assert!(states.labels[1..].iter().all(|l| l.starts_with("npc ")));
    // The NPC's own state is left for the selection to fill in.
    assert!(states.renderables[0].is_none());
    assert!(states.morph.is_some());
}
