//! Open sources: caches, dump directories and packs. Opening (dialog and
//! all) runs as a task; finished sources land in [`Sources`].

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};
use rs2cache::{Cache, Records};
use rs2pack::PackReader;

pub enum Source {
    /// A cache, with its memoising record handle. The cache is leaked for
    /// a `'static` lifetime; a session opens few.
    Cache {
        path: PathBuf,
        records: Arc<Records<'static>>,
    },
    /// A directory written by `cachedump`.
    DumpDir { path: PathBuf },
    /// One converted pack.
    Pack {
        path: PathBuf,
        reader: Arc<PackReader>,
    },
}

impl Source {
    pub fn label(&self) -> String {
        let name = |p: &Path| {
            p.file_name().map_or_else(
                || p.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            )
        };
        match self {
            Source::Cache { path, .. } => format!("cache {}", name(path)),
            Source::DumpDir { path } => format!("dump {}", name(path)),
            Source::Pack { path, reader } => {
                format!("{} pack {}", reader.manifest.kind, name(path))
            }
        }
    }
}

/// A cheap copy of a source (its handles are shared), for handing to a
/// task.
pub fn detach(source: &Source) -> Source {
    match source {
        Source::Cache { path, records } => Source::Cache {
            path: path.clone(),
            records: records.clone(),
        },
        Source::DumpDir { path } => Source::DumpDir { path: path.clone() },
        Source::Pack { path, reader } => Source::Pack {
            path: path.clone(),
            reader: reader.clone(),
        },
    }
}

/// Open a cache directory.
pub fn open_cache(path: &Path) -> anyhow::Result<Source> {
    let cache: &'static Cache = Box::leak(Box::new(Cache::open(path)?));
    Ok(Source::Cache {
        path: path.to_path_buf(),
        records: Arc::new(Records::new(cache)),
    })
}

/// Open a cachedump output directory.
pub fn open_dump(path: &Path) -> anyhow::Result<Source> {
    anyhow::ensure!(
        path.join("manifest.json").exists(),
        "{} has no manifest.json",
        path.display()
    );
    Ok(Source::DumpDir {
        path: path.to_path_buf(),
    })
}

/// Open a pack file.
pub fn open_pack(path: &Path) -> anyhow::Result<Source> {
    Ok(Source::Pack {
        path: path.to_path_buf(),
        reader: Arc::new(PackReader::open(path)?),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Cache,
    DumpDir,
    Pack,
}

type Opening = Task<Option<Result<Source, String>>>;

#[derive(Resource, Default)]
pub struct Sources {
    pub list: Vec<Source>,
    pub errors: Vec<String>,
    pending: Vec<Opening>,
}

impl Sources {
    /// Show a picker for `kind` and open what is chosen, off the UI thread.
    pub fn pick(&mut self, kind: Kind) {
        let task = AsyncComputeTaskPool::get().spawn(async move {
            let dialog = rfd::AsyncFileDialog::new();
            let handle = match kind {
                Kind::Cache | Kind::DumpDir => dialog.pick_folder().await,
                Kind::Pack => dialog.add_filter("pack", &["pack"]).pick_file().await,
            }?;
            let path = handle.path().to_path_buf();
            Some(open(kind, &path).map_err(|e| format!("{}: {e:#}", path.display())))
        });
        self.pending.push(task);
    }

    /// Open `path` as `kind` off the UI thread (used by tests and by
    /// "Open as source").
    pub fn open_path(&mut self, kind: Kind, path: PathBuf) {
        let task = AsyncComputeTaskPool::get().spawn(async move {
            Some(open(kind, &path).map_err(|e| format!("{}: {e:#}", path.display())))
        });
        self.pending.push(task);
    }

    pub fn pending(&self) -> usize {
        self.pending.len()
    }
}

fn open(kind: Kind, path: &Path) -> anyhow::Result<Source> {
    match kind {
        Kind::Cache => open_cache(path),
        Kind::DumpDir => open_dump(path),
        Kind::Pack => open_pack(path),
    }
}

fn poll(mut sources: ResMut<Sources>) {
    let mut i = 0;
    while i < sources.pending.len() {
        match block_on(poll_once(&mut sources.pending[i])) {
            Some(result) => {
                drop(sources.pending.swap_remove(i));
                match result {
                    Some(Ok(s)) => sources.list.push(s),
                    Some(Err(e)) => sources.errors.push(e),
                    None => {}
                }
            }
            None => i += 1,
        }
    }
}

pub fn plugin(app: &mut App) {
    app.init_resource::<Sources>().add_systems(Update, poll);
}
