//! The command line: `rs2ool cache …` and `rs2ool pack …`.

pub mod cache;
mod convert;
pub mod pack;

#[cfg(feature = "explorer")]
pub use explorer::ExplorerArgs;

#[cfg(feature = "explorer")]
mod explorer {
    use std::path::PathBuf;
    use std::process::ExitCode;

    use rs2ool::explorer::app::{self, Startup, parse_select};
    use rs2ool::explorer::selection::Sel;

    /// How the explorer opens.
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
            });
            ExitCode::SUCCESS
        }
    }
}
