//! Phase D8 check: converting the Man from the GUI's action yields packs
//! byte-identical to `rs2conv npc --id 1 --with-anims`, and exporting a
//! record writes the same files as `cachedump dump`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use rs2explorer::actions::{ConvertTarget, convert, decoded, dump};
use rs2explorer::sources::{Kind, Source, open_cache};
use rs2intermediate::record::RecordType;

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

/// Run a workspace binary through cargo, as a user would.
fn cli(package: &str, args: &[&str]) {
    let status = Command::new(env!("CARGO"))
        .args(["run", "-q", "-p", package, "--"])
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "{package} {args:?}");
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

    // Convert the Man.
    let (gui, cli_out) = (tmp("convert-gui"), tmp("convert-cli"));
    let done = convert(records, &ConvertTarget::Npc(1), &gui).unwrap();
    cli(
        "rs2conv",
        &[
            "--cache",
            d,
            "npc",
            "--id",
            "1",
            "--with-anims",
            "-o",
            cli_out.to_str().unwrap(),
        ],
    );
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

    // Export a record.
    let (gui, cli_out) = (tmp("dump-gui"), tmp("dump-cli"));
    dump(records, decoded(RecordType::Npc, Some(1), &gui)).unwrap();
    cli(
        "cachedump",
        &[
            "--cache",
            d,
            "dump",
            "--type",
            "npc",
            "--id",
            "1",
            "--decode",
            "--meta",
            "-o",
            cli_out.to_str().unwrap(),
        ],
    );
    let (a, b) = (tree(&gui), tree(&cli_out));
    assert!(a.len() >= 2);
    assert!(a == b, "export differs from cachedump's");
}
