//! `rs2ool cache structures`: finds whole structures in the map with
//! [`rs2convert::structure`] and lists them, or writes them as JSON, one
//! file per region.
//!
//! A structure belongs to the region its origin (south-west corner) is in.
//! With `--region`, that region and its eight neighbours are searched, so
//! a structure that crosses into a neighbour comes out whole; one that
//! goes on past them is marked clipped. With `--all`, the whole map is
//! searched at once.
//!
//! Each structure also gets what the learned trees ([`rs2convert::learn`])
//! make of it: its class, its rooms' uses and its site. With `--learn`, the
//! command learns those trees from a labels file instead, and reports how
//! they do. With `--profiles`, it also writes a profile of each class
//! ([`rs2convert::profile`]) from what the trees make of the structures.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use rs2cache::types::config::FloDef;
use rs2cache::types::config::LocDef;
use rs2cache::types::map::Region;
use rs2cache::types::record::{Record, RecordType};
use rs2cache::{Cache, Records};
use rs2convert::learn::{self, Label, Trees};
use rs2convert::material;
use rs2convert::profile;
use rs2convert::purpose::{self, Class, Purpose, Use};
use rs2convert::structure::{self, REGION_SIZE, Structure};
use serde::{Deserialize, Serialize};

use crate::cache::{Outcome, plans, select};

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
    /// Also write a floor plan and a text sheet for each structure under
    /// `out`, in `plans/<rx>_<ry>/` (see [`crate::cache::plans`]).
    pub plans: bool,
    /// Also write a profile of each class under `out`, as
    /// `profiles/<class>.json`.
    pub profiles: bool,
    /// Learn the trees from this labels file and report how they do,
    /// instead of listing or writing structures.
    pub learn: Option<PathBuf>,
    /// With `learn`, save the trees here.
    pub save: Option<PathBuf>,
    /// Use the trees in this file rather than the built-in ones.
    pub trees: Option<PathBuf>,
}

/// A labels file: `{"labels": […]}` with any other fields alongside, or
/// just the list.
#[derive(Deserialize)]
#[serde(untagged)]
enum LabelsFile {
    Wrapped { labels: Vec<Label> },
    Bare(Vec<Label>),
}

/// What the learned trees make of a structure.
#[derive(Serialize)]
struct Learned {
    /// Its class.
    class: Class,
    /// Each room's use, in room order.
    rooms: Vec<Use>,
    /// The id of its site's first structure, when it's on a site with
    /// others.
    #[serde(skip_serializing_if = "Option::is_none")]
    site: Option<String>,
}

/// One region's file.
#[derive(Serialize)]
struct RegionFile<'a> {
    /// `RX_RY`.
    region: String,
    /// The structures whose origin is in it.
    structures: Vec<Entry<'a>>,
}

