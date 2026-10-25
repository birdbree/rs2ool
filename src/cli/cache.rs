//! `rs2ool cache`: list, dump, decode, describe and verify a 317 cache,
//! draw its models, or browse it.
//!
//! The clap structs here only parse; each converts into its plain
//! counterpart in `rs2ool::cache` (`IdCli` into `IdArgs` and so on), and
//! the library does the rest.
//!
//! Exit codes: 0 ok; 1 usage (including no cache, or a
//! `rs2ool::cache::select::UsageError` from the command); 2 cache or I/O
//! error, meaning any other error; 3 partial failure (some records failed,
//! and they're listed in the manifest or the verify report).

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Args, Subcommand};
use rs2cache::types::record::RecordType;
use rs2cache::{Cache, Records};
use rs2ool::cache::render::{Frames, Motion, RenderArgs, Target};
use rs2ool::cache::{
    DumpArgs, IdArgs, OutArgs, Outcome, SourceArgs, dump, list, render, select, structures, verify,
};
use rs2ool::render::View;

// The `cache` subcommands.
#[derive(Subcommand)]
pub enum Cmd {
    /// Print the indices and archives, or the contents of one.
    List(ListArgs),
    /// Extract records to files, raw or decoded.
    Dump(DumpCli),
    /// Decode records of one type to JSON (shorthand for `dump --type`).
    Decode(DecodeArgs),
    /// Print one record's metadata sidecar.
    Info(InfoArgs),
    /// Check every sector chain and every file; exit 3 on damage.
    Verify,
    /// Draw a record's model with the classic renderer, still or animated.
    Render(RenderCli),
    /// Find whole structures in the map: rooms, doors, walls, roofs and
    /// what's inside.
    Structures(StructuresCli),
    /// Caption candidate sequences with a vision model on a server you run.
    #[cfg(feature = "caption")]
    Caption(CaptionArgs),
    /// Learn sequence roles from reviewed captions and compare with the rules.
    #[cfg(feature = "caption")]
    Classify(ClassifyArgs),
    /// Open the explorer on the cache.
    #[cfg(feature = "explorer")]
    Explorer {
        #[command(flatten)]
        view: super::ExplorerArgs,
        /// Also open a pack (repeatable).
        #[arg(long = "pack", value_name = "FILE")]
        packs: Vec<PathBuf>,
    },
}

// `list` flags; at most one, and none means the overview.
#[derive(Args)]
#[group(multiple = false)]
pub struct ListArgs {
    /// List the ids of an index with their sizes.
    #[arg(long)]
    index: Option<u8>,
    /// List an archive's entries (name or number).
    #[arg(long)]
    archive: Option<String>,
    /// List the ids of a record type, with names where it has them.
    #[arg(long = "type", value_name = "TYPE")]
    kind: Option<RecordType>,
}

/// Which ids to take.
#[derive(Args, Clone, Default)]
#[group(multiple = false)]
pub struct IdCli {
    /// One id.
    #[arg(long)]
    pub id: Option<u32>,
    /// An inclusive id range, `A-B`.
    #[arg(long, value_parser = parse_range)]
    pub range: Option<(u32, u32)>,
    /// Every id.
    #[arg(long)]
    pub all: bool,
}

// `A-B` into `(A, B)`; an empty range is refused here, at parse time.
fn parse_range(s: &str) -> Result<(u32, u32), String> {
    let (a, b) = s.split_once('-').ok_or("expected A-B")?;
    let a = a.trim().parse().map_err(|e| format!("{a}: {e}"))?;
    let b = b.trim().parse().map_err(|e| format!("{b}: {e}"))?;
    if a > b {
        return Err(format!("range {a}-{b} is empty"));
    }
    Ok((a, b))
}

