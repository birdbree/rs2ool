//! `rs2ool cache verify`: walk every index and id, check every sector chain,
//! inflate every file, decode every archive, and compare indices 1–4 with
//! the versionlist.

use std::io::Write;

use rayon::prelude::*;
use rs2cache::{Archive, Cache, CacheError, SectorFault, compress, versionlist};

use crate::cache::Outcome;

/// What [`check`] made of one file. Everything but `Present` and `Absent`
/// counts as damage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Present,
    Absent,
    Truncated,
    BadSector,
    InflateFailed,
}

/// Reads one file and sorts it into a [`Status`]. Hands back the stored
/// bytes too, when the chain could be read, for the versionlist check.
fn check(cache: &Cache, index: u8, id: u32) -> (Status, Option<Vec<u8>>) {
    match cache.read_raw(index, id) {
        Err(CacheError::Absent { .. }) => (Status::Absent, None),
        Err(CacheError::BadSector { fault, .. }) => match fault {
            SectorFault::OutOfRange | SectorFault::Short => (Status::Truncated, None),
            _ => (Status::BadSector, None),
        },
        Err(_) => (Status::BadSector, None),
        Ok(raw) => {
            // Index 0 files are archives, so "inflates" means "decodes".
            let ok = if index == 0 {
                Archive::decode(&raw, None).is_ok()
            } else {
                compress::inflate(&raw).is_ok()
            };
            (
                if ok {
                    Status::Present
                } else {
                    Status::InflateFailed
                },
                Some(raw),
            )
        }
    }
}

/// The first 20 ids, space-separated, then a count of the rest.
fn ids(list: &[u32]) -> String {
    let shown: Vec<String> = list.iter().take(20).map(u32::to_string).collect();
    let more = list.len().saturating_sub(20);
    if more > 0 {
        format!("{} … and {more} more", shown.join(" "))
    } else {
        shown.join(" ")
    }
}

/// Runs the check and writes the report to `out`.
///
/// Returns [`Outcome::Partial`] when any file is damaged. Files that only
/// differ from the versionlist aren't damage and don't change the outcome.
pub fn run(cache: &Cache, out: &mut impl Write) -> anyhow::Result<Outcome> {
    let versions = cache.versionlist().ok();
    let mut damaged = 0usize;
    writeln!(
        out,
        "index\tpresent\tabsent\ttruncated\tbad-sector\tinflate-failed\tmodified"
    )?;
    let mut details = Vec::new();
    for index in 0..cache.index_count() {
        let n = cache.file_count(index)? as u32;
        // One `(id, status, modified)` per file, checked in parallel.
        let results: Vec<(u32, Status, bool)> = (0..n)
            .into_par_iter()
            .map(|id| {
                let (status, raw) = check(cache, index, id);
                let table = versions.as_ref().and_then(|v| v.table(index));
                // Modified: the file's own version/CRC trailer disagrees
                // with the versionlist. No table or no trailer: not checked.
                let modified = match (
                    table,
                    raw.as_deref().and_then(versionlist::file_version_and_crc),
                ) {
                    (Some(t), Some((ver, crc))) => {
                        t.versions.get(id as usize) != Some(&ver)
                            || t.crcs.get(id as usize) != Some(&crc)
                    }
                    _ => false,
                };
                (id, status, modified)
            })
            .collect();
        // Ids with status `s`, in id order.
        let of =
            |s: Status| -> Vec<u32> { results.iter().filter(|r| r.1 == s).map(|r| r.0).collect() };
        let (truncated, bad, inflate) = (
            of(Status::Truncated),
            of(Status::BadSector),
            of(Status::InflateFailed),
        );
        let modified: Vec<u32> = results.iter().filter(|r| r.2).map(|r| r.0).collect();
        writeln!(
            out,
            "{index}\t{}\t{}\t{}\t{}\t{}\t{}",
            of(Status::Present).len(),
            of(Status::Absent).len(),
            truncated.len(),
            bad.len(),
            inflate.len(),
            modified.len()
        )?;
        damaged += truncated.len() + bad.len() + inflate.len();
        for (what, list) in [
            ("truncated", &truncated),
            ("bad-sector", &bad),
            ("inflate-failed", &inflate),
        ] {
            if !list.is_empty() {
                details.push(format!("index {index} {what}: {}", ids(list)));
            }
        }
        if !modified.is_empty() {
            details.push(format!(
                "index {index} modified since the versionlist was built (not an error): {}",
                ids(&modified)
            ));
        }
    }
    for d in &details {
        writeln!(out, "{d}")?;
    }
    if versions.is_none() {
        writeln!(out, "versionlist unreadable: modified files not checked")?;
    }
    if damaged > 0 {
        writeln!(out, "{damaged} damaged files")?;
        Ok(Outcome::Partial)
    } else {
        writeln!(out, "no damaged files")?;
        Ok(Outcome::Ok)
    }
}
