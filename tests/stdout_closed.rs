//! A command whose reader stops early (`rs2ool … | head`) exits 0 without a
//! panic. Synthetic: it inspects a pack it builds itself, so it runs
//! without a cache.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};

use contentpack::{Codec, Kind, PackWriter};

#[test]
fn closing_stdout_early_is_a_clean_exit() {
    // Enough records that `inspect` writes far more than a pipe buffer holds,
    // so it's still writing when the reader goes away.
    let mut w = PackWriter::new(Kind::Strings, 0).with_codec(Codec::None);
    for id in 0..20_000 {
        w.insert(id, Vec::new());
    }
    let dir = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("stdout_closed");
    let _ = std::fs::remove_dir_all(&dir);
    let pack = w.write_to(&dir).unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_rs2ool"))
        .args(["pack", "inspect"])
        .arg(&pack)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Read one line, like `head -1`, then hang up.
    let mut first = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut first)
        .unwrap();
    assert!(first.starts_with("hash\t"), "{first:?}");

    let out = child.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!stderr.contains("panicked"), "panicked: {stderr}");
    assert!(
        out.status.success(),
        "exit {:?}: {stderr}",
        out.status.code()
    );
}
