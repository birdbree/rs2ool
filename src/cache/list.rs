//! `rs2ool cache list`: what's in the cache.
//!
//! With no flag you get a table of the five indices (files, present,
//! absent) and then every archive of index 0 with its entry names. One flag
//! narrows it to an index's ids and sizes, an archive's entries, or a
//! record type's ids (with names where the type has them).

use std::io::Write;

use rs2cache::types::record::{Record, RecordType};
use rs2cache::{Records, archive};

use crate::cache::Outcome;
use crate::cache::select::usage;

/// Writes the listing to `out`.
///
/// Give at most one of `index`, `archive_name` (a name or a number) and
/// `kind`; with none, it prints the overview. An index or archive that
/// doesn't exist is a usage error.
pub fn run(
    records: &Records,
    index: Option<u8>,
    archive_name: Option<&str>,
    kind: Option<RecordType>,
    out: &mut impl Write,
) -> anyhow::Result<Outcome> {
    let cache = records.cache();
    if let Some(index) = index {
        let entries = cache.entries(index).map_err(|e| usage(e.to_string()))?;
        writeln!(out, "index {index}: {} ids", entries.len())?;
        // The slot number is the file id; empty slots are skipped.
        for (id, e) in entries.iter().enumerate() {
            if !e.is_absent() {
                writeln!(out, "{id}\t{} bytes", e.size)?;
            }
        }
        return Ok(Outcome::Ok);
    }
    if let Some(name) = archive_name {
        let n =
            archive::archive_by_name(name).ok_or_else(|| usage(format!("no archive {name:?}")))?;
        let a = records.archive(n)?;
        writeln!(
            out,
            "archive {n} {}: {} entries{}",
            archive::archive_name(n).unwrap_or("?"),
            a.entries.len(),
            if a.whole_compressed {
                ", compressed whole"
            } else {
                ""
            }
        )?;
        for e in &a.entries {
            writeln!(
                out,
                "{}\t{}\t{} bytes ({} stored)",
                e.entry_name(),
                e.hash,
                e.uncompressed_size,
                e.compressed_size
            )?;
        }
        return Ok(Outcome::Ok);
    }
    if let Some(t) = kind {
        let ids = records.ids(t)?;
        writeln!(out, "{t}: {} records", ids.len())?;
        for id in ids {
            let name = match t {
                RecordType::Loc | RecordType::Npc | RecordType::Obj => {
                    match records.decode(t, id)? {
                        Record::Loc(d) => d.name,
                        Record::Npc(d) => d.name,
                        Record::Obj(d) => d.name,
                        _ => None,
                    }
                }
                RecordType::Sprite => Some(records.sprite_source(id)?.1.to_string()),
                RecordType::Font => rs2cache::sprite::FONTS
                    .get(id as usize)
                    .map(|s| s.to_string()),
                // A map id packs the region as `rx << 8 | ry`.
                RecordType::Map => Some(format!("{}_{}", id >> 8, id & 0xff)),
                _ => None,
            };
            match name {
                Some(n) => writeln!(out, "{id}\t{n}")?,
                None => writeln!(out, "{id}")?,
            }
        }
        return Ok(Outcome::Ok);
    }
    writeln!(out, "index\tfiles\tpresent\tabsent")?;
    for i in 0..cache.index_count() {
        let e = cache.entries(i)?;
        let absent = e.iter().filter(|e| e.is_absent()).count();
        writeln!(out, "{i}\t{}\t{}\t{absent}", e.len(), e.len() - absent)?;
    }
    writeln!(out)?;
    writeln!(out, "archive\tname\tentries")?;
    for &(n, name) in archive::ARCHIVES {
        match records.archive(n) {
            Ok(a) => {
                let names: Vec<String> = a
                    .entries
                    .iter()
                    .map(|e| e.entry_name().to_string())
                    .collect();
                writeln!(
                    out,
                    "{n}\t{name}\t{}: {}",
                    a.entries.len(),
                    names.join(", ")
                )?;
            }
            Err(e) => writeln!(out, "{n}\t{name}\terror: {e}")?,
        }
    }
    Ok(Outcome::Ok)
}
