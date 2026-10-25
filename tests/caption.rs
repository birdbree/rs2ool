//! `rs2ool::caption` end to end against a stand-in model server on this
//! machine: a dry run draws without contacting anything, captioning that's
//! turned off contacts nothing, and a real run stores each reply and
//! picks up where it left off. Needs `RS2_CACHE`.

#![cfg(feature = "caption")]

mod common;
#[path = "common/server.rs"]
mod server;

use std::sync::Mutex;
use std::sync::atomic::Ordering;

use rs2cache::{Cache, Records};
use rs2ool::cache::Outcome;
use rs2ool::caption::store::{Store, lock};
use rs2ool::caption::{self, Event, Options, Target};
use rs2ool::settings::{Backend, Caption};

/// What the stand-in answers every time.
const REPLY: &str = r#"{"message": {"content": "Collapses and lies still.\nLine 2: death"}}"#;

#[test]
fn captions_are_stored_and_resumed() {
    let Some(dir) = std::env::var_os("RS2_CACHE") else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = Cache::open(std::path::Path::new(&dir)).unwrap();
    let records = Records::new(&cache);
    let npc = common::walking_npc(&records);
    let data = std::env::temp_dir().join(format!("rs2ool-caption-test-{}", std::process::id()));
    let store = Mutex::new(Store::open(&data, 7).unwrap());
    let (url, count) = server::ollama(REPLY);
    let mut c = Caption {
        backend: Backend::Ollama,
        endpoint: url,
        model: "stand-in".into(),
        ..Caption::default()
    };
    let opts = |limit| Options {
        limit: Some(limit),
        ..Options::default()
    };

    // A dry run draws without asking, even with captioning off.
    let mut drawn = 0;
    let dry = Options {
        dry_run: true,
        ..opts(3)
    };
    let out = caption::run(&records, &c, &store, Target::Npc(npc), &dry, |e| {
        if let Event::Drawn { size, bytes, .. } = e {
            assert!(size.0 > 0 && bytes > 0);
            drawn += 1;
        }
    })
    .unwrap();
    assert_eq!((out, drawn), (Outcome::Ok, 3));

    // Off: refused before anything's contacted.
    assert!(caption::run(&records, &c, &store, Target::Npc(npc), &opts(3), |_| {}).is_err());
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert_eq!(lock(&store).entries().count(), 0);

    // On: three captions, stored as they come.
    c.enabled = true;
    let mut planned = Vec::new();
    let out = caption::run(&records, &c, &store, Target::Npc(npc), &opts(3), |e| {
        if let Event::Planned(n) = e {
            planned.push(n);
        }
    })
    .unwrap();
    assert_eq!(out, Outcome::Ok);
    assert_eq!(count.load(Ordering::SeqCst), 3);
    let first: Vec<u32> = lock(&store).entries().map(|e| e.seq).collect();
    assert_eq!(first.len(), 3);
    for e in lock(&store).entries() {
        assert_eq!((e.npc, e.model.as_str()), (npc, "stand-in"));
        assert_eq!(e.text, "Collapses and lies still");
        assert_eq!(e.class.as_deref(), Some("death"));
        assert!(e.frames >= 1);
    }

    // A second run reads the file back and captions three more, not the
    // same three again.
    let again = Mutex::new(Store::open(&data, 7).unwrap());
    assert_eq!(lock(&again).entries().count(), 3);
    caption::run(&records, &c, &again, Target::Npc(npc), &opts(3), |e| {
        if let Event::Planned(n) = e {
            planned.push(n);
        }
    })
    .unwrap();
    assert_eq!(planned, [3, 3]);
    assert_eq!(count.load(Ordering::SeqCst), 6);
    assert_eq!(lock(&again).entries().count(), 6);
    assert!(first.iter().all(|&s| lock(&again).has(npc, s, "stand-in")));
    let _ = std::fs::remove_dir_all(data);
}