/// Where the records come from: exactly one.
#[derive(Args, Clone)]
#[group(required = true, multiple = false)]
pub struct SourceCli {
    /// An index, 0–4.
    #[arg(long)]
    pub index: Option<u8>,
    /// An archive of index 0, by name or number.
    #[arg(long)]
    pub archive: Option<String>,
    /// A record type.
    #[arg(long = "type", value_name = "TYPE")]
    pub kind: Option<RecordType>,
    /// One map region, `RX_RY`.
    #[arg(long, value_parser = parse_region)]
    pub region: Option<(u8, u8)>,
    /// Every region in map_index.
    #[arg(long)]
    pub all_regions: bool,
    /// A dependency group: `npc:<id>`, `item:<id>` or `player-kit`.
    #[arg(long)]
    pub group: Option<String>,
}

// `caption`'s flags. Each server flag overrides the saved setting for this
// run only.
#[cfg(feature = "caption")]
#[derive(Args)]
pub struct CaptionArgs {
    /// Caption every candidate of one NPC.
    #[arg(long, group = "target", required_unless_present_any = ["check", "import"])]
    npc: Option<u32>,
    /// Caption every candidate sequence of every NPC, each once.
    #[arg(long, group = "target")]
    all: bool,
    /// Ask the server which models it has, then stop.
    #[arg(long, conflicts_with_all = ["dry_run", "target"])]
    check: bool,
    /// Load labels from this JSON file as captions by --model, instead of
    /// captioning: `[{"npc", "seq", "class", "text"}, …]`. Reviews of
    /// earlier imports are kept.
    #[arg(long, value_name = "FILE", requires = "model", conflicts_with_all = ["dry_run", "target", "check"])]
    import: Option<PathBuf>,
    /// Draw the images and print what would be sent, without sending it.
    #[arg(long)]
    dry_run: bool,
    /// Contact the server even if captioning is off in the settings.
    #[arg(long)]
    enable: bool,
    /// The API the server speaks.
    #[arg(long, value_enum)]
    backend: Option<rs2ool::settings::Backend>,
    /// The server's base URL.
    #[arg(long)]
    endpoint: Option<String>,
    /// The model to ask.
    #[arg(long)]
    model: Option<String>,
    /// The environment variable holding an API key.
    #[arg(long, value_name = "VAR")]
    api_key_env: Option<String>,
    /// Seconds to wait for each reply.
    #[arg(long, value_name = "SECS")]
    timeout: Option<u64>,
    /// Frames per image.
    #[arg(long)]
    frames: Option<u32>,
    /// The question, with {frames} and {classes} filled in.
    #[arg(long)]
    prompt: Option<String>,
    /// Caption again what's already captioned.
    #[arg(long)]
    redo: bool,
    /// Stop after this many images.
    #[arg(long, value_name = "N")]
    limit: Option<usize>,
    /// Only these candidate sequences, comma-separated.
    #[arg(long, value_delimiter = ',', value_name = "A,B,…")]
    seq: Option<Vec<u32>>,
    /// Also write each image to this directory.
    #[arg(long, value_name = "DIR")]
    images: Option<PathBuf>,
}

// `classify`'s flags.
#[cfg(feature = "caption")]
#[derive(Args)]
pub struct ClassifyArgs {
    /// Also take labels from this JSON file
    /// (`[{"npc": 1, "seq": 2820, "class": "death"}, …]`); reviews win.
    #[arg(long, value_name = "FILE", env = "RS2_SEQUENCE_LABELS")]
    labels: Option<PathBuf>,
    /// Count this model's unreviewed captions as checked labels
    /// (repeatable); reviews still win.
    #[arg(long, value_name = "MODEL")]
    trust: Vec<String>,
    /// Also learn from unreviewed captions' own classes, each counting
    /// this much (a reviewed label counts 1). They're never scored.
    #[arg(long, value_name = "W")]
    weak: Option<f64>,
    /// How deep the tree may grow.
    #[arg(long, default_value_t = 4)]
    depth: usize,
    /// The least weight a leaf may have.
    #[arg(long, default_value_t = 1.0)]
    min_leaf: f64,
    /// List every labelled sequence with its three roles.
    #[arg(long)]
    verbose: bool,
    /// Save the tree, grown on every label, as role-tree.json in the data
    /// directory, where the explorer's sequence picker uses it for roles.
    #[arg(long)]
    save: bool,
}

