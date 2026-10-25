//! `rs2ool::caption` end to end against a stand-in model server on this
//! machine: a dry run draws without contacting anything, captioning that's
//! turned off contacts nothing, and a real run stores each reply and
//! picks up where it left off. Needs `RS2_CACHE`.

#![cfg(feature = "caption")]

mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rs2cache::{Cache, Records};
use rs2ool::cache::Outcome;
use rs2ool::caption::store::Store;
use rs2ool::caption::{self, Event, Options, Target};
use rs2ool::settings::{Backend, Caption};

/// An Ollama stand-in on a free local port that answers every chat request
/// with the same reply; returns its URL and a count of requests.
fn server() -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut len = 0;
            loop {
                let mut h = String::new();
                reader.read_line(&mut h).unwrap();
                if h.trim().is_empty() {
                    break;
                }
                if let Some((k, v)) = h.split_once(':')
                    && k.eq_ignore_ascii_case("content-length")
                {
                    len = v.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; len];
            reader.read_exact(&mut body).unwrap();
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert!(
                !body["messages"][0]["images"][0]
                    .as_str()
                    .unwrap()
                    .is_empty()
            );
            seen.fetch_add(1, Ordering::SeqCst);
            let reply = r#"{"message": {"content": "Collapses and lies still.\nLine 2: death"}}"#;
            let mut stream = stream;
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{reply}",
                reply.len()
            )
            .unwrap();
        }
    });
    (url, count)
}

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
    let mut store = Store::open(&data, 7).unwrap();
    let (url, count) = server();
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
    let out = caption::run(&records, &c, &mut store, Target::Npc(npc), &dry, |e| {
        if let Event::Drawn { size, bytes, .. } = e {
            assert!(size.0 > 0 && bytes > 0);
            drawn += 1;
        }
    })
    .unwrap();
    assert_eq!((out, drawn), (Outcome::Ok, 3));

    // Off: refused before anything's contacted.
    assert!(caption::run(&records, &c, &mut store, Target::Npc(npc), &opts(3), |_| {}).is_err());
    assert_eq!(count.load(Ordering::SeqCst), 0);
    assert_eq!(store.entries().count(), 0);

    // On: three captions, stored as they come.
    c.enabled = true;
    let mut planned = Vec::new();
    let out = caption::run(&records, &c, &mut store, Target::Npc(npc), &opts(3), |e| {
        if let Event::Planned(n) = e {
            planned.push(n);
        }
    })
    .unwrap();
    assert_eq!(out, Outcome::Ok);
    assert_eq!(count.load(Ordering::SeqCst), 3);
    let first: Vec<u32> = store.entries().map(|e| e.seq).collect();
    assert_eq!(first.len(), 3);
    for e in store.entries() {
        assert_eq!((e.npc, e.model.as_str()), (npc, "stand-in"));
        assert_eq!(e.text, "Collapses and lies still");
        assert_eq!(e.class.as_deref(), Some("death"));
        assert!(e.frames >= 1);
    }

    // A second run reads the file back and captions three more, not the
    // same three again.
    let mut again = Store::open(&data, 7).unwrap();
    assert_eq!(again.entries().count(), 3);
    caption::run(&records, &c, &mut again, Target::Npc(npc), &opts(3), |e| {
        if let Event::Planned(n) = e {
            planned.push(n);
        }
    })
    .unwrap();
    assert_eq!(planned, [3, 3]);
    assert_eq!(count.load(Ordering::SeqCst), 6);
    assert_eq!(again.entries().count(), 6);
    assert!(first.iter().all(|&s| again.has(npc, s, "stand-in")));
    let _ = std::fs::remove_dir_all(data);
}
