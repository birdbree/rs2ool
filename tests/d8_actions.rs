//! Converting an NPC from the GUI's action yields packs byte-identical to
//! `rs2ool pack npc --id <id> --with-anims --infer`, with a clip for every
//! sequence the explorer offers for the NPC, and exporting a record writes
//! the same files as `rs2ool cache dump`.

#![cfg(feature = "explorer")]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use rs2cache::types::record::RecordType;
use rs2ool::explorer::actions::{ConvertTarget, convert, decoded, dump};
use rs2ool::explorer::playback::sequences;
use rs2ool::explorer::selection::{Sel, load};
use rs2ool::explorer::sources::{Kind, Source, open_cache, open_pack};

mod common;

/// Every file under `dir`, by path relative to it.
fn tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.insert(
                    p.strip_prefix(dir).unwrap().to_path_buf(),
                    std::fs::read(&p).unwrap(),
                );
            }
        }
    }
    out
}

fn tmp(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("d8")
        .join(name);
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// Run the `rs2ool` binary, as a user would.
fn cli(args: &[&str]) {
    let status = Command::new(env!("CARGO_BIN_EXE_rs2ool"))
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "rs2ool {args:?}");
}

/// Run a conversion that may exit 3: every pack written, but some clip
/// over the validation threshold. Inferred sequences picked from a shared
/// pool weren't made for this model, so some re-skin poorly.
fn convert_cli(args: &[&str]) {
    let status = Command::new(env!("CARGO_BIN_EXE_rs2ool"))
        .args(args)
        .status()
        .unwrap();
    assert!(
        matches!(status.code(), Some(0 | 3)),
        "rs2ool {args:?}: {status}"
    );
}

#[test]
fn gui_actions_match_the_clis() {
    let Some(dir) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = open_cache(&dir).unwrap();
    let Source::Cache { records, .. } = &cache else {
        unreachable!()
    };
    let d = dir.to_str().unwrap();
    let npc = common::walking_npc(records);
    let id = npc.to_string();

    // Convert the NPC.
    let (gui, cli_out) = (tmp("convert-gui"), tmp("convert-cli"));
    let done = convert(records, &ConvertTarget::Npc(npc), &gui).unwrap();
    convert_cli(&[
        "--cache",
        d,
        "pack",
        "npc",
        "--id",
        &id,
        "--with-anims",
        "--infer",
        "-o",
        cli_out.to_str().unwrap(),
    ]);
    let (a, b) = (tree(&gui), tree(&cli_out));
    let packs = |t: &BTreeMap<PathBuf, Vec<u8>>| {
        t.iter()
            .filter(|(p, _)| p.extension().is_some_and(|e| e == "pack"))
            .map(|(p, v)| (p.clone(), v.clone()))
            .collect::<BTreeMap<_, _>>()
    };
    assert!(!packs(&a).is_empty());
    assert_eq!(
        packs(&a).keys().collect::<Vec<_>>(),
        packs(&b).keys().collect::<Vec<_>>()
    );
    assert!(packs(&a) == packs(&b), "pack bytes differ from the CLI's");
    assert_eq!(
        a.keys().collect::<Vec<_>>(),
        b.keys().collect::<Vec<_>>(),
        "same files"
    );
    assert!(
        done.opens
            .iter()
            .all(|(k, p)| *k == Kind::Pack && p.exists()),
        "each pack is offered as a source"
    );
    // The models pack is offered first, and every button says what it opens.
    let labels: Vec<String> = done
        .opens
        .iter()
        .map(|(k, p)| rs2ool::explorer::jobs::open_label(*k, p))
        .collect();
    assert_eq!(labels[0], "Open models pack");
    assert!(labels.contains(&"Open anims pack".to_owned()));

    // Every sequence the explorer offers for the NPC, named or inferred,
    // has a clip in the converted packs.
    let (_, cache_side) = load(&cache, &Sel::Record(RecordType::Npc, npc)).unwrap();
    let models = done.opens[0].1.clone();
    let pack = open_pack(&models).unwrap();
    let (_, converted) = load(
        &pack,
        &Sel::PackRecord(rs2convert::pipeline::ids::CREATURE_MODEL | npc),
    )
    .unwrap();
    let (offered, clipped) = (
        sequences(&cache_side.unwrap()),
        sequences(&converted.unwrap()),
    );
    assert!(offered.len() > 2, "npc {npc} offers inferred sequences");
    for s in &offered {
        assert!(clipped.contains(s), "seq {s} has a clip");
    }

    // Export a record.
    let (gui, cli_out) = (tmp("dump-gui"), tmp("dump-cli"));
    dump(records, decoded(RecordType::Npc, Some(npc), &gui)).unwrap();
    cli(&[
        "--cache",
        d,
        "cache",
        "dump",
        "--type",
        "npc",
        "--id",
        &id,
        "--decode",
        "--meta",
        "-o",
        cli_out.to_str().unwrap(),
    ]);
    let (a, b) = (tree(&gui), tree(&cli_out));
    assert!(a.len() >= 2);
    assert!(a == b, "export differs from the CLI's");
}
