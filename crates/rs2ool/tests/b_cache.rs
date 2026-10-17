//! `rs2ool cache` end to end, against the cache in `RS2_CACHE` (skipped
//! without one). Outputs go under Cargo's per-target temp directory; the
//! cache is only read.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rs2cache::types::interface::Content;
use rs2cache::types::record::{Record, RecordType};
use rs2cache::{Cache, Records, archive};
use serde_json::Value;

mod common;

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

/// The listing line of archive `name`, with its entry names.
fn archive_entries<'t>(text: &'t str, name: &str) -> Vec<&'t str> {
    let line = text
        .lines()
        .find(|l| l.split('\t').nth(1) == Some(name))
        .unwrap_or_else(|| panic!("archive {name} missing:\n{text}"));
    let listing = line.split('\t').nth(2).unwrap();
    let (count, names) = listing.split_once(": ").unwrap();
    let names: Vec<&str> = names.split(", ").collect();
    assert_eq!(count.parse::<usize>().unwrap(), names.len(), "{line}");
    names
}

#[test]
fn b1_list() {
    let Some(c) = cache() else { return };
    let text = ok(&c, &["list"]);
    let cache = Cache::open(&c).unwrap();
    for i in 0..5 {
        let files = cache.file_count(i).unwrap();
        assert!(
            text.lines()
                .any(|l| l.starts_with(&format!("{i}\t{files}\t"))),
            "index {i} missing or miscounted:\n{text}"
        );
    }
    assert!(archive_entries(&text, "config").contains(&"loc.dat"));
    // The media archive lists the entry every media sprite comes from.
    let media = archive_entries(&text, "media");
    let records = Records::new(&cache);
    let sprites: Vec<String> = records
        .ids(RecordType::Sprite)
        .unwrap()
        .into_iter()
        .filter_map(|id| records.sprite_source(id).ok())
        .filter(|(n, _)| *n == archive::MEDIA)
        .map(|(_, name)| name.to_string())
        .collect();
    assert!(!sprites.is_empty(), "no media sprites");
    for name in &sprites {
        assert!(media.contains(&name.as_str()), "{name} not listed");
    }
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
    let cache = Cache::open(&c).unwrap();
    let entries = cache.entries(1).unwrap();
    let present = entries.iter().filter(|e| !e.is_absent()).count();
    assert_eq!(files.len(), present, "one file per present id");
    let first = entries
        .iter()
        .position(|e| !e.is_absent())
        .expect("a present file");
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
            &first.to_string(),
            "--raw",
            "-o",
            one.to_str().unwrap(),
        ],
    );
    let file = format!("idx1/{first}.bin");
    assert_eq!(
        std::fs::read(one.join(&file)).unwrap(),
        std::fs::read(a.join(&file)).unwrap()
    );
}

#[test]
fn b3_decoded_output() {
    let Some(c) = cache() else { return };
    let cache = Cache::open(&c).unwrap();
    let records = Records::new(&cache);
    let d = tmp("b3");
    let o = d.to_str().unwrap();

    let npc = common::walking_npc(&records);
    let Ok(Record::Npc(def)) = records.decode(RecordType::Npc, npc) else {
        unreachable!()
    };
    ok(&c, &["decode", "npc", "--id", &npc.to_string(), "-o", o]);
    let n = json(d.join(format!("npc/{npc}.json")));
    assert_eq!(n["name"].as_str(), def.name.as_deref());
    assert!(!n["name"].as_str().unwrap().is_empty());
    let models: Vec<u64> = n["models"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m.as_u64().unwrap())
        .collect();
    assert_eq!(
        models,
        def.models.iter().map(|&m| u64::from(m)).collect::<Vec<_>>()
    );
    assert!(n["stand_anim"].is_u64() && n["walk_anim"].is_u64());

    // The first named item worn by both body types.
    let obj = records
        .ids(RecordType::Obj)
        .unwrap()
        .into_iter()
        .find(|&id| {
            matches!(records.decode(RecordType::Obj, id), Ok(Record::Obj(d))
                if d.name.as_deref().is_some_and(|n| !n.is_empty())
                    && d.male_equip1.is_some()
                    && d.female_equip1.is_some())
        })
        .expect("a named item with male and female equipment models");
    ok(&c, &["decode", "obj", "--id", &obj.to_string(), "-o", o]);
    let item = json(d.join(format!("obj/{obj}.json")));
    assert!(!item["name"].as_str().unwrap().is_empty());
    assert!(item["male_equip1"].is_u64() && item["female_equip1"].is_u64());

    let model = records.ids(RecordType::Model).unwrap()[0];
    ok(
        &c,
        &[
            "dump",
            "--type",
            "model",
            "--id",
            &model.to_string(),
            "--meta",
            "-o",
            o,
        ],
    );
    let m = json(d.join(format!("model/{model}.json")));
    let meta = json(d.join(format!("model/{model}.meta.json")));
    assert_eq!(
        meta["vertex_count"].as_u64().unwrap() as usize,
        m["vertices"].as_array().unwrap().len()
    );
    assert_eq!(
        meta["face_count"].as_u64().unwrap() as usize,
        m["faces"].as_array().unwrap().len()
    );
}

