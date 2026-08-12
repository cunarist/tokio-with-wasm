use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Wake, Waker};

/// Records which tasks of a collection have completed, in completion order.
///
/// Each spawned task gets a waker made by [`task_waker`] registered in its
/// join channel. When the task completes, that waker pushes the task's tag
/// here and wakes the consumer. This lets `JoinSet` and `JoinMap` hand out
/// results in true completion order and learn about completions without
/// polling every stored handle.
///
/// [`task_waker`]: CompletionQueue::task_waker
pub struct CompletionQueue<I> {
  core: Arc<Mutex<QueueCore<I>>>,
}

struct QueueCore<I> {
  /// Tags of completed tasks, in the order they completed.
  ready: VecDeque<I>,
  /// Waker of the consumer awaiting the next completion.
  consumer: Option<Waker>,
}

/// Locks the shared state, recovering it
/// even if another thread panicked while holding the lock.
fn lock<I>(core: &Mutex<QueueCore<I>>) -> MutexGuard<'_, QueueCore<I>> {
  core.lock().unwrap_or_else(|error| error.into_inner())
}

impl<I> CompletionQueue<I> {
  pub fn new() -> Self {
    Self {
      core: Arc::new(Mutex::new(QueueCore {
        ready: VecDeque::new(),
        consumer: None,
      })),
    }
  }

  /// Pops the tag of the earliest completed task, if there is one.
  pub fn pop(&self) -> Option<I> {
    lock(&self.core).ready.pop_front()
  }

  /// Pops the earliest completion, or registers the consumer waker if
  /// there is none yet. Both happen under one lock, so a completion
  /// arriving from a web worker in between cannot be missed.
  pub fn pop_or_register(&self, waker: &Waker) -> Option<I> {
    let mut core = lock(&self.core);
    let popped = core.ready.pop_front();
    if popped.is_none() {
      core.consumer = Some(waker.clone());
    }
    popped
  }
}

impl<I> CompletionQueue<I>
where
  I: Copy + Send + Sync + 'static,
{
  /// Creates the waker to register in one task's join channel.
  /// When the task completes, `tag` is queued and the consumer is woken.
  pub fn task_waker(&self, tag: I) -> Waker {
    Waker::from(Arc::new(TaskWaker {
      tag,
      core: self.core.clone(),
    }))
  }
}

struct TaskWaker<I> {
  tag: I,
  core: Arc<Mutex<QueueCore<I>>>,
}

impl<I> Wake for TaskWaker<I>
where
  I: Copy + Send + Sync + 'static,
{
  fn wake(self: Arc<Self>) {
    self.wake_by_ref();
  }

  fn wake_by_ref(self: &Arc<Self>) {
    let consumer = {
      let mut core = lock(&self.core);
      core.ready.push_back(self.tag);
      core.consumer.take()
    };
    // Wake after releasing the lock,
    // because waking can poll the consumer again on this thread.
    if let Some(waker) = consumer {
      waker.wake();
    }
  }
}

#[cfg(test)]
mod tests {
  use super::super::test_util::CountingWaker;
  use super::*;
  use wasm_bindgen_test::wasm_bindgen_test;

  #[wasm_bindgen_test]
  fn tags_come_out_in_completion_order() {
    let queue: CompletionQueue<u64> = CompletionQueue::new();
    queue.task_waker(1).wake();
    queue.task_waker(2).wake();
    assert_eq!(queue.pop(), Some(1));
    assert_eq!(queue.pop(), Some(2));
    assert_eq!(queue.pop(), None);
  }

  #[wasm_bindgen_test]
  fn a_completion_wakes_the_registered_consumer() {
    let queue: CompletionQueue<u64> = CompletionQueue::new();
    let counter = CountingWaker::new();
    let waker = counter.waker();
    assert_eq!(queue.pop_or_register(&waker), None);

    queue.task_waker(7).wake();
    assert_eq!(counter.count(), 1);
    assert_eq!(queue.pop_or_register(&waker), Some(7));

    // The pop above found a tag, so no waker was registered:
    // this completion has nobody to wake.
    queue.task_waker(8).wake();
    assert_eq!(counter.count(), 1);
  }

  #[wasm_bindgen_test]
  fn a_queued_completion_is_popped_instead_of_registering() {
    let queue: CompletionQueue<u64> = CompletionQueue::new();
    queue.task_waker(1).wake();
    let counter = CountingWaker::new();
    let waker = counter.waker();
    assert_eq!(queue.pop_or_register(&waker), Some(1));
    assert_eq!(counter.count(), 0);
  }
}
