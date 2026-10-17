//! `cachedump dump` and `decode`.

use std::collections::BTreeMap;

use anyhow::Context;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use rs2cache::record::{entry_file, index_file, is_absent, summarize};
use rs2cache::{EntryName, Records, archive, compress, sprite};
use rs2intermediate::interface::Content;
use rs2intermediate::record::{Meta, Record, RecordType, Source, TypeSummary};
use rs2intermediate::sprite::Font;
use serde_json::Value;

use crate::output::{MetaMode, Output, Provenance};
use crate::select::usage;
use crate::xref::Xref;
use crate::{DumpArgs, Outcome, groups};

struct Ctx<'a> {
    records: &'a Records<'a>,
    out: &'a Output,
    xref: Option<&'a Xref>,
    jsonl: bool,
    quiet: bool,
}

fn progress(n: usize, what: &str, quiet: bool) -> ProgressBar {
    if quiet {
        return ProgressBar::hidden();
    }
    let bar = ProgressBar::new(n as u64);
    bar.set_style(
        ProgressStyle::with_template("{msg} {bar:40} {pos}/{len} {eta}")
            .unwrap_or_else(|_| ProgressStyle::default_bar()),
    );
    bar.set_message(what.to_owned());
    bar
}

pub fn run(records: &Records, args: &DumpArgs) -> anyhow::Result<Outcome> {
    let mode = if args.out.meta_only {
        MetaMode::Only
    } else if args.out.meta {
        MetaMode::With
    } else {
        MetaMode::Off
    };
    let out = Output::new(&args.out.dir, mode)?;
    let xref = if args.out.xref {
        Some(Xref::load_or_build(records, &out)?)
    } else {
        None
    };
    let ctx = Ctx {
        records,
        out: &out,
        xref: xref.as_ref(),
        jsonl: args.out.jsonl,
        quiet: args.out.quiet,
    };
    let s = &args.source;
    let ids_given = args.ids.given();
    if ids_given
        && (s.archive.is_some() || s.region.is_some() || s.all_regions || s.group.is_some())
    {
        return Err(usage(
            "--id, --range and --all apply to --index and --type only",
        ));
    }
    if let Some(index) = s.index {
        let available: Vec<u32> = records
            .cache()
            .entries(index)
            .map_err(|e| usage(e.to_string()))?
            .iter()
            .enumerate()
            .map(|(i, _)| i as u32)
            .collect();
        let ids = args.ids.resolve(&format!("index {index}"), &available)?;
        if args.decode {
            let t = match index {
                1 => RecordType::Model,
                2 => RecordType::Anim,
                3 => RecordType::Midi,
                0 => {
                    return Err(usage(
                        "index 0 holds archives; decode with --archive <name> --decode",
                    ));
                }
                _ => {
                    return Err(usage(
                        "index 4 files are per region; decode with --region or --all-regions",
                    ));
                }
            };
            dump_types(&ctx, t, &ids);
        } else {
            dump_index(&ctx, index, &ids, args.raw);
        }
    } else if let Some(name) = &s.archive {
        let n =
            archive::archive_by_name(name).ok_or_else(|| usage(format!("no archive {name:?}")))?;
        dump_archive(&ctx, n, args.entry.as_deref(), args.raw, args.decode)?;
    } else if let Some(t) = s.kind {
        let ids = args.ids.resolve(t.name(), &records.ids(t)?)?;
        dump_types(&ctx, t, &ids);
    } else if s.region.is_some() || s.all_regions {
        let ids: Vec<u32> = match s.region {
            Some((x, y)) => vec![u32::from(x) << 8 | u32::from(y)],
            None => records.ids(RecordType::Map)?,
        };
        if args.decode {
            dump_types(&ctx, RecordType::Map, &ids);
        } else {
            dump_region_files(&ctx, &ids, args.raw);
        }
    } else if let Some(g) = &s.group {
        let deps = groups::resolve(records, g)?;
        let mut by_type: BTreeMap<RecordType, Vec<u32>> = BTreeMap::new();
        for (t, id) in deps {
            by_type.entry(t).or_default().push(id);
        }
        for (t, ids) in by_type {
            dump_types(&ctx, t, &ids);
        }
    }
    let (files, absent, failures) = out.finish()?;
    if !args.out.quiet {
        eprintln!("{files} files written, {absent} absent, {failures} failed");
    }
    Ok(if failures > 0 {
        Outcome::Partial
    } else {
        Outcome::Ok
    })
}

