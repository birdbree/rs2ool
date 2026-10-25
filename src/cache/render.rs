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
//! With `moving`, a sequence the original client mixes with movement plays
//! over one of the creature's movement sequences, each on its own clock,
//! as the original client draws a creature acting while it walks; the
//! converted side mixes the two clips through the moving bones.
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
    /// A movement sequence to play under the sequence.
    pub moving: Option<u16>,
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

    let seqs: Vec<u16> = std::iter::once(seq).chain(a.moving).collect();
    let subject = match a.target {
        Target::Record(kind, id) => {
            let index = records.cache().frame_index()?;
            preview::animated(records, &index, kind, id, &seqs)?
                .ok_or_else(|| usage(format!("{kind} {id} has no model")))?
        }
        Target::Kit { female } => body(records, female, &seqs)?,
    };
    let s = subject.sequence(seq).expect("the sequence was loaded");
    let m = a
        .moving
        .map(|m| subject.sequence(m).expect("the sequence was loaded"));
    let c = &subject.creature;
    if m.is_some() && c.sequences[s].interleave().is_none() {
        writeln!(
            report,
            "sequence {seq} doesn't mix with movement; it plays alone"
        )?;
    }
    let m = m.filter(|_| c.sequences[s].interleave().is_some());
    let steps = steps(&subject, s, m, a.frames).map_err(|f| {
        usage(format!(
            "sequence {seq} has {} frames; there's no frame {f}",
            c.sequences[s].def.frames.len()
        ))
    })?;
    let reference: Vec<Vec<[i32; 3]>> = steps
        .iter()
        .map(|st| match (m, st.moving) {
            (Some(m), Some(j)) => subject.posed_moving(s, st.frame, m, j),
            _ => subject.posed(s, st.frame),
        })
        .collect::<Option<_>>()
        .with_context(|| format!("sequence {seq} won't pose {}", c.name))?;
    // Face transparency where a frame fades the model.
    let fades: Vec<Option<Vec<u8>>> = steps
        .iter()
        .map(|st| match (m, st.moving) {
            (Some(m), Some(j)) => subject.faded_moving(s, st.frame, m, j),
            _ => subject.faded(s, st.frame),
        })
        .collect();
    let converted = if a.compare {
        let (rig, clips, reports) = convert(&subject, s, m)?;
        let poses: Vec<Vec<[i32; 3]>> = steps
            .iter()
            .map(|st| {
                let t = st.frame as u8;
                subject.finish(match &clips[..] {
                    [clip, moving, ..] => {
                        let tm = st.moving.unwrap_or(0) as u8;
                        validate::converted_moving(c, &rig, clip, t, moving, tm)
                    }
                    [clip] => validate::converted(c, &rig, clip, t),
                    [] => unreachable!("convert gives the sequence's clip"),
                })
            })
            .collect();
        // A mix takes the clip's fades.
        let alphas: Vec<Vec<u8>> = steps
            .iter()
            .map(|st| validate::converted_alphas(c, &clips[0], st.frame as u8))
            .collect();
        Some((poses, alphas, reports))
    } else {
        None
    };

    let mut all = reference.clone();
    if let Some((poses, ..)) = &converted {
        all.extend(poses.iter().cloned());
    }
    let framing = render::frame(&a.view, &all, reference.first().map(Vec::as_slice))
        .ok_or_else(|| usage("the model has no vertices"))?;
    let lit = light(&subject.model(), &light::NPC);
    let sheet = matches!(a.frames, Frames::Sheet(_));
    let panel = |pose: &[[i32; 3]], alphas: Option<&Vec<u8>>| {
        let faded;
        let lit = match alphas {
            Some(f) => {
                faded = rs2classic::LitModel {
                    alphas: f.clone(),
                    ..lit.clone()
                };
                &faded
            }
            None => &lit,
        };
        let mut fb = render::draw(&a.view, &framing, lit, pose, &textures);
        if sheet || a.compare {
            render::outline(&mut fb, BORDER);
        }
        render::image(&fb)
    };
    let images: Vec<RgbaImage> = reference
        .iter()
        .zip(&fades)
        .map(|(p, f)| panel(p, f.as_ref()))
        .collect();
    let converted_images: Option<Vec<RgbaImage>> = converted.as_ref().map(|(poses, alphas, _)| {
        poses
            .iter()
            .zip(alphas)
            .map(|(p, f)| panel(p, Some(f)))
            .collect()
    });
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
        let delays: Vec<u32> = steps.iter().map(|st| st.delay).collect();
        save(a, gif, &frames, &delays, report)?;
    }
    let Some((_, _, reports)) = converted else {
        return Ok(Outcome::Ok);
    };
    let mut flagged = false;
    for (name, max, mean, over) in reports {
        writeln!(
            report,
            "{name}: max error {max:.2}, mean {mean:.2}{}",
            if over { "\tFLAGGED" } else { "" }
        )?;
        flagged |= over;
    }
    Ok(if flagged {
        Outcome::Partial
    } else {
        Outcome::Ok
    })
}

/// One drawn frame: the sequence's frame, the movement sequence's frame
/// under it if one plays, and how long it shows, in client cycles.
struct Step {
    frame: usize,
    moving: Option<usize>,
    delay: u32,
}

