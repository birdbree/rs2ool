//! `rs2ool cache render` against the cache in `RS2_CACHE` (skipped without
//! one): every type with a model draws, animations keep their frames and
//! timing, compare mode draws the converted clip where the original is,
//! and bad requests are usage errors. Images go under Cargo's per-target
//! temp directory.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use image::{AnimationDecoder, RgbaImage};
use rs2cache::types::record::{Record, RecordType};
use rs2cache::{Cache, Records};
use rs2convert::preview;
use rs2ool::cache::render::{Frames, Motion, RenderArgs, Target, run};
use rs2ool::render::{BACKGROUND, View};

mod common;

fn cache() -> Option<PathBuf> {
    let dir = std::env::var_os("RS2_CACHE").map(PathBuf::from);
    if dir.is_none() {
        eprintln!("skipping: RS2_CACHE is not set");
    }
    dir
}

fn tmp(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("render")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn render(cache: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rs2ool"))
        .arg("--cache")
        .arg(cache)
        .args(["cache", "render"])
        .args(args)
        .output()
        .expect("run rs2ool cache render")
}

fn ok(cache: &Path, args: &[&str]) -> String {
    let out = render(cache, args);
    assert!(
        out.status.success(),
        "render {args:?} exited {:?}\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn png(path: &Path) -> RgbaImage {
    image::open(path)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        .to_rgba8()
}

/// Pixels that aren't the background.
fn drawn(img: &RgbaImage) -> usize {
    let bg = BACKGROUND.to_be_bytes();
    img.pixels().filter(|p| p.0[..3] != bg[1..]).count()
}

/// Every `step`th id of `kind` whose record has a model with faces.
fn with_models(records: &Records, kind: RecordType, step: usize) -> Vec<u32> {
    let ids = records.ids(kind).unwrap();
    ids.into_iter()
        .filter(|&id| {
            records
                .decode(kind, id)
                .ok()
                .and_then(|r| preview::model_for(records, &r).ok().flatten())
                .is_some_and(|m| !m.faces.is_empty())
        })
        .step_by(step)
        .collect()
}

#[test]
fn every_type_with_a_model_draws() {
    let Some(dir) = cache() else { return };
    let cache = Cache::open(&dir).unwrap();
    let records = Records::new(&cache);
    let out = tmp("types");
    for kind in [
        RecordType::Model,
        RecordType::Npc,
        RecordType::Obj,
        RecordType::Idk,
        RecordType::SpotAnim,
        RecordType::Loc,
    ] {
        let ids = with_models(&records, kind, 97);
        assert!(!ids.is_empty(), "no {kind} has a model");
        for id in ids {
            let file = out.join(format!("{kind}-{id}.png"));
            // Faces are one-sided, so a flat model only shows from its
            // front: try both sides, from above and below.
            let views = [(256, 48), (256, -48), (1280, 48), (1280, -48)];
            let pixels = views.map(|(yaw, pitch)| {
                let args = RenderArgs {
                    target: Target::Record(kind, id),
                    motion: Motion::Still,
                    frames: Frames::All,
                    compare: false,
                    moving: None,
                    view: View {
                        width: 128,
                        height: 96,
                        yaw,
                        pitch,
                        ..View::default()
                    },
                    out: file.clone(),
                };
                run(&records, &args, &mut Vec::new())
                    .unwrap_or_else(|e| panic!("{kind} {id}: {e:#}"));
                let img = png(&file);
                assert_eq!(img.dimensions(), (128, 96));
                drawn(&img)
            });
            assert!(pixels.iter().any(|&n| n > 0), "{kind} {id} drew nothing");
        }
    }
    // Through the command line, the kit and a record, at the default size.
    let kit = out.join("kit.png");
    let line = ok(&dir, &["kit", "-o", kit.to_str().unwrap()]);
    assert!(line.ends_with("512×512 PNG\n"), "{line}");
    assert!(drawn(&png(&kit)) > 1000);
    let npc = common::walking_npc(&records).to_string();
    let file = out.join("npc.png");
    ok(&dir, &["npc", "--id", &npc, "-o", file.to_str().unwrap()]);
    let again = out.join("npc-again.png");
    ok(&dir, &["npc", "--id", &npc, "-o", again.to_str().unwrap()]);
    assert_eq!(
        std::fs::read(&file).unwrap(),
        std::fs::read(&again).unwrap()
    );
}

#[test]
fn animations_keep_frames_and_timing() {
    let Some(dir) = cache() else { return };
    let cache = Cache::open(&dir).unwrap();
    let records = Records::new(&cache);
    let npc = common::walking_npc(&records);
    let Record::Npc(d) = records.decode(RecordType::Npc, npc).unwrap() else {
        unreachable!()
    };
    let walk = d.walk_anim.unwrap();
    let index = cache.frame_index().unwrap();
    let subject = preview::animated(&records, &index, RecordType::Npc, npc, &[walk])
        .unwrap()
        .unwrap();
    let delays = subject.creature.delays(subject.sequence(walk).unwrap());
    let (npc, walk) = (npc.to_string(), walk.to_string());
    let out = tmp("animations");

    let file = out.join("walk.png");
    let line = ok(
        &dir,
        &[
            "npc",
            "--id",
            &npc,
            "--seq",
            &walk,
            "--size",
            "64",
            "-o",
            file.to_str().unwrap(),
        ],
    );
    assert!(
        line.contains(&format!("64×64 APNG, {} frames", delays.len())),
        "{line}"
    );
    let mut reader = png::Decoder::new(Cursor::new(std::fs::read(&file).unwrap()))
        .read_info()
        .unwrap();
    assert_eq!(
        reader.info().animation_control.unwrap().num_frames as usize,
        delays.len()
    );
    let mut buf = vec![0; reader.output_buffer_size().unwrap()];
    for &d in &delays {
        reader.next_frame(&mut buf).unwrap();
        let fc = reader.info().frame_control.unwrap();
        assert_eq!((u32::from(fc.delay_num), fc.delay_den), (d, 50));
    }

    let file = out.join("walk.gif");
    ok(
        &dir,
        &[
            "npc",
            "--id",
            &npc,
            "--seq",
            &walk,
            "--size",
            "64",
            "-o",
            file.to_str().unwrap(),
        ],
    );
    let frames = image::codecs::gif::GifDecoder::new(Cursor::new(std::fs::read(&file).unwrap()))
        .unwrap()
        .into_frames()
        .collect_frames()
        .unwrap();
    assert_eq!(frames.len(), delays.len());

    // One frame, and a sheet of four, at 256 a tile by default.
    let file = out.join("frame.png");
    let line = ok(
        &dir,
        &[
            "npc",
            "--id",
            &npc,
            "--seq",
            &walk,
            "--frame",
            "1",
            "-o",
            file.to_str().unwrap(),
        ],
    );
    assert!(line.ends_with("512×512 PNG\n"), "{line}");
    let file = out.join("sheet.png");
    ok(
        &dir,
        &[
            "npc",
            "--id",
            &npc,
            "--seq",
            &walk,
            "--sheet",
            "4",
            "-o",
            file.to_str().unwrap(),
        ],
    );
    assert_eq!(png(&file).dimensions(), (512, 512));

    // A spot animation plays its own sequence with a bare --seq.
    let spot = with_models(&records, RecordType::SpotAnim, 1)
        .into_iter()
        .find(|&s| {
            matches!(records.decode(RecordType::SpotAnim, s), Ok(Record::SpotAnim(d))
                if d.sequence.is_some())
        })
        .unwrap()
        .to_string();
    let file = out.join("spot.png");
    let line = ok(
        &dir,
        &[
            "spotanim",
            "--id",
            &spot,
            "--seq",
            "-o",
            file.to_str().unwrap(),
        ],
    );
    assert!(line.contains("APNG"), "{line}");
}

#[test]
fn compare_draws_the_converted_clip_over_the_original() {
    let Some(dir) = cache() else { return };
    let cache = Cache::open(&dir).unwrap();
    let records = Records::new(&cache);
    let npc = common::walking_npc(&records).to_string();
    let out = tmp("compare");
    let file = out.join("walk.png");
    let line = ok(
        &dir,
        &[
            "npc",
            "--id",
            &npc,
            "--seq",
            "--sheet",
            "3",
            "--compare",
            "-o",
            file.to_str().unwrap(),
        ],
    );
    // The stand sequence converts well under the validation threshold.
    let max: f64 = line
        .split("max error ")
        .nth(1)
        .and_then(|s| s.split(',').next())
        .unwrap()
        .parse()
        .unwrap();
    assert!(max < 4.0, "{line}");
    // A strip of the original over a strip of the converted, tile for tile
    // nearly the same picture.
    let img = png(&file);
    let (w, h) = img.dimensions();
    assert_eq!(h, 512);
    let differ = (0..w)
        .flat_map(|x| (0..256).map(move |y| (x, y)))
        .filter(|&(x, y)| img.get_pixel(x, y) != img.get_pixel(x, y + 256))
        .count();
    assert!(drawn(&img) > 1000);
    assert!(
        differ * 20 < drawn(&img),
        "{differ} of {} drawn pixels differ",
        drawn(&img)
    );
}

#[test]
fn an_animated_location_is_shaped_after_posing() {
    let Some(dir) = cache() else { return };
    let cache = Cache::open(&dir).unwrap();
    let records = Records::new(&cache);
    let index = cache.frame_index().unwrap();
    let id = records
        .ids(RecordType::Loc)
        .unwrap()
        .into_iter()
        .find(|&id| {
            matches!(records.decode(RecordType::Loc, id), Ok(Record::Loc(d))
                if d.animation.is_some() && d.scale_x != 128 && !d.models.is_empty())
        })
        .expect("a scaled location with an animation");
    let record = records.decode(RecordType::Loc, id).unwrap();
    let still = preview::model_for(&records, &record).unwrap().unwrap();
    let seq = preview::own_sequence(&records, RecordType::Loc, id)
        .unwrap()
        .unwrap();
    let subject = preview::animated(&records, &index, RecordType::Loc, id, &[seq])
        .unwrap()
        .unwrap();
    // Drawn at rest, it's the shaped model the still render draws.
    assert_eq!(subject.model(), still);
    assert_eq!(
        subject.finish(subject.creature.model.vertices.clone()),
        still.vertices
    );
    let out = tmp("location");
    let file = out.join("loc.png");
    ok(
        &dir,
        &[
            "loc",
            "--id",
            &id.to_string(),
            "--seq",
            "--frame",
            "0",
            "-o",
            file.to_str().unwrap(),
        ],
    );
    assert!(drawn(&png(&file)) > 0);
}

#[test]
fn bad_requests_are_usage_errors() {
    let Some(dir) = cache() else { return };
    let out = tmp("usage");
    let file = out.join("x.png");
    let f = file.to_str().unwrap();
    for args in [
        &["varp", "--id", "0", "-o", f][..],
        &["npc", "-o", f],
        &["kit", "--id", "1", "-o", f],
        &["model", "--id", "0", "--female", "-o", f],
        &["model", "--id", "0", "--seq", "-o", f],
        &["npc", "--id", "1", "--compare", "-o", f],
        &["kit", "--seq", "808", "--frame", "9999", "-o", f],
        &["kit", "--size", "0", "-o", f],
    ] {
        let o = render(&dir, args);
        assert_eq!(
            o.status.code(),
            Some(1),
            "{args:?}: {}",
            String::from_utf8_lossy(&o.stderr)
        );
    }
    assert!(!file.exists());
}

#[test]
fn an_action_renders_over_the_walk() {
    let Some(dir) = cache() else { return };
    let cache = Cache::open(&dir).unwrap();
    let records = Records::new(&cache);
    let index = cache.frame_index().unwrap();
    // The player's first action with an interleave list that shares the
    // walk's frame base.
    let body = preview::player_body(&records, &index, false, &[]).unwrap();
    let base = |c: &rs2convert::creature::Creature, id: u32| {
        let s = c.sequences.iter().find(|s| s.def.id == id)?;
        c.frame(s, 0).map(|(p, _)| p.base.clone())
    };
    let walk = base(&body.creature, 819);
    let action = records
        .ids(RecordType::Seq)
        .unwrap()
        .into_iter()
        .find(|&s| {
            matches!(records.decode(RecordType::Seq, s), Ok(Record::Seq(d)) if d.interleave_labels.is_some())
                && preview::player_body(&records, &index, false, &[s as u16])
                    .is_ok_and(|b| base(&b.creature, s) == walk)
        })
        .unwrap();
    let frames = {
        let b = preview::player_body(&records, &index, false, &[action as u16]).unwrap();
        b.creature.delays(b.sequence(action as u16).unwrap()).len()
    };
    let out = tmp("moving");
    let file = out.join("mixed.png");
    let a = action.to_string();
    let line = ok(
        &dir,
        &[
            "kit",
            "--seq",
            &a,
            "--moving",
            "819",
            "--compare",
            "--size",
            "64",
            "-o",
            file.to_str().unwrap(),
        ],
    );
    // Both clocks step it, so there are at least as many frames as the
    // action has, and the mix is validated on its own line.
    let n: usize = line
        .split("APNG, ")
        .nth(1)
        .and_then(|s| s.split(' ').next())
        .unwrap()
        .parse()
        .unwrap();
    assert!(n >= frames, "{line}");
    assert!(
        line.contains(&format!(
            "sequence {action} while moving (seq 819): max error"
        )),
        "{line}"
    );
    // A sequence without an interleave list plays alone, and says so.
    let line = ok(
        &dir,
        &[
            "kit",
            "--seq",
            "808",
            "--moving",
            "819",
            "--frame",
            "0",
            "-o",
            file.to_str().unwrap(),
        ],
    );
    assert!(
        line.starts_with("sequence 808 doesn't mix with movement"),
        "{line}"
    );
}
