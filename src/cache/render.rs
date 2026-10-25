//! `render`: draw a record's model with the classic renderer into an
//! image file, still or animated.
//!
//! Without a sequence it draws the model at rest. With one it draws the
//! whole sequence as an animation, one frame of it, or a grid of frames
//! sampled evenly in time. With `compare`, every frame is drawn twice: as
//! the original client deforms the model, and as the converted rig and
//! clip pose it, the same conversion `rs2ool pack` writes. An animation or
//! a single frame shows the two side by side, the original on the left; a
//! sheet shows a strip of the original's frames over a strip of the
//! converted ones.
//!
//! The file's extension picks the format: `.gif` writes a GIF, anything
//! else a PNG, animated (APNG) when there's more than one frame.

use std::io::Write;
use std::path::PathBuf;

use anyhow::{Context, Result};
use contentpack::records::anim::Clip;
use contentpack::records::rig::Rig;
use image::RgbaImage;
use rs2cache::Records;
use rs2cache::types::record::RecordType;
use rs2classic::light;
use rs2convert::palette::Palette;
use rs2convert::pipeline::{self, PLAYER_SEQUENCES};
use rs2convert::preview::{self, Subject};
use rs2convert::validate::{self, ClipReport};

use super::Outcome;
use super::select::usage;
use crate::render::{self, BORDER, View};

/// What to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The model a record describes, as the explorer shows it.
    Record(RecordType, u32),
    /// The default player body, as `rs2ool pack player-kit` builds it.
    Kit {
        /// The female body instead of the male one.
        female: bool,
    },
}

/// Which sequence poses the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Motion {
    /// None: the model at rest.
    #[default]
    Still,
    /// The record's own: an NPC's stand sequence, a spot animation's or a
    /// location's sequence, the player's stand sequence for the kit.
    Own,
    /// This sequence.
    Seq(u16),
}

/// What to draw of a sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Frames {
    /// Every frame, as an animation.
    #[default]
    All,
    /// One frame, by its index in the sequence.
    One(usize),
    /// A grid of up to this many frames, evenly spaced in time.
    Sheet(usize),
}

/// The options of `render`.
#[derive(Debug, Clone)]
pub struct RenderArgs {
    /// What to draw.
    pub target: Target,
    /// Which sequence poses it.
    pub motion: Motion,
    /// What to draw of the sequence; ignored when still.
    pub frames: Frames,
    /// Draw the converted rig and clip beside the original.
    pub compare: bool,
    /// The image size (of each tile of a sheet) and camera.
    pub view: View,
    /// The file to write.
    pub out: PathBuf,
}