/// What to draw of sequence `s`, with movement sequence `m` under it.
///
/// Alone, it's the frames `frames` picks, each for its own delay. With a
/// movement sequence, the two run on their own clocks from the sequence's
/// start, the movement looping as the original client loops it: a sheet
/// or a single frame pairs each of the sequence's frames with the movement
/// frame showing when it starts, and an animation steps whenever either
/// changes frame, over one run of the sequence. `Err` names a frame past
/// the end.
fn steps(
    subject: &Subject,
    s: usize,
    m: Option<usize>,
    frames: Frames,
) -> Result<Vec<Step>, usize> {
    let c = &subject.creature;
    let delays = c.delays(s);
    let starts: Vec<u64> = delays
        .iter()
        .scan(0u64, |t, &d| {
            let start = *t;
            *t += u64::from(d);
            Some(start)
        })
        .collect();
    let end: u64 = delays.iter().map(|&d| u64::from(d)).sum();
    let moving = m.map(|m| (c.delays(m), c.sequences[m].def.loop_offset));
    let moving_at = |t: u64| moving.as_ref().map(|(d, lo)| frame_at(d, *lo, t));
    let shown = match frames {
        Frames::All => None,
        Frames::Sheet(n) => Some(render::sample(&delays, n)),
        Frames::One(f) if f < delays.len() => Some(vec![f]),
        Frames::One(f) => return Err(f),
    };
    if let Some(shown) = shown {
        return Ok(shown
            .into_iter()
            .map(|i| Step {
                frame: i,
                moving: moving_at(starts[i]),
                delay: delays[i],
            })
            .collect());
    }
    // Every moment either sequence changes frame, over one run of this one.
    let mut times: Vec<u64> = starts.clone();
    if let Some((d, lo)) = &moving {
        let mut t = 0u64;
        let mut j = 0usize;
        while t < end && !d.is_empty() {
            times.push(t);
            t += u64::from(d[j].max(1));
            j = next_frame(j, d.len(), *lo);
        }
    }
    times.sort_unstable();
    times.dedup();
    Ok(times
        .iter()
        .enumerate()
        .map(|(k, &t)| Step {
            frame: starts.partition_point(|&st| st <= t) - 1,
            moving: moving_at(t),
            delay: (times.get(k + 1).copied().unwrap_or(end) - t) as u32,
        })
        .collect())
}

/// The frame after `j` of a looping sequence of `n` frames: past the end,
/// back `loop_offset` frames from the end, or to the start.
fn next_frame(j: usize, n: usize, loop_offset: Option<u16>) -> usize {
    if j + 1 < n {
        return j + 1;
    }
    match loop_offset {
        Some(o) if o > 0 && usize::from(o) <= n => n - usize::from(o),
        _ => 0,
    }
}

/// The frame a looping sequence with these delays shows `t` cycles in.
fn frame_at(delays: &[u32], loop_offset: Option<u16>, t: u64) -> usize {
    let (mut at, mut j) = (0u64, 0usize);
    if delays.is_empty() {
        return 0;
    }
    loop {
        let d = u64::from(delays[j].max(1));
        if t < at + d {
            return j;
        }
        at += d;
        j = next_frame(j, delays.len(), loop_offset);
    }
}

/// The default player body with the stance sequences and `seqs`.
fn body(records: &Records, female: bool, seqs: &[u16]) -> Result<Subject> {
    let index = records.cache().frame_index()?;
    Ok(preview::player_body(records, &index, female, seqs)?)
}

/// What a comparison reports for one clip: a name, the worst and mean
/// errors, and whether that's over the threshold.
type Report = (String, f64, f64, bool);

/// Convert the subject's creature as `rs2ool pack` would and return the
/// rig, sequence `s`'s clip (and movement sequence `m`'s after it), all
/// through an encode and decode round trip, and the validation: the clip
/// alone, and mixed with the movement clip when their frames share a
/// base.
fn convert(subject: &Subject, s: usize, m: Option<usize>) -> Result<(Rig, Vec<Clip>, Vec<Report>)> {
    let c = &subject.creature;
    // The rig and clip don't depend on the colours, so any palette will do.
    let palette = Palette::from_rgb(vec![[0, 0, 0]]);
    let out = pipeline::convert(c.clone(), &palette, 0, false, None)?;
    let clip = |i: usize| -> Result<Clip> {
        let clip = rs2convert::clip::convert(c, &out.inferred, &c.sequences[i], 0)?;
        Ok(Clip::decode(&clip.encode()?)?)
    };
    let mut clips = vec![clip(s)?];
    let r: ClipReport = validate::validate(c, &out.rig, &clips[0], &c.sequences[s]);
    let id = c.sequences[s].def.id;
    let mut reports = vec![(format!("sequence {id}"), r.max, r.mean, r.flagged)];
    if let Some(m) = m {
        clips.push(clip(m)?);
        let base = |i: usize| c.frame(&c.sequences[i], 0).map(|(p, _)| &p.base);
        if base(s) == base(m) {
            let r = validate::validate_moving(
                c,
                &out.rig,
                &clips[0],
                &c.sequences[s],
                &clips[1],
                &c.sequences[m],
            );
            let name = format!("sequence {id} while moving (seq {})", r.sequence);
            reports.push((name, r.max, r.mean, r.flagged));
        }
    }
    Ok((out.rig, clips, reports))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looping_sequences_step_back_by_their_offset() {
        assert_eq!(next_frame(0, 3, None), 1);
        assert_eq!(next_frame(2, 3, None), 0);
        assert_eq!(next_frame(2, 3, Some(2)), 1);
        assert_eq!(
            next_frame(2, 3, Some(9)),
            0,
            "an offset past the start restarts"
        );
        let delays = [2, 3, 1];
        let at: Vec<usize> = (0..9).map(|t| frame_at(&delays, None, t)).collect();
        assert_eq!(at, [0, 0, 1, 1, 1, 2, 0, 0, 1]);
        let at: Vec<usize> = (6..10).map(|t| frame_at(&delays, Some(2), t)).collect();
        assert_eq!(at, [1, 1, 1, 2], "after the end it loops from frame 1");
    }
}
