//! `rs2ool cache` end to end, against the cache in `RS2_CACHE` (skipped
//! without one). Outputs go under Cargo's per-target temp directory; the
//! cache is only read.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn cache() -> Option<PathBuf> {
    let dir = std::env::var_os("RS2_CACHE").map(PathBuf::from);
    if dir.is_none() {
        eprintln!("skipping: RS2_CACHE is not set");
    }
    dir
}

fn tmp(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("cache")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn run(cache: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rs2ool"))
        .arg("--cache")
        .arg(cache)
        .arg("cache")
        .args(args)
        .output()
        .expect("run rs2ool cache")
}

fn ok(cache: &Path, args: &[&str]) -> String {
    let out = run(cache, args);
    assert!(
        out.status.success(),
        "rs2ool cache {args:?} exited {:?}\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn json(path: impl AsRef<Path>) -> Value {
    let p = path.as_ref();
    serde_json::from_slice(&std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display())))
        .unwrap()
}

fn manifest_paths(dir: &Path) -> Vec<String> {
    json(dir.join("manifest.json"))["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap().to_owned())
        .collect()
}

fn files_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out
}

#[test]
fn b1_list() {
    let Some(c) = cache() else { return };
    let text = ok(&c, &["list"]);
    for i in 0..5 {
        assert!(
            text.lines().any(|l| l.starts_with(&format!("{i}\t"))),
            "index {i} missing:\n{text}"
        );
    }
    assert!(text.contains("config\t24: ") && text.contains("loc.dat"));
    assert!(text.contains("media") && text.contains("compass.dat"));
    let usage = run(
        &c,
        &[
            "dump", "--index", "1", "--type", "model", "--all", "-o", "x",
        ],
    );
    assert_eq!(usage.status.code(), Some(1), "contradictory selectors");
}

#[test]
fn b2_raw_dumps_are_reproducible() {
    let Some(c) = cache() else { return };
    let (a, b, one) = (tmp("b2a"), tmp("b2b"), tmp("b2one"));
    ok(
        &c,
        &[
            "dump",
            "--index",
            "1",
            "--all",
            "--raw",
            "-o",
            a.to_str().unwrap(),
        ],
    );
    ok(
        &c,
        &[
            "dump",
            "--index",
            "1",
            "--all",
            "--raw",
            "-o",
            b.to_str().unwrap(),
        ],
    );
    let ma = std::fs::read(a.join("manifest.json")).unwrap();
    assert_eq!(
        ma,
        std::fs::read(b.join("manifest.json")).unwrap(),
        "manifests differ between runs"
    );
    let files = json(a.join("manifest.json"))["files"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(files.len(), 14927);
    for f in &files {
        let len = std::fs::metadata(a.join(f["path"].as_str().unwrap()))
            .unwrap()
            .len();
        assert_eq!(len, f["raw_size"].as_u64().unwrap(), "{} size", f["path"]);
    }
    ok(
        &c,
        &[
            "dump",
            "--index",
            "1",
            "--id",
            "0",
            "--raw",
            "-o",
            one.to_str().unwrap(),
        ],
    );
    assert_eq!(
        std::fs::read(one.join("idx1/0.bin")).unwrap(),
        std::fs::read(a.join("idx1/0.bin")).unwrap()
    );
}

#[test]
fn b3_decoded_output() {
    let Some(c) = cache() else { return };
    let d = tmp("b3");
    let o = d.to_str().unwrap();
    ok(&c, &["decode", "npc", "--id", "1", "-o", o]);
    let man = json(d.join("npc/1.json"));
    assert_eq!(man["name"], "Man");
    assert!(!man["models"].as_array().unwrap().is_empty());
    assert!(man["stand_anim"].is_u64() && man["walk_anim"].is_u64());

    ok(&c, &["decode", "obj", "--id", "1038", "-o", o]);
    let hat = json(d.join("obj/1038.json"));
    assert_eq!(hat["name"], "Red partyhat");
    assert!(hat["male_equip1"].is_u64() && hat["female_equip1"].is_u64());

    ok(
        &c,
        &["dump", "--type", "model", "--id", "0", "--meta", "-o", o],
    );
    let model = json(d.join("model/0.json"));
    let meta = json(d.join("model/0.meta.json"));
    assert_eq!(
        meta["vertex_count"].as_u64().unwrap() as usize,
        model["vertices"].as_array().unwrap().len()
    );
    assert_eq!(
        meta["face_count"].as_u64().unwrap() as usize,
        model["faces"].as_array().unwrap().len()
    );
}

#[test]
fn b3_meta_only_writes_no_payloads() {
    let Some(c) = cache() else { return };
    let d = tmp("b3meta");
    for t in [
        "loc", "npc", "obj", "idk", "seq", "spotanim", "flo", "varp", "varbit", "texture",
        "sprite", "font", "midi", "anim",
    ] {
        ok(
            &c,
            &[
                "dump",
                "--type",
                t,
                "--all",
                "--meta-only",
                "-o",
                d.to_str().unwrap(),
            ],
        );
    }
    let files = files_under(&d);
    let payloads: Vec<_> = files
        .iter()
        .filter(|p| {
            let n = p.file_name().unwrap().to_str().unwrap();
            !n.ends_with(".meta.json") && n != "manifest.json"
        })
        .collect();
    assert!(
        payloads.is_empty(),
        "payloads written: {:?}",
        &payloads[..payloads.len().min(5)]
    );
    assert!(files.len() > 30_000);
}

#[test]
fn b3_regions() {
    let Some(c) = cache() else { return };
    let d = tmp("b3map");
    ok(
        &c,
        &[
            "dump",
            "--region",
            "50_50",
            "--decode",
            "-o",
            d.to_str().unwrap(),
        ],
    );
    let r = json(d.join("map/50_50.json"));
    assert_eq!(r["planes"].as_array().unwrap().len(), 4);
    assert!(!r["locations"].as_array().unwrap().is_empty());

    let all = tmp("b3maps");
    ok(&c, &["dump", "--all-regions", "-o", all.to_str().unwrap()]);
    // Two files (tiles and locations) per region; 660 regions = 4620 / 7.
    assert_eq!(manifest_paths(&all).len(), 660 * 2);
}

#[test]
fn b3_images() {
    let Some(c) = cache() else { return };
    let d = tmp("b3img");
    let o = d.to_str().unwrap();
    ok(&c, &["dump", "--type", "sprite", "--all", "-o", o]);
    let pngs = manifest_paths(&d)
        .into_iter()
        .filter(|p| p.ends_with(".png"))
        .count();
    // Every media and title sub-sprite (fonts are their own type).
    assert_eq!(pngs, 643 + 1043 - 4 * 256);
    ok(&c, &["dump", "--type", "font", "--all", "-o", o]);
    for f in ["p11_full", "p12_full", "b12_full", "q8_full"] {
        assert!(d.join(format!("font/{f}.png")).exists());
        assert_eq!(
            json(d.join(format!("font/{f}.metrics.json")))["advance"]
                .as_array()
                .unwrap()
                .len(),
            256
        );
    }
    ok(&c, &["dump", "--type", "texture", "--all", "-o", o]);
    assert_eq!(
        manifest_paths(&d)
            .iter()
            .filter(|p| p.starts_with("texture/"))
            .count(),
        50
    );
}

#[test]
fn b3_interface_tree() {
    let Some(c) = cache() else { return };
    let d = tmp("b3if");
    ok(
        &c,
        &[
            "dump",
            "--type",
            "interface",
            "--all",
            "-o",
            d.to_str().unwrap(),
        ],
    );
    let tree = json(d.join("interface/tree.json"));
    let tab = tree
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == 3213)
        .expect("3213 is a root");
    assert!(
        tab["children"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["id"] == 3214)
    );
}

