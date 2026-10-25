//! The command line: `rs2ool cache …` and `rs2ool pack …`.
//!
//! - `cache`: the `cache` subcommands, mapped onto `rs2ool::cache`.
//! - `pack`: the `pack` subcommands; `inspect` and `build` are here.
//! - `convert`: the converting `pack` subcommands, over `rs2convert::run`.
//! - `ExplorerArgs`: the startup flags both `explorer` subcommands share.
//!
//! Doc comments on the clap structs double as the `--help` text, so
//! anything meant only for readers of the code is a `//` comment.

pub mod cache;
mod convert;
pub mod pack;

/// Whether `e` comes from writing to a stdout nobody's reading any more,
/// as in `rs2ool cache list | head`. That's not a failure: the reader got
/// what it wanted, so the command stops quietly and exits 0, like any other
/// Unix tool. Rust ignores SIGPIPE, so without this the write would just
/// error (or, through `println!`, panic).
pub fn broken_pipe(e: &anyhow::Error) -> bool {
    e.chain().any(|c| {
        c.downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::BrokenPipe)
    })
}

#[cfg(feature = "explorer")]
pub use explorer::ExplorerArgs;

#[cfg(feature = "explorer")]
mod explorer {
    use std::path::PathBuf;
    use std::process::ExitCode;

    use rs2ool::explorer::app::{self, CameraView, Startup, parse_camera, parse_select};
    use rs2ool::explorer::selection::Sel;

    /// How the explorer opens.
    //
    // Shared by `cache explorer` and `pack explorer`; neither needs a
    // cache to open the window.
    #[derive(clap::Args)]
    pub struct ExplorerArgs {
        /// Select a record in the cache at startup: `TYPE:ID`, or `kit` for
        /// the player kit.
        #[arg(long, value_parser = parse_select)]
        select: Option<Sel>,
        /// Show the selection's other version (original or converted)
        /// beside it.
        #[arg(long, requires = "select")]
        compare: bool,
        /// Start in the classic renderer.
        #[arg(long)]
        classic: bool,
        /// Once everything has loaded, point the camera: `YAW,PITCH,DISTANCE`
        /// (degrees, degrees, world units), optionally followed by `,X,Y,Z`,
        /// the point it looks at in world units.
        #[arg(long, value_parser = parse_camera, allow_hyphen_values = true)]
        camera: Option<CameraView>,
        /// Once everything has loaded (and the camera has moved), save a
        /// screenshot of the window to this PNG and close.
        #[arg(long, value_name = "FILE")]
        screenshot: Option<PathBuf>,
    }

    impl ExplorerArgs {
        /// Open the explorer on `cache` and `packs`; returns when the
        /// window closes.
        pub fn open(self, cache: Option<PathBuf>, packs: Vec<PathBuf>) -> ExitCode {
            app::run(Startup {
                cache,
                packs,
                select: self.select,
                compare: self.compare,
                classic: self.classic,
                camera: self.camera,
                screenshot: self.screenshot,
            });
            ExitCode::SUCCESS
        }
    }
}
