//! Phase A10 check: varp and varbit decode exactly with no unknown opcodes;
//! every MIDI file inflates to MIDI; every model and frame pack carries the
//! version and CRC its versionlist table gives.
//!
//! The supplied cache was modified after its versionlist was built: 46 MIDI
//! files differ from it (41 have no version trailer) and 18 map files
//! were added without versionlist entries. Mismatches in indices 3 and 4 are
//! reported; any in indices 1 and 2 fail.

mod common;

use rs2cache::config::{self, ConfigType, varbit::Varbit, varp::Varp};
use rs2cache::io::Cursor;
use rs2cache::{CacheError, Error, compress, versionlist};

fn exact<T: ConfigType>(archive: &rs2cache::Archive) -> Vec<T::Def> {
    let dat = archive.get(T::DAT).unwrap();
    let mut c = Cursor::new(dat);
    let n = c.u16().unwrap();
    let defs: Vec<_> = (0..u32::from(n))
        .map(|id| T::decode(id, &mut c).unwrap())
        .collect();
    assert_eq!(c.remaining(), 0, "{} leaves trailing bytes", T::KIND);
    defs
}

#[test]
fn varp_and_varbit() {
    let Some(cache) = common::cache() else { return };
    let a = cache.archive(config::ARCHIVE).unwrap();
    let varps = exact::<Varp>(&a);
    let varbits = exact::<Varbit>(&a);
    assert!(varps.iter().all(|d| d.unknown_opcodes.is_empty()));
    assert!(varbits.iter().all(|d| d.unknown_opcodes.is_empty()));
    for b in &varbits {
        assert!(
            b.lsb <= b.msb && b.msb < 32,
            "varbit {} bits {}..{}",
            b.id,
            b.lsb,
            b.msb
        );
        assert!(
            usize::from(b.varp) < varps.len(),
            "varbit {} varp {}",
            b.id,
            b.varp
        );
    }
    eprintln!("{} varps, {} varbits", varps.len(), varbits.len());
}

#[test]
fn midi_inflates() {
    let Some(cache) = common::cache() else { return };
    let mut n = 0;
    for id in 0..cache.file_count(3).unwrap() as u32 {
        match cache.read_raw(3, id) {
            Ok(raw) => {
                assert!(compress::is_gzip(&raw), "midi {id} is not gzip");
                let midi = cache.midi(id).unwrap();
                // Standard MIDI, or MIDI wrapped in a RIFF RMID container.
                let rmid = midi.starts_with(b"RIFF") && midi.get(8..12) == Some(b"RMID");
                assert!(midi.starts_with(b"MThd") || rmid, "midi {id} is not MIDI");
                n += 1;
            }
            Err(CacheError::Absent { .. }) => {}
            Err(e) => panic!("midi {id}: {e}"),
        }
    }
    eprintln!("{n} midi files");
}

#[test]
fn versions_and_crcs_match() {
    let Some(cache) = common::cache() else { return };
    let v = cache.versionlist().unwrap();
    assert_eq!(v.models.versions.len(), cache.file_count(1).unwrap());
    assert_eq!(v.anims.versions.len(), cache.file_count(2).unwrap());
    assert_eq!(v.model_index.len(), v.models.versions.len());
    for index in 1..=4u8 {
        let t = v.table(index).unwrap();
        assert_eq!(t.versions.len(), t.crcs.len());
        let (mut checked, mut modified, mut bad) = (0, 0, Vec::new());
        for id in 0..cache.file_count(index).unwrap() as u32 {
            let raw = match cache.read_raw(index, id) {
                Ok(r) => r,
                Err(CacheError::Absent { .. }) => continue,
                Err(e) => panic!("{}", Error::from(e)),
            };
            let Some((version, crc)) = versionlist::file_version_and_crc(&raw) else {
                continue;
            };
            let i = id as usize;
            let (want_version, want_crc) = (t.versions.get(i).copied(), t.crcs.get(i).copied());
            if want_version != Some(version) || want_crc != Some(crc) {
                if index <= 2 {
                    bad.push(id);
                }
                modified += 1;
            }
            checked += 1;
        }
        eprintln!(
            "index {index}: {checked} files checked, {modified} modified after the versionlist was built"
        );
        assert!(
            bad.is_empty(),
            "index {index}: unexplained mismatches {bad:?}"
        );
    }
}
