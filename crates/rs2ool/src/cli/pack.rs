//! `rs2ool pack`: convert 317 cache records into low-poly client packs,
//! inspect and build packs, or browse them.
//!
//! Exit codes: 0 ok, 1 usage, 2 cache or I/O error, 3 partial failure.

use std::path::PathBuf;
use std::process::ExitCode;

use clap::Subcommand;
use contentpack::PackReader;

use super::convert;

#[derive(Subcommand)]
pub enum Cmd {
    /// Print a pack's manifest and index.
    Inspect { pack: PathBuf },
    /// Build the client palette from every model colour and write the
    /// materials pack (palette and default material) plus palette.png.
    Materials(Out),
    /// Pack the 50 textures into an atlas, with a material per texture.
    Textures(Out),
    /// Convert an NPC: merged model, inferred rig and (with --with-anims)
    /// validated clips.
    Npc {
        /// NPC id.
        #[arg(long)]
        id: u32,
        #[command(flatten)]
        anims: AnimArgs,
        /// Extra sequence ids (317 npc.dat names no combat animations).
        #[arg(long, value_delimiter = ',')]
        seq: Vec<u16>,
        #[command(flatten)]
        out: Out,
    },
    /// Convert every identity-kit part, and rig the default body with the
    /// player stance sequences.
    PlayerKit {
        #[command(flatten)]
        anims: AnimArgs,
        #[command(flatten)]
        out: Out,
    },
    /// Convert media and title sprites, and fonts, into a sprites pack.
    Sprites {
        /// Only this archive (`media` or `title`).
        #[arg(long)]
        archive: Option<String>,
        /// Every sprite (the default when --archive is not given).
        #[arg(long)]
        all: bool,
        #[command(flatten)]
        out: Out,
    },
    /// Convert floor types into floor materials and a string table.
    Floors(Out),
    /// Convert world objects: models, objects, wall kits and strings.
    Objects {
        #[command(flatten)]
        ids: Ids,
        #[command(flatten)]
        out: Out,
    },
    /// Convert models into a models pack.
    Models {
        #[command(flatten)]
        ids: Ids,
        #[command(flatten)]
        out: Out,
    },
    /// Bake a directory of loose records (`pack.json` plus `<id>.bin`) into
    /// a hashed pack.
    Build {
        dir: PathBuf,
        /// Output file (`.pack`), or a directory to write `<hash>.pack` in.
        #[arg(short = 'o', long = "out")]
        out: PathBuf,
    },
    /// Open the explorer on packs, with the cache too when one is given.
    #[cfg(feature = "explorer")]
    Explorer {
        /// Pack files to open.
        #[arg(required = true)]
        packs: Vec<PathBuf>,
        #[command(flatten)]
        view: super::ExplorerArgs,
    },
}

#[derive(clap::Args)]
pub struct AnimArgs {
    /// Also infer the rig's clips and validate them.
    #[arg(long)]
    pub with_anims: bool,
    /// A JSON parent table replacing rig inference's parent choice:
    /// `[[[child labels], [parent labels]], …]`.
    #[arg(long = "override")]
    pub override_file: Option<PathBuf>,
}

/// Which ids to convert.
#[derive(clap::Args, Clone)]
#[group(required = true, multiple = false)]
pub struct Ids {
    /// One id.
    #[arg(long)]
    pub id: Option<u32>,
    /// Every id.
    #[arg(long)]
    pub all: bool,
}

#[derive(clap::Args)]
pub struct Out {
    /// Pack directory; packs go to <dir>/<kind>/<hash>.pack.
    #[arg(short = 'o', long = "out")]
    pub dir: PathBuf,
}

fn inspect(path: &std::path::Path) -> anyhow::Result<()> {
    let r = PackReader::open(path)?;
    println!("hash\t{}", r.hash);
    println!("manifest\t{}", serde_json::to_string(&r.manifest)?);
    println!("id\toffset\tlength\tcrc");
    for e in &r.index {
        println!("{}\t{}\t{}\t{:08x}", e.id, e.offset, e.length, e.crc);
    }
    Ok(())
}

pub fn run(cache: Option<PathBuf>, cmd: Cmd) -> ExitCode {
    let result = match cmd {
        #[cfg(feature = "explorer")]
        Cmd::Explorer { packs, view } => return view.open(cache, packs),
        Cmd::Inspect { pack } => inspect(&pack),
        Cmd::Build { dir, out } => contentpack::loose::bake(&dir, &out)
            .map(|p| println!("{}", p.display()))
            .map_err(Into::into),
        cmd => {
            let Some(dir) = cache else {
                eprintln!("error: no cache directory; pass --cache <dir> or set RS2_CACHE");
                return ExitCode::from(1);
            };
            let cache = match rs2cache::Cache::open(&dir) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::from(2);
                }
            };
            let ctx = convert::Ctx::new(&cache);
            match cmd {
                Cmd::Materials(o) => ctx.and_then(|c| c.materials(&o.dir)),
                Cmd::Textures(o) => ctx.and_then(|c| c.textures(&o.dir)),
                Cmd::Models { ids, out } => ctx.and_then(|c| c.models(&ids, &out.dir)),
                Cmd::Npc {
                    id,
                    anims,
                    seq,
                    out,
                } => ctx.and_then(|c| c.npc(id, &anims, &seq, &out.dir)),
                Cmd::Sprites {
                    archive,
                    all: _,
                    out,
                } => ctx.and_then(|c| c.sprites(archive.as_deref(), &out.dir)),
                Cmd::Objects { ids, out } => ctx.and_then(|c| c.objects(&ids, &out.dir)),
                Cmd::Floors(o) => ctx.and_then(|c| c.floors(&o.dir)),
                Cmd::PlayerKit { anims, out } => ctx.and_then(|c| c.player_kit(&anims, &out.dir)),
                _ => unreachable!(),
            }
        }
    };
    match result {
        Ok(()) if convert::FLAGGED.load(std::sync::atomic::Ordering::Relaxed) => ExitCode::from(3),
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}