// `structures --region RX_RY | --all [-o DIR]`.
#[derive(Args)]
pub struct StructuresCli {
    /// Search this region and its neighbours, keeping the structures whose
    /// south-west corner is in it: `RX_RY`, e.g. `50_50`.
    #[arg(long, value_parser = parse_region, conflicts_with = "all", required_unless_present = "all")]
    region: Option<(u8, u8)>,
    /// Search the whole map.
    #[arg(long)]
    all: bool,
    /// Write `structures/RX_RY.json` per region under this directory
    /// instead of listing them.
    #[arg(short, long, value_name = "DIR")]
    out: Option<PathBuf>,
    /// Also draw each structure's floor plan, with a text sheet beside it,
    /// into `plans/RX_RY/` under `-o`.
    #[arg(long, requires = "out")]
    plans: bool,
    /// Learn what structures and rooms are for, and which belong to one
    /// site, from this labels file, and report how well, held out, instead
    /// of listing structures.
    #[arg(long, value_name = "LABELS")]
    learn: Option<PathBuf>,
    /// With --learn, save the trees to this file.
    #[arg(long, value_name = "FILE", requires = "learn")]
    save: Option<PathBuf>,
    /// Use the trees in this file (from --save) instead of the built-in
    /// ones.
    #[arg(long, value_name = "FILE", conflicts_with = "learn")]
    trees: Option<PathBuf>,
}

// `RX_RY` into `(rx, ry)`; each part has to fit a u8.
fn parse_region(s: &str) -> Result<(u8, u8), String> {
    let (x, y) = s.split_once('_').ok_or("expected RX_RY, e.g. 50_50")?;
    Ok((
        x.parse().map_err(|e| format!("{x}: {e}"))?,
        y.parse().map_err(|e| format!("{y}: {e}"))?,
    ))
}

// `dump`'s flags. Clap catches the simple contradictions (`--raw` with
// `--decode`, `--entry` without `--archive`); `dump::run` catches the rest.
#[derive(Args, Clone)]
pub struct DumpCli {
    #[command(flatten)]
    pub source: SourceCli,
    #[command(flatten)]
    pub ids: IdCli,
    /// With --archive, only this entry (a name, or `#hash`).
    #[arg(long, requires = "archive")]
    pub entry: Option<String>,
    /// Write stored (compressed) bytes instead of inflated ones.
    #[arg(long, conflicts_with_all = ["kind", "group", "decode"])]
    pub raw: bool,
    /// Decode to JSON/PNG instead of writing bytes.
    #[arg(long)]
    pub decode: bool,
    #[command(flatten)]
    pub out: OutCli,
}

// Output flags shared by `dump` and `decode`.
#[derive(Args, Clone)]
pub struct OutCli {
    /// Output directory.
    #[arg(short = 'o', long = "out")]
    pub dir: PathBuf,
    /// Write a <id>.meta.json sidecar beside every output.
    #[arg(long)]
    pub meta: bool,
    /// Write only the sidecars.
    #[arg(long, conflicts_with = "meta")]
    pub meta_only: bool,
    /// Add reverse references to sidecars (cached in <out>/xref.json).
    #[arg(long)]
    pub xref: bool,
    /// Write one <type>.jsonl instead of a file per record.
    #[arg(long)]
    pub jsonl: bool,
}

// `decode TYPE`: `dump --type TYPE` with the type up front.
#[derive(Args, Clone)]
pub struct DecodeArgs {
    /// Record type.
    pub kind: RecordType,
    #[command(flatten)]
    pub ids: IdCli,
    #[command(flatten)]
    pub out: OutCli,
}

// `info --type TYPE --id N`.
#[derive(Args)]
pub struct InfoArgs {
    #[arg(long = "type", value_name = "TYPE")]
    kind: RecordType,
    #[arg(long)]
    id: u32,
}

