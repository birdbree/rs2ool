//! Converters are deterministic: two runs produce byte-identical packs (the
//! same pack names). Runs against the cache in `RS2_CACHE`, skipped without.

use std::path::{Path, PathBuf};
use std::process::Command;

fn run(cache: &Path, out: &Path, args: &[&str]) -> Vec<String> {
    let _ = std::fs::remove_dir_all(out);
    let o = Command::new(env!("CARGO_BIN_EXE_rs2conv"))
        .arg("--cache")
        .arg(cache)
        .args(args)
        .arg("-o")
        .arg(out)
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "rs2conv {args:?}: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    let mut names = Vec::new();
    for kind in std::fs::read_dir(out).unwrap() {
        let kind = kind.unwrap().path();
        if kind.is_dir() {
            for p in std::fs::read_dir(&kind).unwrap() {
                let p = p.unwrap().path();
                if p.extension().is_some_and(|e| e == "pack") {
                    names.push(p.strip_prefix(out).unwrap().display().to_string());
                }
            }
        }
    }
    names.sort();
    names
}

#[test]
fn npc_and_models_runs_are_identical() {
    let Some(cache) = std::env::var_os("RS2_CACHE").map(PathBuf::from) else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let tmp = Path::new(env!("CARGO_TARGET_TMPDIR")).join("rs2conv");
    for args in [
        &["npc", "--id", "1", "--with-anims"][..],
        &["materials"][..],
        &["textures"][..],
    ] {
        let a = run(&cache, &tmp.join("a"), args);
        let b = run(&cache, &tmp.join("b"), args);
        assert!(!a.is_empty());
        assert_eq!(a, b, "{args:?}");
    }
}