#[test]
fn labels_are_measured_and_judged() {
    use rs2ool::caption::classify::{self, Label, Origin};

    let Some(dir) = std::env::var_os("RS2_CACHE") else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = Cache::open(std::path::Path::new(&dir)).unwrap();
    let records = Records::new(&cache);
    let npc = common::walking_npc(&records);
    let rs2cache::types::record::Record::Npc(d) = records
        .decode(rs2cache::types::record::RecordType::Npc, npc)
        .unwrap()
    else {
        unreachable!()
    };
    let (stand, walk) = (d.stand_anim.unwrap(), d.walk_anim.unwrap());
    let label = |seq: u16, class: &str| Label {
        npc,
        seq: seq.into(),
        class: class.into(),
        from: Origin::Labels,
    };
    let labels = vec![
        label(stand, "idle"),
        label(walk, "walk"),
        label(stand, "dance"),
        // Not a sequence id at all.
        label(u16::MAX - 1, "death"),
    ];
    let report = classify::run(&records, labels, 0.0, Default::default()).unwrap();
    assert_eq!(report.judged.len(), 2);
    assert_eq!(report.skipped.len(), 2);
    assert!(
        report
            .skipped
            .iter()
            .any(|(_, why)| why.contains("unknown class"))
    );
    let (n, rules, _) = report.scores();
    assert_eq!(n, 2);
    // A walk moves and comes back: the rules call it an action.
    assert!(rules >= 1);
}

#[test]
fn descriptions_go_into_the_anims_pack() {
    use rs2ool::caption::describe::describe;
    use rs2ool::caption::store::Imported;

    let Some(dir) = std::env::var_os("RS2_CACHE") else {
        eprintln!("skipping: RS2_CACHE is not set");
        return;
    };
    let cache = Cache::open(std::path::Path::new(&dir)).unwrap();
    let records = Records::new(&cache);
    let npc = common::walking_npc(&records);
    let rs2cache::types::record::Record::Npc(d) = records
        .decode(rs2cache::types::record::RecordType::Npc, npc)
        .unwrap()
    else {
        unreachable!()
    };
    let stand = u32::from(d.stand_anim.unwrap());
    let convert = || rs2convert::run::npc(&records, 0, npc, true, &[], None).unwrap();
    let anims = |o: &rs2convert::run::Outcome| {
        let p = o
            .packs
            .iter()
            .find(|p| p.kind() == contentpack::Kind::Anims)
            .unwrap();
        contentpack::PackReader::from_bytes(&p.bake().1).unwrap()
    };
    let data = std::env::temp_dir().join(format!("rs2ool-describe-test-{}", std::process::id()));

    // No captions: no table, and the same pack as without describing.
    let empty = Store::open(&data, 3).unwrap();
    let mut plain = convert();
    assert_eq!(describe(&mut plain, &empty, Some(npc)), 0);
    assert!(anims(&plain).manifest.extra.is_empty());
    assert_eq!(plain.packs[2].bake().0, convert().packs[2].bake().0);

    // A caption for the stand sequence lands on its clip.
    let mut store = Store::open(&data, 3).unwrap();
    store.import(
        "labeller",
        vec![Imported {
            npc,
            seq: stand,
            class: Some("idle".into()),
            text: "stands about".into(),
            frames: 9,
        }],
    );
    let mut described = convert();
    assert_eq!(describe(&mut described, &store, Some(npc)), 1);
    let extra = &anims(&described).manifest.extra["descriptions"];
    let (id, e) = extra.as_object().unwrap().iter().next().unwrap();
    assert_eq!(id.parse::<u32>().unwrap() & 0xffff, stand);
    assert_eq!(e["class"], "idle");
    assert_eq!(e["role"], "idle");
    assert_eq!(e["text"], "stands about");
    assert_eq!(e["by"], "labeller");
    assert_eq!(e["reviewed"], false);
    let _ = std::fs::remove_dir_all(data);
}