// `render TYPE --id N -o FILE`, with a sequence and camera.
#[derive(Args)]
pub struct RenderCli {
    /// A record type with a model (model, npc, obj, idk, spotanim, loc), or
    /// `kit` for the default player body.
    #[arg(value_name = "TYPE", value_parser = parse_target)]
    kind: RenderType,
    /// The record's id (not for `kit`).
    #[arg(long)]
    id: Option<u32>,
    /// With `kit`, the default female body.
    #[arg(long)]
    female: bool,
    /// Pose it with this sequence, or with no id, with its own.
    #[arg(long, value_name = "ID")]
    seq: Option<Option<u16>>,
    /// Draw just this frame of the sequence (its index).
    #[arg(long, value_name = "N", requires = "seq", conflicts_with = "sheet")]
    frame: Option<usize>,
    /// Draw a grid of N frames evenly spaced in time (9 with no N).
    #[arg(long, value_name = "N", requires = "seq", num_args = 0..=1, default_missing_value = "9")]
    sheet: Option<usize>,
    /// Draw the converted rig and clip beside the original, and print the
    /// clip's validation.
    #[arg(long, requires = "seq")]
    compare: bool,
    /// Play the sequence over this movement sequence, as the creature acts
    /// while it moves.
    #[arg(long, value_name = "SEQ", requires = "seq")]
    moving: Option<u16>,
    /// Image size, `N` or `WxH` (each tile's with --sheet). Default 512,
    /// or 256 with --sheet.
    #[arg(long, value_parser = parse_size)]
    size: Option<(usize, usize)>,
    /// Turn of the camera about the model, 2048 per turn.
    #[arg(long, default_value_t = 256, allow_negative_numbers = true)]
    yaw: i32,
    /// Tilt of the camera, 2048 per turn; positive looks down.
    #[arg(long, default_value_t = 48, allow_negative_numbers = true)]
    pitch: i32,
    /// Distance from the camera to the model's centre; fitted if not given.
    #[arg(long)]
    distance: Option<i32>,
    /// Background colour, `RRGGBB`.
    #[arg(long, value_name = "RRGGBB", value_parser = parse_rgb)]
    background: Option<u32>,
    /// The file to write: `.gif` for a GIF, anything else for a PNG
    /// (animated when there's more than one frame).
    #[arg(short, long, value_name = "FILE")]
    out: PathBuf,
}

// What `render` draws: a record type, or the player kit.
#[derive(Clone, Copy)]
enum RenderType {
    Record(RecordType),
    Kit,
}

fn parse_target(s: &str) -> Result<RenderType, String> {
    if s.eq_ignore_ascii_case("kit") {
        return Ok(RenderType::Kit);
    }
    s.parse()
        .map(RenderType::Record)
        .map_err(|e| format!("{e}"))
}

// `N` or `WxH`, each 1–4096.
fn parse_size(s: &str) -> Result<(usize, usize), String> {
    let (w, h) = s.split_once(['x', 'X']).unwrap_or((s, s));
    let side = |v: &str| match v.trim().parse::<usize>() {
        Ok(n @ 1..=4096) => Ok(n),
        _ => Err(format!("{v}: expected a size from 1 to 4096")),
    };
    Ok((side(w)?, side(h)?))
}

// `RRGGBB`, with or without a leading `#`.
fn parse_rgb(s: &str) -> Result<u32, String> {
    let hex = s.trim_start_matches('#');
    match u32::from_str_radix(hex, 16) {
        Ok(v) if hex.len() == 6 => Ok(v),
        _ => Err(format!("{s}: expected RRGGBB")),
    }
}

impl RenderCli {
    // The plain options, or a usage error for an id where none belongs.
    fn args(self) -> anyhow::Result<RenderArgs> {
        let target = match (self.kind, self.id) {
            (RenderType::Record(kind), Some(id)) if !self.female => Target::Record(kind, id),
            (RenderType::Kit, None) => Target::Kit {
                female: self.female,
            },
            (RenderType::Record(_), None) => {
                return Err(select::usage("render needs --id for a record type"));
            }
            (RenderType::Record(_), Some(_)) => {
                return Err(select::usage("--female is for kit only"));
            }
            (RenderType::Kit, Some(_)) => return Err(select::usage("kit takes no --id")),
        };
        let motion = match self.seq {
            None => Motion::Still,
            Some(None) => Motion::Own,
            Some(Some(s)) => Motion::Seq(s),
        };
        let frames = match (self.frame, self.sheet) {
            (Some(f), _) => Frames::One(f),
            (None, Some(n)) => Frames::Sheet(n),
            (None, None) => Frames::All,
        };
        let side = if self.sheet.is_some() { 256 } else { 512 };
        let (width, height) = self.size.unwrap_or((side, side));
        let view = View {
            width,
            height,
            yaw: self.yaw,
            pitch: self.pitch,
            distance: self.distance,
            background: self.background.unwrap_or(rs2ool::render::BACKGROUND),
        };
        Ok(RenderArgs {
            target,
            motion,
            frames,
            compare: self.compare,
            moving: self.moving,
            view,
            out: self.out,
        })
    }
}