/// Path stem (no extension) for a record's outputs.
fn stem(records: &Records, t: RecordType, id: u32) -> anyhow::Result<String> {
    Ok(match t {
        RecordType::Map => format!("map/{}_{}", id >> 8, id & 0xff),
        RecordType::Font => format!("font/{}", sprite::FONTS[id as usize]),
        RecordType::Sprite => {
            let (n, entry) = records.sprite_source(id)?;
            let name = entry.to_string();
            let name = name.strip_suffix(".dat").unwrap_or(&name);
            format!(
                "sprite/{id}-{}-{name}",
                archive::archive_name(n).unwrap_or("?")
            )
        }
        t => format!("{t}/{id}"),
    })
}

/// Decode and write records of one type, in parallel.
fn dump_types(ctx: &Ctx, t: RecordType, ids: &[u32]) {
    let bar = progress(ids.len(), t.name(), ctx.quiet);
    let lines: Vec<Option<(u32, String)>> = ids
        .par_iter()
        .map(|&id| {
            let r = dump_one(ctx, t, id);
            bar.inc(1);
            match r {
                Ok(line) => line.map(|l| (id, l)),
                Err(e) => {
                    let absent = e.downcast_ref::<rs2cache::Error>().is_some_and(is_absent);
                    if absent {
                        ctx.out.absent();
                    } else {
                        ctx.out.fail(format!("{t} {id}"), format!("{e:#}"));
                    }
                    None
                }
            }
        })
        .collect();
    bar.finish_and_clear();
    if ctx.jsonl && ctx.out.meta != MetaMode::Only {
        let mut text = String::new();
        for (_, line) in lines.into_iter().flatten() {
            text.push_str(&line);
            text.push('\n');
        }
        let prov = Provenance::default();
        if let Err(e) = ctx
            .out
            .payload(&format!("{t}.jsonl"), text.as_bytes(), &prov)
        {
            ctx.out.fail(format!("{t}.jsonl"), e);
        }
    }
    if t == RecordType::Interface
        && ids.len() > 1
        && let Err(e) = write_interface_tree(ctx, ids)
    {
        ctx.out.fail("interface tree".into(), format!("{e:#}"));
    }
}

fn enrich(ctx: &Ctx, t: RecordType, id: u32, meta: &mut Meta) {
    let Some(x) = ctx.xref else { return };
    match &mut meta.summary {
        TypeSummary::Model { textures, .. } => {
            *textures = Some(x.model_textures.get(&id).cloned().unwrap_or_default());
        }
        TypeSummary::Sprite { used_by_models, .. } if t == RecordType::Texture => {
            *used_by_models = Some(x.texture_models.get(&id).cloned().unwrap_or_default());
        }
        _ => {}
    }
}

/// Write one record's outputs; with `--jsonl`, return its line instead.
fn dump_one(ctx: &Ctx, t: RecordType, id: u32) -> anyhow::Result<Option<String>> {
    let records = ctx.records;
    let record = records.decode(t, id)?;
    let mut meta = Meta {
        record_type: t,
        id,
        files: records.files(t, id)?,
        summary: summarize(&record),
    };
    enrich(ctx, t, id, &mut meta);
    let prov = Provenance::from_meta(&meta);
    let stem = stem(records, t, id)?;
    ctx.out.sidecar(&stem, &meta, &prov)?;
    if ctx.jsonl {
        return Ok(Some(serde_json::to_string(&record)?));
    }
    match &record {
        Record::Texture(s) => {
            let full = s.untrimmed(0).context("texture has no sprite")?;
            let sheet = rs2intermediate::sprite::SpriteSheet {
                sprites: vec![full],
                ..s.clone()
            };
            let sp = &sheet.sprites[0];
            ctx.out.png(
                &format!("{stem}.png"),
                sp.width.into(),
                sp.height.into(),
                sheet.rgba(0).unwrap_or_default(),
                &prov,
            )?;
        }
        Record::Sprite(s) => {
            for (i, sp) in s.sprites.iter().enumerate() {
                ctx.out.png(
                    &format!("{stem}-{i}.png"),
                    sp.width.into(),
                    sp.height.into(),
                    s.rgba(i).unwrap_or_default(),
                    &prov,
                )?;
            }
        }
        Record::Font(f) => {
            let (w, h, rgba) = glyph_sheet(f);
            ctx.out.png(&format!("{stem}.png"), w, h, rgba, &prov)?;
            let metrics = serde_json::json!({
                "name": f.name,
                "advance": f.advance,
                "draw_offset_x": f.draw_offset_x,
                "draw_offset_y": f.draw_offset_y,
                "glyph_sizes": f.glyphs.sprites.iter().map(|g| [g.width, g.height]).collect::<Vec<_>>(),
            });
            ctx.out
                .json(&format!("{stem}.metrics.json"), &metrics, &prov)?;
        }
        Record::Midi(bytes) => ctx.out.payload(&format!("{stem}.mid"), bytes, &prov)?,
        other => {
            // Write the inner struct, not the type-tagged wrapper.
            let mut v = serde_json::to_value(other)?;
            let inner = v.get_mut("record").map(Value::take).unwrap_or(v);
            ctx.out.json(&format!("{stem}.json"), &inner, &prov)?;
        }
    }
    Ok(None)
}

