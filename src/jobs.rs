//! Shared worker supervision and research checkpoint policy for CLI and TUI.
use crate::app::{JobEvent, JobSender};
use futures_util::FutureExt;
use std::{
    future::Future,
    panic::AssertUnwindSafe,
    time::{Duration, Instant},
};

pub const CHECKPOINT_INTERVAL: Duration = Duration::from_secs(1);
pub const CHECKPOINT_BYTES: usize = 64 * 1024;

pub struct Checkpoint {
    dirty: bool,
    bytes: usize,
    saved_at: Instant,
}
impl Default for Checkpoint {
    fn default() -> Self {
        Self {
            dirty: false,
            bytes: 0,
            saved_at: Instant::now(),
        }
    }
}
impl Checkpoint {
    pub fn changed(&mut self, bytes: usize) {
        self.dirty = true;
        self.bytes = self.bytes.saturating_add(bytes);
    }
    pub fn dirty(&self) -> bool {
        self.dirty
    }
    pub fn due(&self) -> bool {
        self.dirty
            && (self.bytes >= CHECKPOINT_BYTES || self.saved_at.elapsed() >= CHECKPOINT_INTERVAL)
    }
    pub fn saved(&mut self) {
        self.dirty = false;
        self.bytes = 0;
        self.saved_at = Instant::now();
    }
}

/// A worker must emit its normal terminal event before returning. A trailing stop
/// event detects panics and accidental returns, without persisting panic payloads.
pub async fn supervise(task: impl Future<Output = ()>, tx: JobSender) {
    let _ = AssertUnwindSafe(task).catch_unwind().await;
    let _ = tx.send(JobEvent::Stopped);
}