impl From<IdCli> for IdArgs {
    fn from(a: IdCli) -> Self {
        IdArgs {
            id: a.id,
            range: a.range,
            all: a.all,
        }
    }
}

impl From<SourceCli> for SourceArgs {
    fn from(a: SourceCli) -> Self {
        SourceArgs {
            index: a.index,
            archive: a.archive,
            kind: a.kind,
            region: a.region,
            all_regions: a.all_regions,
            group: a.group,
        }
    }
}

impl From<OutCli> for OutArgs {
    fn from(a: OutCli) -> Self {
        OutArgs {
            dir: a.dir,
            meta: a.meta,
            meta_only: a.meta_only,
            xref: a.xref,
            jsonl: a.jsonl,
            // The CLI always shows progress bars.
            quiet: false,
        }
    }
}

impl From<DumpCli> for DumpArgs {
    fn from(a: DumpCli) -> Self {
        DumpArgs {
            source: a.source.into(),
            ids: a.ids.into(),
            entry: a.entry,
            raw: a.raw,
            decode: a.decode,
            out: a.out.into(),
        }
    }
}

/// Runs one `cache` subcommand and turns the result into an exit code.
pub fn run(cache: Option<PathBuf>, cmd: Cmd) -> ExitCode {
    #[cfg(feature = "explorer")]
    if let Cmd::Explorer { view, packs } = cmd {
        return view.open(cache, packs);
    }
    #[cfg(feature = "caption")]
    let cmd = match cmd {
        Cmd::Caption(a) if a.check => return caption::check(&a),
        cmd => cmd,
    };
    let Some(dir) = cache else {
        eprintln!("error: no cache directory; pass --cache <dir> or set RS2_CACHE");
        return ExitCode::from(1);
    };
    let cache = match Cache::open(&dir) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    let records = Records::new(&cache);
    let result = match cmd {
        Cmd::List(a) => list::run(
            &records,
            a.index,
            a.archive.as_deref(),
            a.kind,
            &mut std::io::stdout().lock(),
        ),
        Cmd::Dump(a) => dump::run(&records, &a.into()),
        Cmd::Decode(a) => {
            let args = DumpArgs {
                source: SourceArgs {
                    kind: Some(a.kind),
                    ..Default::default()
                },
                ids: a.ids.into(),
                entry: None,
                raw: false,
                decode: true,
                out: a.out.into(),
            };
            dump::run(&records, &args)
        }
        Cmd::Info(a) => records
            .meta(a.kind, a.id)
            .map_err(anyhow::Error::from)
            .and_then(|m| {
                writeln!(std::io::stdout(), "{}", serde_json::to_string_pretty(&m)?)?;
                Ok(Outcome::Ok)
            }),
        Cmd::Verify => verify::run(&cache, &mut std::io::stdout().lock()),
        Cmd::Structures(a) => structures::run(
            &cache,
            &records,
            &structures::StructuresArgs {
                region: a.region,
                all: a.all,
                out: a.out,
                plans: a.plans,
                learn: a.learn,
                save: a.save,
                trees: a.trees,
            },
            &mut std::io::stdout().lock(),
        ),
        Cmd::Render(a) => a
            .args()
            .and_then(|args| render::run(&records, &args, &mut std::io::stdout().lock())),
        #[cfg(feature = "caption")]
        Cmd::Caption(a) => caption::run(&cache, &records, &a),
        #[cfg(feature = "caption")]
        Cmd::Classify(a) => caption::classify(&cache, &records, &a),
        // Returned early above.
        #[cfg(feature = "explorer")]
        Cmd::Explorer { .. } => unreachable!(),
    };
    match result {
        Ok(Outcome::Ok) => ExitCode::SUCCESS,
        Ok(Outcome::Partial) => ExitCode::from(3),
        Err(e) if super::broken_pipe(&e) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            // A bad selection is a usage error; anything else is 2.
            let usage = e.downcast_ref::<select::UsageError>().is_some();
            ExitCode::from(if usage { 1 } else { 2 })
        }
    }
}