/// Draw what `a` asks for into its file and print a line saying what was
/// written, plus the clip's validation with `compare`. A flagged clip is
/// a partial failure, as in `rs2ool pack`.
pub fn run(records: &Records, a: &RenderArgs, report: &mut impl Write) -> Result<Outcome> {
    let gif = a
        .out
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("gif"));
    let textures = render::textures(records);
    let seq = match (a.motion, a.target) {
        (Motion::Still, _) => None,
        (Motion::Seq(s), _) => Some(s),
        (Motion::Own, Target::Kit { .. }) => Some(PLAYER_SEQUENCES[0]),
        (Motion::Own, Target::Record(kind, id)) => Some(
            preview::own_sequence(records, kind, id)?
                .ok_or_else(|| usage(format!("{kind} {id} has no sequence of its own")))?,
        ),
    };
    let Some(seq) = seq else {
        if a.compare {
            return Err(usage("--compare needs a sequence"));
        }
        let model = match a.target {
            Target::Record(kind, id) => {
                let record = records.decode(kind, id)?;
                preview::model_for(records, &record)?
                    .ok_or_else(|| usage(format!("{kind} {id} has no model")))?
            }
            Target::Kit { female } => body(records, female, &[])?.model(),
        };
        let poses = [model.vertices.clone()];
        let framing = render::frame(&a.view, &poses, None)
            .ok_or_else(|| usage("the model has no vertices"))?;
        let lit = light(&model, &light::NPC);
        let img = render::image(&render::draw(
            &a.view,
            &framing,
            &lit,
            &model.vertices,
            &textures,
        ));
        save(a, gif, &[img], &[1], report)?;
        return Ok(Outcome::Ok);
    };

    let subject = match a.target {
        Target::Record(kind, id) => {
            let index = records.cache().frame_index()?;
            preview::animated(records, &index, kind, id, &[seq])?
                .ok_or_else(|| usage(format!("{kind} {id} has no model")))?
        }
        Target::Kit { female } => body(records, female, &[seq])?,
    };
    let s = subject.sequence(seq).expect("the sequence was loaded");
    let c = &subject.creature;
    let delays = c.delays(s);
    let shown: Vec<usize> = match a.frames {
        Frames::All => (0..delays.len()).collect(),
        Frames::Sheet(n) => render::sample(&delays, n),
        Frames::One(f) if f < delays.len() => vec![f],
        Frames::One(f) => {
            return Err(usage(format!(
                "sequence {seq} has {} frames; there's no frame {f}",
                delays.len()
            )));
        }
    };
    let reference: Vec<Vec<[i32; 3]>> = shown
        .iter()
        .map(|&i| subject.posed(s, i))
        .collect::<Option<_>>()
        .with_context(|| format!("sequence {seq} won't pose {}", c.name))?;
    let converted = if a.compare {
        let (rig, clip, clip_report) = convert(&subject, s)?;
        let poses: Vec<Vec<[i32; 3]>> = shown
            .iter()
            .map(|&i| subject.finish(validate::converted(c, &rig, &clip, i as u8)))
            .collect();
        Some((poses, clip_report))
    } else {
        None
    };

    let mut all = reference.clone();
    if let Some((poses, _)) = &converted {
        all.extend(poses.iter().cloned());
    }
    let framing = render::frame(&a.view, &all, reference.first().map(Vec::as_slice))
        .ok_or_else(|| usage("the model has no vertices"))?;
    let lit = light(&subject.model(), &light::NPC);
    let sheet = matches!(a.frames, Frames::Sheet(_));
    let panel = |pose: &[[i32; 3]]| {
        let mut fb = render::draw(&a.view, &framing, &lit, pose, &textures);
        if sheet || a.compare {
            render::outline(&mut fb, BORDER);
        }
        render::image(&fb)
    };
    let images: Vec<RgbaImage> = reference.iter().map(|p| panel(p)).collect();
    let converted_images: Option<Vec<RgbaImage>> = converted
        .as_ref()
        .map(|(poses, _)| poses.iter().map(|p| panel(p)).collect());
    if sheet {
        // Square, or with compare a strip of the original over a strip of
        // the converted.
        let img = match converted_images {
            Some(c) => render::grid(&[images.clone(), c].concat(), Some(images.len())),
            None => render::grid(&images, None),
        };
        save(a, gif, &[img], &[1], report)?;
    } else {
        let frames = match converted_images {
            Some(c) => images
                .iter()
                .zip(c)
                .map(|(a, b)| render::grid(&[a.clone(), b], Some(2)))
                .collect(),
            None => images,
        };
        let delays: Vec<u32> = shown.iter().map(|&i| delays[i]).collect();
        save(a, gif, &frames, &delays, report)?;
    }
    Ok(match converted {
        Some((_, r)) => {
            writeln!(
                report,
                "sequence {seq}: max error {:.2}, mean {:.2}{}",
                r.max,
                r.mean,
                if r.flagged { "\tFLAGGED" } else { "" }
            )?;
            if r.flagged {
                Outcome::Partial
            } else {
                Outcome::Ok
            }
        }
        None => Outcome::Ok,
    })
}

/// The default player body with the stance sequences and `seqs`.
fn body(records: &Records, female: bool, seqs: &[u16]) -> Result<Subject> {
    let index = records.cache().frame_index()?;
    Ok(preview::player_body(records, &index, female, seqs)?)
}

/// Convert the subject's creature as `rs2ool pack` would and return the
/// rig and sequence `s`'s clip, both through an encode and decode round
/// trip, and the clip's validation.
fn convert(subject: &Subject, s: usize) -> Result<(Rig, Clip, ClipReport)> {
    let c = &subject.creature;
    // The rig and clip don't depend on the colours, so any palette will do.
    let palette = Palette::from_rgb(vec![[0, 0, 0]]);
    let out = pipeline::convert(c.clone(), &palette, 0, false, None)?;
    let seq = &c.sequences[s];
    let clip = rs2convert::clip::convert(c, &out.inferred, seq, 0)?;
    let clip = Clip::decode(&clip.encode()?)?;
    let report = validate::validate(c, &out.rig, &clip, seq);
    Ok((out.rig, clip, report))
}

/// Write `frames` to the output file, as a GIF or a PNG (an APNG when
/// there's more than one), and say so.
fn save(
    a: &RenderArgs,
    gif: bool,
    frames: &[RgbaImage],
    delays: &[u32],
    report: &mut impl Write,
) -> Result<()> {
    let (bytes, format) = if gif {
        (render::gif(frames, delays)?, "GIF")
    } else if frames.len() == 1 {
        (render::png(&frames[0]), "PNG")
    } else {
        (render::apng(frames, delays)?, "APNG")
    };
    if let Some(dir) = a.out.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    std::fs::write(&a.out, &bytes).with_context(|| format!("writing {}", a.out.display()))?;
    let (w, h) = frames.first().map_or((0, 0), |f| f.dimensions());
    write!(report, "{}\t{w}×{h} {format}", a.out.display())?;
    if frames.len() > 1 {
        let ms: u32 = delays.iter().sum::<u32>() * render::CYCLE_MS;
        write!(
            report,
            ", {} frames, {:.2} s",
            frames.len(),
            f64::from(ms) / 1000.0
        )?;
    }
    writeln!(report)?;
    Ok(())
}