/// Lay a font's 256 glyphs out 16 × 16, each at its draw offsets.
fn glyph_sheet(f: &Font) -> (u32, u32, Vec<u8>) {
    let g = &f.glyphs.sprites;
    let cw = g
        .iter()
        .map(|s| usize::from(s.width) + 2)
        .max()
        .unwrap_or(1);
    let ch = g
        .iter()
        .map(|s| usize::from(s.height) + usize::from(s.y_offset))
        .max()
        .unwrap_or(1);
    let (w, h) = (cw * 16, ch * 16);
    let mut out = vec![0u8; w * h * 4];
    for (i, s) in g.iter().enumerate() {
        let Some(rgba) = f.glyphs.rgba(i) else {
            continue;
        };
        let (ox, oy) = (
            (i % 16) * cw + f.draw_offset_x[i].max(0) as usize,
            (i / 16) * ch + usize::from(s.y_offset),
        );
        for y in 0..usize::from(s.height) {
            for x in 0..usize::from(s.width) {
                let src = (y * usize::from(s.width) + x) * 4;
                let dst = ((oy + y) * w + ox + x) * 4;
                if dst + 4 <= out.len() {
                    out[dst..dst + 4].copy_from_slice(&rgba[src..src + 4]);
                }
            }
        }
    }
    (w as u32, h as u32, out)
}

/// `interface/tree.json`: every root (a component whose parent is itself)
/// with its layer children nested.
fn write_interface_tree(ctx: &Ctx, ids: &[u32]) -> anyhow::Result<()> {
    let mut all = BTreeMap::new();
    for &id in ids {
        if let Ok(Record::Interface(i)) = ctx.records.decode(RecordType::Interface, id) {
            all.insert(i.id, i);
        }
    }
    fn node(
        all: &BTreeMap<u16, rs2intermediate::interface::Interface>,
        id: u16,
        depth: usize,
    ) -> Value {
        let Some(i) = all.get(&id) else {
            return serde_json::json!({ "id": id, "missing": true });
        };
        let mut v = serde_json::to_value(i).unwrap_or(Value::Null);
        if let (Content::Layer { children, .. }, Some(obj)) = (&i.content, v.as_object_mut()) {
            // Interfaces are trees in 317; the depth bound only guards
            // against a malformed cache.
            let kids: Vec<Value> = if depth < 32 {
                children
                    .iter()
                    .map(|c| {
                        let mut n = node(all, c.id, depth + 1);
                        if let Some(o) = n.as_object_mut() {
                            o.insert("x".into(), c.x.into());
                            o.insert("y".into(), c.y.into());
                        }
                        n
                    })
                    .collect()
            } else {
                Vec::new()
            };
            obj.insert("children".into(), kids.into());
        }
        v
    }
    let roots: Vec<Value> = all
        .values()
        .filter(|i| i.parent == Some(i.id))
        .map(|i| node(&all, i.id, 0))
        .collect();
    let prov = Provenance {
        sources: vec![Source::Archive {
            archive: archive::INTERFACE,
            entry: "data".into(),
        }],
        ..Default::default()
    };
    ctx.out.json("interface/tree.json", &roots, &prov)
}

fn dump_index(ctx: &Ctx, index: u8, ids: &[u32], raw: bool) {
    let cache = ctx.records.cache();
    let bar = progress(ids.len(), &format!("idx{index}"), ctx.quiet);
    ids.par_iter().for_each(|&id| {
        let r = (|| -> anyhow::Result<()> {
            let fm = index_file(cache, index, id)?;
            let prov = Provenance {
                sources: vec![fm.source.clone()],
                raw_size: fm.raw_size,
                inflated_size: fm.inflated_size,
            };
            let stored = cache.read_raw(index, id)?;
            let bytes = if raw {
                stored
            } else {
                compress::inflate(&stored)?
            };
            let stem = format!("idx{index}/{id}");
            ctx.out.payload(&format!("{stem}.bin"), &bytes, &prov)?;
            let t = match index {
                1 => Some(RecordType::Model),
                2 => Some(RecordType::Anim),
                3 => Some(RecordType::Midi),
                _ => None,
            };
            match t.filter(|_| ctx.out.meta != MetaMode::Off) {
                Some(t) => {
                    let mut meta = ctx.records.meta(t, id)?;
                    enrich(ctx, t, id, &mut meta);
                    ctx.out.sidecar(&stem, &meta, &prov)?;
                }
                None => ctx.out.sidecar(&stem, &fm, &prov)?,
            }
            Ok(())
        })();
        bar.inc(1);
        if let Err(e) = r {
            let absent = e.downcast_ref::<rs2cache::Error>().is_some_and(is_absent)
                || e.downcast_ref::<rs2cache::CacheError>()
                    .is_some_and(|c| matches!(c, rs2cache::CacheError::Absent { .. }));
            if absent {
                ctx.out.absent();
            } else {
                ctx.out.fail(format!("idx{index} {id}"), format!("{e:#}"));
            }
        }
    });
    bar.finish_and_clear();
}

