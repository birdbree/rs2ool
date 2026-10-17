//! `cachedump list`.

use rs2cache::{Records, archive};
use rs2intermediate::record::{Record, RecordType};

use crate::Outcome;
use crate::select::usage;

pub fn run(
    records: &Records,
    index: Option<u8>,
    archive_name: Option<&str>,
    kind: Option<RecordType>,
) -> anyhow::Result<Outcome> {
    let cache = records.cache();
    if let Some(index) = index {
        let entries = cache.entries(index).map_err(|e| usage(e.to_string()))?;
        println!("index {index}: {} ids", entries.len());
        for (id, e) in entries.iter().enumerate() {
            if !e.is_absent() {
                println!("{id}\t{} bytes", e.size);
            }
        }
        return Ok(Outcome::Ok);
    }
    if let Some(name) = archive_name {
        let n =
            archive::archive_by_name(name).ok_or_else(|| usage(format!("no archive {name:?}")))?;
        let a = records.archive(n)?;
        println!(
            "archive {n} {}: {} entries{}",
            archive::archive_name(n).unwrap_or("?"),
            a.entries.len(),
            if a.whole_compressed {
                ", compressed whole"
            } else {
                ""
            }
        );
        for e in &a.entries {
            println!(
                "{}\t{}\t{} bytes ({} stored)",
                e.entry_name(),
                e.hash,
                e.uncompressed_size,
                e.compressed_size
            );
        }
        return Ok(Outcome::Ok);
    }
    if let Some(t) = kind {
        let ids = records.ids(t)?;
        println!("{t}: {} records", ids.len());
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
                RecordType::Map => Some(format!("{}_{}", id >> 8, id & 0xff)),
                _ => None,
            };
            match name {
                Some(n) => println!("{id}\t{n}"),
                None => println!("{id}"),
            }
        }
        return Ok(Outcome::Ok);
    }
    println!("index\tfiles\tpresent\tabsent");
    for i in 0..cache.index_count() {
        let e = cache.entries(i)?;
        let absent = e.iter().filter(|e| e.is_absent()).count();
        println!("{i}\t{}\t{}\t{absent}", e.len(), e.len() - absent);
    }
    println!();
    println!("archive\tname\tentries");
    for &(n, name) in archive::ARCHIVES {
        match records.archive(n) {
            Ok(a) => {
                let names: Vec<String> = a
                    .entries
                    .iter()
                    .map(|e| e.entry_name().to_string())
                    .collect();
                println!("{n}\t{name}\t{}: {}", a.entries.len(), names.join(", "));
            }
            Err(e) => println!("{n}\t{name}\terror: {e}"),
        }
    }
    Ok(Outcome::Ok)
}
