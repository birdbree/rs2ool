//! `rs2ool cache structures`: finds whole structures in the map with
//! [`rs2convert::structure`] and lists them, or writes them as JSON, one
//! file per region.
//!
//! A structure belongs to the region its origin (south-west corner) is in.
//! With `--region`, that region and its eight neighbours are searched, so
//! a structure that crosses into a neighbour comes out whole; one that
//! goes on past them is marked clipped. With `--all`, the whole map is
//! searched at once.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use rs2cache::types::config::FloDef;
use rs2cache::types::config::LocDef;
use rs2cache::types::map::Region;
use rs2cache::types::record::{Record, RecordType};
use rs2cache::{Cache, Records};
use rs2convert::material;
use rs2convert::purpose::{self, Class, Purpose};
use rs2convert::structure::{self, REGION_SIZE, Structure};
use serde::Serialize;

use crate::cache::{Outcome, select};

/// What to search and where the JSON goes.
#[derive(Debug, Clone, Default)]
pub struct StructuresArgs {
    /// One region, `(rx, ry)`.
    pub region: Option<(u8, u8)>,
    /// The whole map.
    pub all: bool,
    /// A directory to write `structures/<rx>_<ry>.json` into; without it,
    /// a summary goes to stdout.
    pub out: Option<PathBuf>,
}

/// One region's file.
#[derive(Serialize)]
struct RegionFile<'a> {
    /// `RX_RY`.
    region: String,
    /// The structures whose origin is in it.
    structures: Vec<Entry<'a>>,
}

/// A structure's blueprint and what it's likely for, side by side.
#[derive(Serialize)]
struct Entry<'a> {
    #[serde(flatten)]
    structure: &'a Structure,
    purpose: Purpose,
}

/// Runs the command.
pub fn run(
    cache: &Cache,
    records: &Records,
    a: &StructuresArgs,
    out: &mut impl Write,
) -> Result<Outcome> {
    let index = cache.map_index()?;
    let wanted: BTreeSet<(u8, u8)> = match (a.region, a.all) {
        (Some(r), false) => {
            let (x, y) = (i32::from(r.0), i32::from(r.1));
            (-1..=1)
                .flat_map(|dx| (-1..=1).map(move |dy| (x + dx, y + dy)))
                .filter_map(|(x, y)| Some((u8::try_from(x).ok()?, u8::try_from(y).ok()?)))
                .collect()
        }
        (None, true) => index.iter().map(|e| (e.x(), e.y())).collect(),
        _ => return Err(select::usage("structures needs --region RX_RY or --all")),
    };
    if let Some((x, y)) = a.region
        && !index.iter().any(|e| (e.x(), e.y()) == (x, y))
    {
        return Err(select::usage(format!("there's no region {x}_{y}")));
    }
    let mut regions = Vec::new();
    let mut failed = 0;
    for e in index.iter().filter(|e| wanted.contains(&(e.x(), e.y()))) {
        match cache.region_for(e) {
            Ok(r) => regions.push(r),
            Err(err) => {
                eprintln!("warning: {err}");
                failed += 1;
            }
        }
    }
    let defs = definitions(records, &regions);
    let mut found = structure::find(&regions, &defs);
    material::dress(&mut found, records, &defs, &floors(records));
    // Each structure under the region its origin is in.
    let mut by_region: BTreeMap<(u8, u8), Vec<&Structure>> = BTreeMap::new();
    for s in &found {
        let r = region_of(s);
        if a.region.is_none_or(|w| w == r) {
            by_region.entry(r).or_default().push(s);
        }
    }
    match &a.out {
        Some(dir) => {
            let dir = dir.join("structures");
            std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
            for ((x, y), structures) in &by_region {
                let path = dir.join(format!("{x}_{y}.json"));
                let file = RegionFile {
                    region: format!("{x}_{y}"),
                    structures: structures
                        .iter()
                        .map(|s| Entry {
                            structure: s,
                            purpose: purpose::of(s),
                        })
                        .collect(),
                };
                let json = serde_json::to_string_pretty(&file)?;
                std::fs::write(&path, json + "\n")
                    .with_context(|| format!("writing {}", path.display()))?;
            }
            let n: usize = by_region.values().map(Vec::len).sum();
            writeln!(
                out,
                "{n} structures in {} regions, written to {}",
                by_region.len(),
                dir.display()
            )?;
        }
        None => {
            for ((x, y), structures) in &by_region {
                for s in structures {
                    writeln!(
                        out,
                        "{x}_{y}  {:<16}  {}",
                        class(&purpose::of(s)),
                        summary(s)
                    )?;
                }
            }
        }
    }
    Ok(if failed > 0 {
        Outcome::Partial
    } else {
        Outcome::Ok
    })
}