/// A structure's blueprint and what it's likely for, side by side: by the
/// rules, with their evidence, and by the learned trees.
#[derive(Serialize)]
struct Entry<'a> {
    #[serde(flatten)]
    structure: &'a Structure,
    purpose: Purpose,
    learned: Learned,
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
    if let Some(path) = &a.learn {
        learn_from(&found, path, a.save.as_deref(), out)?;
        return Ok(if failed > 0 {
            Outcome::Partial
        } else {
            Outcome::Ok
        });
    }
    let trees = match &a.trees {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;
            serde_json::from_str(&text).with_context(|| format!("reading {}", path.display()))?
        }
        None => Trees::built_in(),
    };
    let learned = learned(&trees, &found);
    // Each structure under the region its origin is in.
    let mut by_region: BTreeMap<(u8, u8), Vec<&Structure>> = BTreeMap::new();
    for s in &found {
        let r = region_of(s);
        if a.region.is_none_or(|w| w == r) {
            by_region.entry(r).or_default().push(s);
        }
    }
    if a.plans && a.out.is_none() {
        return Err(select::usage("--plans needs -o DIR"));
    }
    if a.profiles {
        let Some(dir) = &a.out else {
            return Err(select::usage("--profiles needs -o DIR"));
        };
        write_profiles(dir, &found, &learned, &by_region, out)?;
    }
    let purposes: Vec<Purpose> = found.iter().map(purpose::of).collect();
    match &a.out {
        Some(dir) if a.plans => {
            let neighbours = learn::neighbours(&found);
            for ((x, y), structures) in &by_region {
                let rdir = dir.join("plans").join(format!("{x}_{y}"));
                std::fs::create_dir_all(&rdir)
                    .with_context(|| format!("creating {}", rdir.display()))?;
                for s in structures {
                    let i = index_of(&found, s);
                    let near: Vec<(&Structure, &Purpose, u32)> = neighbours[i]
                        .iter()
                        .map(|&n| (&found[n], &purposes[n], structure::gap(s, &found[n])))
                        .collect();
                    let stem = rdir.join(structure::id(s));
                    let png = stem.with_extension("png");
                    std::fs::write(&png, crate::render::png(&plans::plan(s)))
                        .with_context(|| format!("writing {}", png.display()))?;
                    let md = stem.with_extension("md");
                    std::fs::write(&md, plans::sheet(s, &purposes[i], &near))
                        .with_context(|| format!("writing {}", md.display()))?;
                }
            }
            write_json(dir, &by_region, &purposes, &learned, &found, out)?;
        }
        Some(dir) => write_json(dir, &by_region, &purposes, &learned, &found, out)?,
        None => {
            for ((x, y), structures) in &by_region {
                for s in structures {
                    let i = index_of(&found, s);
                    let (l, p) = (&learned[i], &purposes[i]);
                    // The learned class, with a shop's wares from the rules'
                    // guess when they agree; the rules' guess beside it when
                    // they don't.
                    let guess = if l.class == p.class.what {
                        class(p)
                    } else {
                        format!("{} (rules: {})", name(&l.class), class(p))
                    };
                    let site = l
                        .site
                        .as_ref()
                        .map(|id| format!("  site {id}"))
                        .unwrap_or_default();
                    writeln!(out, "{x}_{y}  {guess:<28}  {}{site}", summary(s))?;
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

/// What the trees make of each structure, with sites named by their first
/// structure's id.
struct Made {
    class: Class,
    rooms: Vec<Use>,
    site: Option<String>,
    /// Its site's first structure, as an index; its own when it's alone.
    root: usize,
}

fn learned(trees: &Trees, found: &[Structure]) -> Vec<Made> {
    let sites = trees.sites(found);
    let mut members: BTreeMap<usize, usize> = BTreeMap::new();
    for &s in &sites {
        *members.entry(s).or_default() += 1;
    }
    found
        .iter()
        .zip(&sites)
        .map(|(s, &site)| {
            let (class, rooms) = trees.classify(s);
            Made {
                class,
                rooms,
                site: (members[&site] > 1).then(|| structure::id(&found[site])),
                root: site,
            }
        })
        .collect()
}

/// Where `s` is in `found`.
fn index_of(found: &[Structure], s: &Structure) -> usize {
    found
        .iter()
        .position(|f| std::ptr::eq(f, s))
        .expect("a found structure")
}

/// A class or use by the name it has in the JSON.
fn name(what: &impl Serialize) -> String {
    serde_json::to_value(what)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Learns the trees from the labels at `path`, prints how they do held out
/// against the rules, and saves them to `save` if given.
fn learn_from(
    found: &[Structure],
    path: &std::path::Path,
    save: Option<&std::path::Path>,
    out: &mut impl Write,
) -> Result<()> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let labels =
        match serde_json::from_str(&text).with_context(|| format!("reading {}", path.display()))? {
            LabelsFile::Wrapped { labels } | LabelsFile::Bare(labels) => labels,
        };
    let learned = learn::learn(found, &labels, learn::Sizes::default());
    let pct = |a: usize, b: usize| 100.0 * a as f64 / b.max(1) as f64;
    let c = learned.classes;
    writeln!(
        out,
        "{} labelled structures; held out over {} folds:",
        c.total,
        learn::FOLDS
    )?;
    writeln!(
        out,
        "  classes  learned {}/{} ({:.0}%), rules {}/{} ({:.0}%)",
        c.learned,
        c.total,
        pct(c.learned, c.total),
        c.rules,
        c.total,
        pct(c.rules, c.total)
    )?;
    let r = learned.rooms;
    writeln!(
        out,
        "  rooms    learned {}/{} ({:.0}%), rules {}/{} ({:.0}%)",
        r.learned,
        r.total,
        pct(r.learned, r.total),
        r.rules,
        r.total,
        pct(r.rules, r.total)
    )?;
    let s = learned.sites;
    writeln!(
        out,
        "  sites    {} neighbouring pairs, {} labelled as one site; {} predicted so rightly, {} wrongly (precision {:.0}%, recall {:.0}%)",
        s.pairs,
        s.labelled,
        s.right,
        s.wrong,
        pct(s.right, s.right + s.wrong),
        pct(s.right, s.labelled)
    )?;
    let mut mistakes: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (label, guess, _) in &learned.class_table {
        if label != guess {
            *mistakes.entry((name(label), name(guess))).or_default() += 1;
        }
    }
    let mut mistakes: Vec<_> = mistakes.into_iter().collect();
    mistakes.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let shown: Vec<String> = mistakes
        .iter()
        .take(10)
        .map(|((l, g), n)| format!("{l} as {g} ×{n}"))
        .collect();
    writeln!(out, "  commonest class mistakes: {}", shown.join(", "))?;
    if let Some(save) = save {
        // Compact: forests are big.
        let json = serde_json::to_string(&learned.trees)?;
        std::fs::write(save, json + "\n").with_context(|| format!("writing {}", save.display()))?;
        writeln!(out, "trees saved to {}", save.display())?;
    }
    Ok(())
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

/// Writes `structures/RX_RY.json` under `dir` for each region, and says
/// so on `out`.
fn write_json(
    dir: &std::path::Path,
    by_region: &BTreeMap<(u8, u8), Vec<&Structure>>,
    purposes: &[Purpose],
    learned: &[Made],
    found: &[Structure],
    out: &mut impl Write,
) -> Result<()> {
    let dir = dir.join("structures");
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    for ((x, y), structures) in by_region {
        let path = dir.join(format!("{x}_{y}.json"));
        let file = RegionFile {
            region: format!("{x}_{y}"),
            structures: structures
                .iter()
                .map(|s| {
                    let i = index_of(found, s);
                    let l = &learned[i];
                    Entry {
                        structure: s,
                        purpose: purposes[i].clone(),
                        learned: Learned {
                            class: l.class,
                            rooms: l.rooms.clone(),
                            site: l.site.clone(),
                        },
                    }
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
    Ok(())
}

/// Writes `DIR/profiles/<class>.json` for each class among the structures
/// `by_region` keeps.
fn write_profiles(
    dir: &std::path::Path,
    found: &[Structure],
    learned: &[Made],
    by_region: &BTreeMap<(u8, u8), Vec<&Structure>>,
    out: &mut impl Write,
) -> Result<()> {
    let kept: Vec<usize> = by_region
        .values()
        .flatten()
        .map(|s| index_of(found, s))
        .collect();
    let structures: Vec<Structure> = kept.iter().map(|&i| found[i].clone()).collect();
    let guesses: Vec<(Class, Vec<Use>)> = kept
        .iter()
        .map(|&i| (learned[i].class, learned[i].rooms.clone()))
        .collect();
    let sites: Vec<usize> = kept.iter().map(|&i| learned[i].root).collect();
    let profiles = profile::profiles(&structures, &guesses, &sites);
    let dir = dir.join("profiles");
    std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    for p in &profiles {
        let path = dir.join(format!("{}.json", name(&p.class)));
        let json = serde_json::to_string_pretty(p)?;
        std::fs::write(&path, json + "\n")
            .with_context(|| format!("writing {}", path.display()))?;
    }
    writeln!(
        out,
        "{} class profiles from {} structures, written to {}",
        profiles.len(),
        structures.len(),
        dir.display()
    )?;
    Ok(())
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