// `cache caption`: the saved settings with the flags on top, then
// `rs2ool::caption::run` with a progress bar.
#[cfg(feature = "caption")]
mod caption {
    use std::process::ExitCode;
    use std::sync::Mutex;

    use anyhow::Result;
    use indicatif::{ProgressBar, ProgressStyle};
    use rs2cache::{Cache, Records};
    use rs2ool::cache::Outcome;
    use rs2ool::caption::client::{Client, is_local};
    use rs2ool::caption::store::{Store, lock};
    use rs2ool::caption::{self, Event, Options, Target};
    use rs2ool::settings::{self, Caption, Settings};

    use super::{CaptionArgs, ClassifyArgs};

    // The saved caption settings with this run's flags on top.
    fn settings(a: &CaptionArgs) -> Caption {
        let mut c = Settings::load_default().caption;
        c.enabled |= a.enable;
        if let Some(b) = a.backend {
            c.backend = b;
        }
        if let Some(e) = &a.endpoint {
            c.endpoint.clone_from(e);
        }
        if let Some(m) = &a.model {
            c.model.clone_from(m);
        }
        if a.api_key_env.is_some() {
            c.api_key_env.clone_from(&a.api_key_env);
        }
        if a.prompt.is_some() {
            c.prompt.clone_from(&a.prompt);
        }
        c.timeout_secs = a.timeout.unwrap_or(c.timeout_secs);
        c.frames = a.frames.unwrap_or(c.frames);
        c
    }

    // Frames sent anywhere but this machine leave it; say so.
    fn warn_if_remote(c: &Caption) {
        if !is_local(&c.endpoint) {
            eprintln!(
                "warning: {} isn't this machine; images will be sent to it",
                c.endpoint
            );
        }
    }

