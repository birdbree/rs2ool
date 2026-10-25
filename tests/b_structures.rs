//! `rs2ool cache structures` against the cache in `RS2_CACHE` (skipped
//! without one): it lists a region's structures with their guesses, writes
//! their blueprints as JSON, and with `--plans` draws each one's floor plan
//! and writes its sheet. Files go under Cargo's per-target temp directory.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn cache() -> Option<PathBuf> {
    let dir = std::env::var_os("RS2_CACHE").map(PathBuf::from);
    if dir.is_none() {
        eprintln!("skipping: RS2_CACHE is not set");
    }
    dir
}

fn tmp(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("structures")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn structures(cache: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rs2ool"))
        .arg("--cache")
        .arg(cache)
        .args(["cache", "structures"])
        .args(args)
        .output()
        .expect("run rs2ool cache structures")
}

#[test]
fn a_region_lists_with_its_guesses() {
    let Some(cache) = cache() else { return };
    let out = structures(&cache, &["--region", "50_50"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(text.lines().count(), 8, "{text}");
    assert!(text.lines().any(|l| l.contains(" church ")), "{text}");
    assert!(text.lines().any(|l| l.contains("shop (general)")), "{text}");
}

#[test]
fn plans_and_sheets_come_with_the_blueprints() {
    let Some(cache) = cache() else { return };
    let dir = tmp("plans");
    let out = structures(
        &cache,
        &["--region", "50_50", "-o", dir.to_str().unwrap(), "--plans"],
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(dir.join("structures/50_50.json").exists());
    let plans = dir.join("plans/50_50");
    let mut pngs = 0;
    for entry in std::fs::read_dir(&plans).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "png") {
            pngs += 1;
            let img = image::open(&path).unwrap().to_rgba8();
            assert!(img.width() >= 64 && img.height() >= 64);
            let sheet = std::fs::read_to_string(path.with_extension("md")).unwrap();
            assert!(sheet.starts_with("# Structure "));
            assert!(sheet.contains("## Rooms") && sheet.contains("## Neighbours"));
        }
    }
    assert_eq!(pngs, 8);
    // The church's sheet lists its pews and the neighbours around it.
    let church = std::fs::read_to_string(plans.join("3238_3204_0.md")).unwrap();
    assert!(church.contains("Church pew [worship]"), "{church}");
    assert!(church.contains("Current guess: church"), "{church}");
    // Plans need somewhere to go: a usage error.
    let bad = structures(&cache, &["--region", "50_50", "--plans"]);
    assert_eq!(
        bad.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&bad.stderr)
    );
}
