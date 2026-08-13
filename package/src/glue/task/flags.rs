//! Shared cancellation and completion state of one spawned task.

use super::super::common::lock;
use std::sync::{Arc, Mutex};
use std::task::Waker;

/// The flags of one spawned task, shared by its [`JoinHandle`], its
/// [`AbortHandle`]s, and the task itself. This carries no type parameter,
/// so an `AbortHandle` can hold it without knowing the task's output type.
///
/// [`JoinHandle`]: crate::task::JoinHandle
/// [`AbortHandle`]: crate::task::AbortHandle
pub(crate) struct TaskFlags {
  core: Mutex<FlagsCore>,
}

struct FlagsCore {
  /// Set when the task is aborted.
  cancelled: bool,
  /// Set by the task right before it stores its result.
  finished: bool,
  /// Waker of the spawned task, woken by a cancellation
  /// so that the task stops at its next poll.
  waker: Option<Waker>,
}

impl TaskFlags {
  pub fn new() -> Arc<Self> {
    Arc::new(TaskFlags {
      core: Mutex::new(FlagsCore {
        cancelled: false,
        finished: false,
        waker: None,
      }),
    })
  }

  /// Asks the task to stop, waking it so that it notices.
  pub fn cancel(&self) {
    let waker = {
      let mut core = lock(&self.core);
      core.cancelled = true;
      core.waker.take()
    };
    // Wake after releasing the lock,
    // because waking can poll the task again on this thread.
    if let Some(waker) = waker {
      waker.wake();
    }
  }

  /// Returns whether the task was aborted, without registering anything.
  pub fn is_cancelled(&self) -> bool {
    lock(&self.core).cancelled
  }

  /// Returns whether the task was aborted, registering `waker` to be
  /// woken by a later cancellation otherwise. Both happen under one
  /// lock, so an abort from another thread cannot slip in between.
  pub fn cancelled_or_register(&self, waker: &Waker) -> bool {
    let mut core = lock(&self.core);
    if core.cancelled {
      return true;
    }
    // The clone is skipped when the stored waker
    // would already wake this task.
    match &mut core.waker {
      Some(stored) if stored.will_wake(waker) => {}
      stored => *stored = Some(waker.clone()),
    }
    false
  }

  /// Records that the task is done, right before its result is stored.
  pub fn finish(&self) {
    let stale_waker = {
      let mut core = lock(&self.core);
      core.finished = true;
      // A completed task has nothing left to cancel-wake, so the waker
      // is released rather than held for as long as a handle is.
      core.waker.take()
    };
    // Dropped after releasing the lock, like the wakes above.
    drop(stale_waker);
  }

  /// Returns whether the task has finished.
  pub fn is_finished(&self) -> bool {
    lock(&self.core).finished
  }
}

#[cfg(test)]
mod tests {
  use super::super::super::common::test_util::CountingWaker;
  use super::*;
  use wasm_bindgen_test::wasm_bindgen_test;

  #[wasm_bindgen_test]
  fn a_cancellation_wakes_the_registered_task() {
    let flags = TaskFlags::new();
    let counter = CountingWaker::new();
    assert!(!flags.cancelled_or_register(&counter.waker()));
    assert!(!flags.is_cancelled());

    flags.cancel();
    assert_eq!(counter.count(), 1);
    assert!(flags.is_cancelled());
    // Once cancelled, nothing is registered anymore.
    assert!(flags.cancelled_or_register(&counter.waker()));
  }

  #[wasm_bindgen_test]
  fn finishing_flips_only_the_finished_flag() {
    let flags = TaskFlags::new();
    assert!(!flags.is_finished());
    flags.finish();
    assert!(flags.is_finished());
    assert!(!flags.is_cancelled());
  }

  #[wasm_bindgen_test]
  fn finishing_releases_the_registered_waker() {
    let flags = TaskFlags::new();
    let counter = CountingWaker::new();
    assert!(!flags.cancelled_or_register(&counter.waker()));
    flags.finish();
    // Aborting a task that already completed must not wake anything.
    flags.cancel();
    assert_eq!(counter.count(), 0);
  }
}
