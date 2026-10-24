//! Background jobs: the Actions menu's dumps and conversions.
//!
//! Each one runs on the `AsyncComputeTaskPool` and bumps an atomic counter
//! the bottom panel reads for its progress bar. Finished jobs stay in
//! [`Jobs::list`] so their results stay on screen.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};

/// Handed to a job so it can report progress. Cheap to clone into worker
/// threads.
#[derive(Clone)]
pub struct ProgressHandle(Arc<AtomicU32>);

impl ProgressHandle {
    /// One more item done.
    pub fn inc(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
    /// Sets the count of items done outright.
    pub fn set(&self, n: u32) {
        self.0.store(n, Ordering::Relaxed);
    }
}

/// What a finished job reports: a message, and outputs the user can open
/// as sources.
#[derive(Debug, Clone, Default)]
pub struct Done {
    /// One line or a few, shown under the job.
    pub message: String,
    /// What the job wrote, each with the source kind to open it as; each
    /// gets a button saying what it opens (see [`open_label`]).
    pub opens: Vec<(crate::explorer::sources::Kind, std::path::PathBuf)>,
}

/// The button text for opening `path` as `kind`: "Open models pack" for a
/// pack (named by the kind directory it was written into), "Open dump
/// folder" or "Open cache".
pub fn open_label(kind: crate::explorer::sources::Kind, path: &std::path::Path) -> String {
    use crate::explorer::sources::Kind;
    match kind {
        Kind::Pack => match path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .filter(|n| contentpack::Kind::ALL.iter().any(|k| k.name() == *n))
        {
            Some(kind) => format!("Open {kind} pack"),
            None => "Open pack".to_owned(),
        },
        Kind::DumpDir => "Open dump folder".to_owned(),
        Kind::Cache => "Open cache".to_owned(),
    }
}

impl From<String> for Done {
    fn from(message: String) -> Self {
        Done {
            message,
            opens: Vec::new(),
        }
    }
}

/// How a job ended: its report, or the error with its context chain.
pub type JobResult = Result<Done, String>;

/// One job, running or finished.
pub struct Job {
    /// What the bottom panel calls it.
    pub name: String,
    /// Items done so far; the job's [`ProgressHandle`] writes it.
    pub progress: Arc<AtomicU32>,
    /// Items in all; 0 when the job can't tell up front.
    pub total: u32,
    /// The result, once it's finished.
    pub done: Option<JobResult>,
    // The running task; `None` once it's finished.
    task: Option<Task<JobResult>>,
}

impl Job {
    /// How far along it is, 0.0 to 1.0. With no known total it jumps
    /// straight from 0 to 1 when the job finishes.
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            return if self.done.is_some() { 1.0 } else { 0.0 };
        }
        (self.progress.load(Ordering::Relaxed) as f32 / self.total as f32).min(1.0)
    }
}

/// Every job this session has started, oldest first.
#[derive(Resource, Default)]
pub struct Jobs {
    /// The jobs, running and finished.
    pub list: Vec<Job>,
}

impl Jobs {
    /// Run `f` on the compute pool. `total` is the progress denominator (0
    /// when unknown).
    pub fn spawn_job(
        &mut self,
        name: impl Into<String>,
        total: u32,
        f: impl FnOnce(ProgressHandle) -> anyhow::Result<Done> + Send + 'static,
    ) {
        let progress = Arc::new(AtomicU32::new(0));
        let handle = ProgressHandle(progress.clone());
        let task = AsyncComputeTaskPool::get()
            .spawn(async move { f(handle).map_err(|e| format!("{e:#}")) });
        self.list.push(Job {
            name: name.into(),
            progress,
            total,
            done: None,
            task: Some(task),
        });
    }

    /// How many haven't finished yet.
    pub fn running(&self) -> usize {
        self.list.iter().filter(|j| j.done.is_none()).count()
    }
}

/// Collects finished jobs' results without blocking.
fn poll(mut jobs: ResMut<Jobs>) {
    for job in &mut jobs.list {
        if let Some(task) = &mut job.task
            && let Some(r) = block_on(poll_once(task))
        {
            job.done = Some(r);
            job.task = None;
        }
    }
}

/// Adds [`Jobs`] and the system that polls it.
pub fn plugin(app: &mut App) {
    app.init_resource::<Jobs>().add_systems(Update, poll);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_buttons_say_what_they_open() {
        use crate::explorer::sources::Kind;
        use std::path::Path;
        let label = |k, p: &str| open_label(k, Path::new(p));
        assert_eq!(
            label(Kind::Pack, "out/models/ab12.pack"),
            "Open models pack"
        );
        assert_eq!(label(Kind::Pack, "out/anims/ab12.pack"), "Open anims pack");
        assert_eq!(label(Kind::Pack, "elsewhere/x.pack"), "Open pack");
        assert_eq!(label(Kind::DumpDir, "out"), "Open dump folder");
    }
}