#[test]
fn b3_meta_only_writes_no_payloads() {
    let Some(c) = cache() else { return };
    let d = tmp("b3meta");
    const TYPES: [&str; 14] = [
        "loc", "npc", "obj", "idk", "seq", "spotanim", "flo", "varp", "varbit", "texture",
        "sprite", "font", "midi", "anim",
    ];
    for t in TYPES {
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
    // A sidecar for every record of every type dumped.
    let cache = Cache::open(&c).unwrap();
    let records = Records::new(&cache);
    let want: usize = TYPES
        .iter()
        .map(|t| records.ids(t.parse().unwrap()).unwrap().len())
        .sum();
    let sidecars = files
        .iter()
        .filter(|p| p.to_str().unwrap().ends_with(".meta.json"))
        .count();
    assert_eq!(sidecars, want);
}

#[test]
fn b3_regions() {
    let Some(c) = cache() else { return };
    let cache = Cache::open(&c).unwrap();
    let records = Records::new(&cache);
    // The first region with locations on the ground plane.
    let region = records
        .ids(RecordType::Map)
        .unwrap()
        .into_iter()
        .find_map(|id| match records.decode(RecordType::Map, id) {
            Ok(Record::Map(r)) if r.locations.iter().any(|l| l.plane == 0) => Some(r),
            _ => None,
        })
        .expect("a region with ground-plane locations");
    let name = format!("{}_{}", region.x, region.y);
    let d = tmp("b3map");
    ok(
        &c,
        &[
            "dump",
            "--region",
            &name,
            "--decode",
            "-o",
            d.to_str().unwrap(),
        ],
    );
    let r = json(d.join(format!("map/{name}.json")));
    assert_eq!(r["planes"].as_array().unwrap().len(), 4);
    assert_eq!(
        r["locations"].as_array().unwrap().len(),
        region.locations.len()
    );

    let all = tmp("b3maps");
    ok(&c, &["dump", "--all-regions", "-o", all.to_str().unwrap()]);
    // Two files (tiles and locations) per region in the map index.
    assert_eq!(
        manifest_paths(&all).len(),
        records.map_index().unwrap().len() * 2
    );
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
    let cache = Cache::open(&c).unwrap();
    let records = Records::new(&cache);
    let sub_sprites: usize = records
        .ids(RecordType::Sprite)
        .unwrap()
        .into_iter()
        .map(|id| match records.decode(RecordType::Sprite, id) {
            Ok(Record::Sprite(sheet)) => sheet.sprites.len(),
            other => panic!("sprite {id}: {other:?}"),
        })
        .sum();
    assert_eq!(pngs, sub_sprites);
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
        records.ids(RecordType::Texture).unwrap().len()
    );
}

/// The child ids of a layer component; empty for anything else.
fn layer_children(i: &rs2cache::types::interface::Interface) -> Vec<u64> {
    match &i.content {
        Content::Layer { children, .. } => children.iter().map(|c| u64::from(c.id)).collect(),
        _ => Vec::new(),
    }
}

/// The ids of a tree node's nested children.
fn node_children(node: &Value) -> Vec<u64> {
    node["children"]
        .as_array()
        .map(|a| a.iter().map(|c| c["id"].as_u64().unwrap()).collect())
        .unwrap_or_default()
}

#[test]
fn b3_interface_tree() {
    let Some(c) = cache() else { return };
    let cache = Cache::open(&c).unwrap();
    let records = Records::new(&cache);
    let all: std::collections::BTreeMap<u16, _> = records
        .ids(RecordType::Interface)
        .unwrap()
        .into_iter()
        .filter_map(|id| match records.decode(RecordType::Interface, id) {
            Ok(Record::Interface(i)) => Some((i.id, i)),
            _ => None,
        })
        .collect();
    // The first root layer with a child that is itself a layer with
    // children, so the tree nests at least two levels.
    let (root, inner) = all
        .values()
        .filter(|i| i.parent == Some(i.id))
        .find_map(|i| {
            let inner = layer_children(i).into_iter().find(|&c| {
                all.get(&(c as u16))
                    .is_some_and(|c| !layer_children(c).is_empty())
            })?;
            Some((i, &all[&(inner as u16)]))
        })
        .expect("a root layer with a nested layer");

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
    let node = tree
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == root.id)
        .unwrap_or_else(|| panic!("{} is a root", root.id));
    assert_eq!(node_children(node), layer_children(root));
    let nested = node["children"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == inner.id)
        .unwrap();
    assert_eq!(node_children(nested), layer_children(inner));
}

#[test]
fn b4_npc_group() {
    let Some(c) = cache() else { return };
    let cache = Cache::open(&c).unwrap();
    let npc = common::walking_npc(&Records::new(&cache));
    let d = tmp("b4");
    ok(
        &c,
        &[
            "dump",
            "--group",
            &format!("npc:{npc}"),
            "-o",
            d.to_str().unwrap(),
        ],
    );
    let paths = manifest_paths(&d);
    let unique: BTreeSet<_> = paths.iter().collect();
    assert_eq!(unique.len(), paths.len(), "a dependency is listed twice");
    let def = json(d.join(format!("npc/{npc}.json")));
    for m in def["models"].as_array().unwrap() {
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
    for s in [&def["stand_anim"], &def["walk_anim"]] {
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
    // uses. (A cache can end with sectors no file references, so cutting
    // 10 KB off the end of the file itself might damage nothing.)
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
