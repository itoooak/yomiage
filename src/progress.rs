use std::{
    collections::HashMap,
    sync::{Mutex, MutexGuard},
    time::Instant,
};

use serde::Serialize;

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Status {
    Waiting,
    Checking,
    Synthesizing,
    Saving,
    Completed,
    Failed,
}

#[derive(Clone, Serialize)]
pub(crate) struct TargetProgress {
    status: Status,
    completed_lines: usize,
    total_lines: usize,
    elapsed_seconds: Option<u64>,
    #[serde(skip)]
    started_at: Option<Instant>,
}

impl Default for TargetProgress {
    fn default() -> Self {
        Self {
            status: Status::Waiting,
            completed_lines: 0,
            total_lines: 0,
            elapsed_seconds: None,
            started_at: None,
        }
    }
}

pub(super) struct Progress {
    targets: HashMap<String, Mutex<TargetProgress>>,
}

impl Progress {
    pub(super) fn new(ids: impl Iterator<Item = String>) -> Self {
        Self {
            targets: ids
                .map(|id| (id, Mutex::new(TargetProgress::default())))
                .collect(),
        }
    }

    pub(super) fn get(&self, id: &str) -> TargetProgress {
        let mut state = self.target(id).clone();
        if let Some(started_at) = state.started_at {
            state.elapsed_seconds = Some(started_at.elapsed().as_secs());
        }
        state
    }

    pub(super) fn start(&self, id: &str) -> TargetProgress {
        let mut state = self.target(id);
        std::mem::replace(
            &mut *state,
            TargetProgress {
                status: Status::Checking,
                started_at: Some(Instant::now()),
                ..TargetProgress::default()
            },
        )
    }

    pub(super) fn synthesizing(&self, id: &str, total_lines: usize) {
        let mut state = self.target(id);
        state.status = Status::Synthesizing;
        state.total_lines = total_lines;
    }

    pub(super) fn line_completed(&self, id: &str) {
        self.target(id).completed_lines += 1;
    }

    pub(super) fn saving(&self, id: &str) {
        self.target(id).status = Status::Saving;
    }

    pub(super) fn completed(&self, id: &str) {
        self.finish(id, Status::Completed);
    }

    pub(super) fn failed(&self, id: &str) {
        self.finish(id, Status::Failed);
    }

    pub(super) fn unchanged(&self, id: &str, previous: TargetProgress) {
        let state = if previous.status == Status::Completed {
            previous
        } else {
            TargetProgress {
                status: Status::Completed,
                ..TargetProgress::default()
            }
        };
        *self.target(id) = state;
    }

    fn finish(&self, id: &str, status: Status) {
        let mut state = self.target(id);
        state.status = status;
        state.elapsed_seconds = state
            .started_at
            .take()
            .map(|start| start.elapsed().as_secs());
    }

    fn target(&self, id: &str) -> MutexGuard<'_, TargetProgress> {
        self.targets
            .get(id)
            .expect("configured target")
            .lock()
            .unwrap()
    }
}