/// The definition of every location placed in `regions` that decodes.
fn definitions(records: &Records, regions: &[Region]) -> BTreeMap<u16, LocDef> {
    let ids: BTreeSet<u16> = regions
        .iter()
        .flat_map(|r| r.locations.iter().map(|l| l.id))
        .collect();
    ids.into_iter()
        .filter_map(|id| match records.decode(RecordType::Loc, id.into()) {
            Ok(Record::Loc(d)) => Some((id, d)),
            _ => None,
        })
        .collect()
}

/// Every floor definition that decodes, by id.
fn floors(records: &Records) -> BTreeMap<u16, FloDef> {
    let ids = records.ids(RecordType::Flo).unwrap_or_default();
    ids.into_iter()
        .filter_map(|id| match records.decode(RecordType::Flo, id) {
            Ok(Record::Flo(d)) => Some((u16::try_from(id).ok()?, d)),
            _ => None,
        })
        .collect()
}

/// The region a structure's origin is in.
fn region_of(s: &Structure) -> (u8, u8) {
    let r = |v: u16| u8::try_from(v / REGION_SIZE).unwrap_or(u8::MAX);
    (r(s.origin.0), r(s.origin.1))
}

/// The structure's guessed class, with what a shop sells, or `?` when
/// nothing scored enough.
fn class(p: &Purpose) -> String {
    let name = match p.class.what {
        Class::Unknown => "?".to_string(),
        c => serde_json::to_value(c)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default(),
    };
    match &p.class.detail {
        Some(d) => format!("{name} ({d})"),
        None => name,
    }
}

/// One line about a structure: where it is, its size, its rooms (and how
/// many are yards) and ways in, and its named objects, the commonest
/// first.
fn summary(s: &Structure) -> String {
    let planes = match (s.planes.first(), s.planes.last()) {
        (Some(a), Some(b)) if a != b => format!("planes {a}–{b}"),
        (Some(a), _) => format!("plane {a}"),
        _ => String::new(),
    };
    let entrances = s
        .doors
        .iter()
        .filter(|d| !d.open && d.rooms.contains(&None))
        .count();
    let mut names: BTreeMap<&str, usize> = BTreeMap::new();
    let mut unnamed = 0;
    for o in &s.objects {
        match o.name.as_deref() {
            Some(n) => *names.entry(n).or_default() += 1,
            None => unnamed += 1,
        }
    }
    let mut names: Vec<(&str, usize)> = names.into_iter().collect();
    names.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let mut shown: Vec<String> = names
        .iter()
        .take(8)
        .map(|&(n, c)| {
            if c > 1 {
                format!("{n} ×{c}")
            } else {
                n.to_string()
            }
        })
        .collect();
    if names.len() > 8 {
        shown.push("…".into());
    }
    let yards = s.rooms.iter().filter(|r| !r.roofed).count();
    let rooms = if yards > 0 {
        format!("{} rooms ({yards} yards)", s.rooms.len())
    } else {
        format!("{} rooms", s.rooms.len())
    };
    let mut line = format!(
        "at {},{}  {}×{}  {planes}  {rooms}  {} doors ({entrances} to outside)  {} open edges  {} objects ({unnamed} unnamed)",
        s.origin.0,
        s.origin.1,
        s.size.0,
        s.size.1,
        s.doors.len(),
        s.openings.len(),
        s.objects.len(),
    );
    if !shown.is_empty() {
        line += &format!(": {}", shown.join(", "));
    }
    if s.clipped {
        line += "  (clipped)";
    }
    line
}
