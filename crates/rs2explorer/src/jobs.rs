//! Background jobs: every long action runs on the `AsyncComputeTaskPool`
//! and reports progress through atomics the bottom bar reads.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task, block_on, poll_once};

/// Handed to a job so it can report progress.
#[derive(Clone)]
pub struct ProgressHandle(Arc<AtomicU32>);

impl ProgressHandle {
    pub fn inc(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
    pub fn set(&self, n: u32) {
        self.0.store(n, Ordering::Relaxed);
    }
}

/// How a job ended: a message for the user, or an error.
pub type JobResult = Result<String, String>;

pub struct Job {
    pub name: String,
    pub progress: Arc<AtomicU32>,
    pub total: u32,
    pub done: Option<JobResult>,
    task: Option<Task<JobResult>>,
}

impl Job {
    pub fn fraction(&self) -> f32 {
        if self.total == 0 {
            return if self.done.is_some() { 1.0 } else { 0.0 };
        }
        (self.progress.load(Ordering::Relaxed) as f32 / self.total as f32).min(1.0)
    }
}

#[derive(Resource, Default)]
pub struct Jobs {
    pub list: Vec<Job>,
}

impl Jobs {
    /// Run `f` on the compute pool. `total` is the progress denominator (0
    /// when unknown).
    pub fn spawn_job(
        &mut self,
        name: impl Into<String>,
        total: u32,
        f: impl FnOnce(ProgressHandle) -> anyhow::Result<String> + Send + 'static,
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

    pub fn running(&self) -> usize {
        self.list.iter().filter(|j| j.done.is_none()).count()
    }
}

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

pub fn plugin(app: &mut App) {
    app.init_resource::<Jobs>().add_systems(Update, poll);
}