#[test]
fn b4_npc_group() {
    let Some(c) = cache() else { return };
    let d = tmp("b4");
    ok(&c, &["dump", "--group", "npc:1", "-o", d.to_str().unwrap()]);
    let paths = manifest_paths(&d);
    let unique: BTreeSet<_> = paths.iter().collect();
    assert_eq!(unique.len(), paths.len(), "a dependency is listed twice");
    let man = json(d.join("npc/1.json"));
    for m in man["models"].as_array().unwrap() {
        assert!(
            paths.contains(&format!("model/{m}.json")),
            "model {m} missing"
        );
    }
    // Every frame of the stand and walk sequences is in a dumped pack.
    let mut have = BTreeSet::new();
    for p in paths.iter().filter(|p| p.starts_with("anim/")) {
        for f in json(d.join(p))["frames"].as_array().unwrap() {
            have.insert(f["id"].as_u64().unwrap());
        }
    }
    assert!(!have.is_empty());
    for s in [&man["stand_anim"], &man["walk_anim"]] {
        let seq = json(d.join(format!("seq/{s}.json")));
        for f in seq["frames"].as_array().unwrap() {
            assert!(
                have.contains(&f["frame"].as_u64().unwrap()),
                "seq {s} frame {} not dumped",
                f["frame"]
            );
        }
    }
}

#[test]
fn b5_verify() {
    let Some(c) = cache() else { return };
    let text = ok(&c, &["verify"]);
    assert!(text.contains("no damaged files"), "{text}");

    // A copy with the data file cut 10 KB short of the last sector any file
    // uses. (The supplied cache ends with ~64 KB of sectors no file
    // references, so cutting 10 KB off the end damages nothing.)
    let used_end = {
        let cache = rs2cache::Cache::open(&c).unwrap();
        let mut end = 0usize;
        for index in 0..cache.index_count() {
            for id in 0..cache.file_count(index).unwrap() as u32 {
                if let Ok((data, chain)) = cache.read_raw_with_chain(index, id) {
                    let last = chain.last().unwrap();
                    let tail = data.len() - 512 * (chain.len() - 1);
                    end = end.max(last.sector as usize * 520 + 8 + tail);
                }
            }
        }
        end
    };
    let copy = tmp("b5");
    std::fs::create_dir_all(&copy).unwrap();
    for i in 0..5 {
        let name = format!("main_file_cache.idx{i}");
        std::fs::copy(c.join(&name), copy.join(&name)).unwrap();
    }
    let mut dat = std::fs::read(c.join("main_file_cache.dat")).unwrap();
    dat.truncate(used_end - 10 * 1024);
    std::fs::write(copy.join("main_file_cache.dat"), dat).unwrap();
    let out = run(&copy, &["verify"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(3), "{text}");
    assert!(text.contains("truncated: "), "no ids named:\n{text}");
}