    // `--check`: list the server's models. Needs no cache.
    pub fn check(a: &CaptionArgs) -> ExitCode {
        let c = settings(a);
        if !c.enabled {
            eprintln!(
                "error: captioning is off: turn it on in the explorer's caption settings, or pass --enable"
            );
            return ExitCode::from(1);
        }
        warn_if_remote(&c);
        // Listing models doesn't need one chosen.
        let probe = Caption {
            model: if c.model.is_empty() {
                "-".into()
            } else {
                c.model.clone()
            },
            ..c.clone()
        };
        match Client::new(&probe).and_then(|cl| cl.models()) {
            Ok(models) => {
                println!(
                    "{} ({}) has {} models:",
                    c.endpoint,
                    c.backend.name(),
                    models.len()
                );
                for m in &models {
                    let mark = if *m == c.model { "  (chosen)" } else { "" };
                    println!("  {m}{mark}");
                }
                if !c.model.is_empty() && !models.contains(&c.model) {
                    eprintln!("warning: the chosen model {} isn't among them", c.model);
                }
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: {e:#}");
                ExitCode::from(2)
            }
        }
    }

    pub fn run(cache: &Cache, records: &Records, a: &CaptionArgs) -> Result<Outcome> {
        if let Some(path) = &a.import {
            return import(cache, path, a.model.as_deref().expect("clap requires it"));
        }
        let c = settings(a);
        let target = match a.npc {
            Some(id) => Target::Npc(id),
            None => Target::All,
        };
        let opts = Options {
            dry_run: a.dry_run,
            redo: a.redo,
            limit: a.limit,
            images: a.images.clone(),
            seqs: a.seq.as_ref().map(|s| s.iter().copied().collect()),
            cancel: None,
        };
        let dir = settings::data_dir()
            .ok_or_else(|| anyhow::anyhow!("this platform has no data directory for captions"))?;
        let store = Mutex::new(Store::open(&dir, rs2convert::source_cache_crc(cache)?)?);
        if a.dry_run {
            match Client::new(&c) {
                Ok(cl) => println!("would ask {} at {}", cl.model(), cl.chat_url()),
                Err(e) => println!("couldn't ask anything yet: {e:#}"),
            }
            println!(
                "with each image, the question:\n{}\n",
                caption::prompt(&c, c.frames as usize)
            );
        } else if c.enabled {
            warn_if_remote(&c);
        }
        let bar = ProgressBar::hidden();
        let outcome = caption::run(records, &c, &store, target, &opts, |e| match e {
            Event::Planned(n) => {
                bar.set_length(n as u64);
                bar.set_style(
                    ProgressStyle::with_template("{bar:30} {pos}/{len} {msg}")
                        .expect("the template is valid"),
                );
                if !a.dry_run {
                    bar.set_draw_target(indicatif::ProgressDrawTarget::stderr());
                }
                if n == 0 {
                    println!("nothing to caption");
                }
            }
            Event::Drawn {
                npc,
                seq,
                size,
                bytes,
            } => {
                println!(
                    "npc {npc} seq {seq}: {}×{} PNG, {bytes} bytes",
                    size.0, size.1
                );
            }
            Event::Captioned(e) => {
                bar.inc(1);
                let class = e.class.as_deref().unwrap_or("?");
                // Printed past the bar rather than through it, so the
                // lines show when the output isn't a terminal too.
                bar.suspend(|| println!("npc {} seq {}: [{class}] {}", e.npc, e.seq, e.text));
            }
            Event::Failed { npc, seq, error } => {
                bar.inc(1);
                bar.suspend(|| eprintln!("npc {npc} seq {seq}: failed: {error:#}"));
            }
        })?;
        bar.finish_and_clear();
        if !a.dry_run {
            println!("captions are in {}", lock(&store).path().display());
        }
        Ok(outcome)
    }

    // `--import`: check every class, then add the labels to the caption
    // file under `model`.
    fn import(cache: &Cache, path: &std::path::Path, model: &str) -> Result<Outcome> {
        use anyhow::Context;
        use rs2ool::caption::CLASSES;
        use rs2ool::caption::store::Imported;

        let text =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let rows: Vec<Imported> =
            serde_json::from_str(&text).with_context(|| format!("reading {}", path.display()))?;
        for r in &rows {
            if let Some(c) = &r.class
                && !CLASSES.iter().any(|(n, _)| n == c)
            {
                return Err(rs2ool::cache::select::usage(format!(
                    "npc {} seq {}: {c:?} isn't a class",
                    r.npc, r.seq
                )));
            }
        }
        let dir = settings::data_dir()
            .ok_or_else(|| anyhow::anyhow!("this platform has no data directory for captions"))?;
        let mut store = Store::open(&dir, rs2convert::source_cache_crc(cache)?)?;
        let n = store.import(model, rows);
        store.save()?;
        println!(
            "imported {n} labels as {model} into {}",
            store.path().display()
        );
        Ok(Outcome::Ok)
    }

    // `classify`: gather the labels, grow and score the tree, print the
    // comparison, and save the tree if asked.
    pub fn classify(cache: &Cache, records: &Records, a: &ClassifyArgs) -> Result<Outcome> {
        use rs2convert::classify::Params;
        use rs2convert::infer::{ROLE_ORDER, Role};
        use rs2ool::caption::classify::{self as learn, Origin};

        let dir = settings::data_dir()
            .ok_or_else(|| anyhow::anyhow!("this platform has no data directory for captions"))?;
        let store = Store::open(&dir, rs2convert::source_cache_crc(cache)?)?;
        let file = match &a.labels {
            Some(p) => learn::read_labels(p)?,
            None => Vec::new(),
        };
        let (labels, overridden) = learn::gather(&store, &file, &a.trust, a.weak.is_some());
        let params = Params {
            max_depth: a.depth,
            min_leaf: a.min_leaf,
        };
        let report = learn::run(records, labels, a.weak.unwrap_or(0.0), params)?;
        let count = |o: Origin| report.judged.iter().filter(|j| j.label.from == o).count();
        let (n, rules, tree) = report.scores();
        if n == 0 {
            println!("no labelled sequences yet: review some captions in the explorer");
            return Ok(Outcome::Ok);
        }
        println!(
            "{n} labelled sequences: {} reviewed captions, {} from the labels file, {} trusted",
            count(Origin::Review),
            count(Origin::Labels),
            count(Origin::Trusted)
        );
        if overridden > 0 {
            println!("  ({overridden} labels-file entries disagree with a review; the review won)");
        }
        if a.weak.is_some() {
            println!(
                "  plus {} unreviewed captions as weak labels, not scored",
                count(Origin::Caption)
            );
        }
        for (l, why) in &report.skipped {
            println!("  skipped npc {} seq {}: {why}", l.npc, l.seq);
        }
        let pct = |k: usize| 100.0 * k as f64 / n as f64;
        println!();
        println!(
            "rules                 {rules:>3}/{n} right ({:.0}%)",
            pct(rules)
        );
        println!(
            "tree, leave-one-out   {tree:>3}/{n} right ({:.0}%)",
            pct(tree)
        );
        let combined = report.combined();
        println!(
            "family trees where there are, else the tree   {combined:>3}/{n} right ({:.0}%)",
            pct(combined)
        );
        let families = report.family_scores();
        if !families.is_empty() {
            println!();
            println!(
                "families with enough labels to try a tree of their own, scored within the family; kept where it beats the tree:"
            );
            println!(
                "  {:<24} {:>5} {:>7} {:>7} {:>7}",
                "family", "n", "rules", "tree", "own"
            );
            for f in &families {
                println!(
                    "  {:<24} {:>5} {:>7} {:>7} {:>7}{}",
                    format!("{} ({})", f.name, f.family),
                    f.n,
                    f.rules,
                    f.global,
                    f.own,
                    if f.kept { "  kept" } else { "" }
                );
            }
        }
        // One confusion table per judge: rows the true role, columns what
        // the judge said.
        let table = |name: &str, said: fn(&learn::Judged) -> Role| {
            println!();
            print!("{name:<8}");
            for r in ROLE_ORDER {
                print!("{:>7}", r.name());
            }
            println!();
            for truth in ROLE_ORDER {
                let row: Vec<usize> = ROLE_ORDER
                    .iter()
                    .map(|&s| {
                        report
                            .checked()
                            .filter(|j| j.truth == truth && said(j) == s)
                            .count()
                    })
                    .collect();
                if row.iter().sum::<usize>() == 0 {
                    continue;
                }
                print!("{:<8}", truth.name());
                for c in row {
                    print!("{:>7}", if c == 0 { ".".into() } else { c.to_string() });
                }
                println!();
            }
        };
        println!();
        println!("rows are the labelled role, columns what each said");
        table("rules", |j| j.rules);
        table("tree", |j| j.tree);
        if a.verbose {
            println!();
            for j in report.checked() {
                println!(
                    "npc {:>4} seq {:<5} {:<10} → {:<7} rules {:<7} tree {:<7}",
                    j.label.npc,
                    j.label.seq,
                    j.label.class,
                    j.truth.name(),
                    j.rules.name(),
                    j.tree.name()
                );
            }
        }
        println!();
        println!("the tree, grown on every label:");
        print!("{}", report.trees.global.describe());
        if a.save {
            let path = settings::role_tree_path()
                .ok_or_else(|| anyhow::anyhow!("this platform has no data directory"))?;
            std::fs::create_dir_all(&dir)?;
            std::fs::write(&path, serde_json::to_vec_pretty(&report.trees)?)?;
            println!(
                "saved the tree and {} family trees to {}",
                report.trees.families.len(),
                path.display()
            );
        }
        Ok(Outcome::Ok)
    }
}