fn dump_region_files(ctx: &Ctx, ids: &[u32], raw: bool) {
    let cache = ctx.records.cache();
    for &id in ids {
        let r = (|| -> anyhow::Result<()> {
            let e = ctx.records.region_entry(id)?;
            let stem = format!("map/{}_{}", e.x(), e.y());
            for (file, ext) in [(e.tile_file, "m"), (e.location_file, "l")] {
                let fm = index_file(cache, 4, file.into())?;
                let prov = Provenance {
                    sources: vec![fm.source.clone()],
                    raw_size: fm.raw_size,
                    inflated_size: fm.inflated_size,
                };
                let stored = cache.read_raw(4, file.into())?;
                let bytes = if raw {
                    stored
                } else {
                    compress::inflate(&stored)?
                };
                ctx.out
                    .payload(&format!("{stem}.{ext}.bin"), &bytes, &prov)?;
                ctx.out.sidecar(&format!("{stem}.{ext}"), &fm, &prov)?;
            }
            Ok(())
        })();
        if let Err(err) = r {
            ctx.out.fail(
                format!("region {}_{}", id >> 8, id & 0xff),
                format!("{err:#}"),
            );
        }
    }
}

/// Record types decoded from an archive with `--archive --decode`.
fn archive_types(n: u8) -> &'static [RecordType] {
    match n {
        archive::TITLE => &[RecordType::Font, RecordType::Sprite],
        archive::CONFIG => &[
            RecordType::Loc,
            RecordType::Npc,
            RecordType::Obj,
            RecordType::Idk,
            RecordType::Seq,
            RecordType::SpotAnim,
            RecordType::Flo,
            RecordType::Varp,
            RecordType::Varbit,
        ],
        archive::INTERFACE => &[RecordType::Interface],
        archive::MEDIA => &[RecordType::Sprite],
        archive::TEXTURES => &[RecordType::Texture],
        _ => &[],
    }
}

fn dump_archive(
    ctx: &Ctx,
    n: u8,
    entry: Option<&str>,
    raw: bool,
    decode: bool,
) -> anyhow::Result<()> {
    let dir = format!("archive{n}");
    if raw {
        let fm = index_file(ctx.records.cache(), 0, n.into())?;
        let prov = Provenance {
            sources: vec![fm.source.clone()],
            raw_size: fm.raw_size,
            inflated_size: fm.inflated_size,
        };
        let bytes = ctx.records.cache().read_raw(0, n.into())?;
        ctx.out.payload(&format!("{dir}/{n}.jag"), &bytes, &prov)?;
        return ctx.out.sidecar(&format!("{dir}/{n}"), &fm, &prov);
    }
    if decode {
        if entry.is_some() {
            return Err(usage(
                "--decode works on a whole archive; drop --entry or use --type",
            ));
        }
        if n == archive::VERSIONLIST {
            let v = ctx.records.cache().versionlist()?;
            let prov = Provenance {
                sources: vec![Source::Archive {
                    archive: n,
                    entry: "*".into(),
                }],
                ..Default::default()
            };
            return ctx.out.json("versionlist.json", &v, &prov);
        }
        let types = archive_types(n);
        if types.is_empty() {
            return Err(usage(format!(
                "archive {n} has no decoder; dump its entries without --decode"
            )));
        }
        for &t in types {
            let mut ids = ctx.records.ids(t)?;
            if t == RecordType::Sprite {
                ids.retain(|&id| ctx.records.sprite_source(id).is_ok_and(|(a, _)| a == n));
            }
            dump_types(ctx, t, &ids);
        }
        return Ok(());
    }
    let a = ctx.records.archive(n)?;
    let wanted = entry.map(EntryName::parse);
    let mut found = false;
    for e in &a.entries {
        if wanted.as_ref().is_some_and(|w| w.hash() != e.hash) {
            continue;
        }
        found = true;
        let fm = entry_file(n, &a, e);
        let prov = Provenance {
            sources: vec![fm.source.clone()],
            raw_size: fm.raw_size,
            inflated_size: fm.inflated_size,
        };
        let stem = format!("{dir}/{}", e.entry_name());
        ctx.out.payload(&stem, &e.data, &prov)?;
        ctx.out.sidecar(&stem, &fm, &prov)?;
    }
    if !found {
        let w = wanted.map(|w| w.to_string()).unwrap_or_default();
        ctx.out
            .fail(format!("archive {n} entry {w}"), "no such entry");
    }
    Ok(())
}
