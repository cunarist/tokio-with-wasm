use super::Id;
use crate::lock;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::task::{Wake, Waker};

/// Records which tasks of a collection have completed, in completion order.
///
/// Each task's join channel holds a waker from [`task_waker`] that, on
/// completion, queues the task's ID here and wakes the consumer. This lets
/// `JoinSet` hand out results in true completion order without polling
/// every stored handle.
///
/// [`task_waker`]: CompletionQueue::task_waker
pub struct CompletionQueue {
  core: Arc<Mutex<QueueCore>>,
}

struct QueueCore {
  ready: VecDeque<Id>,
  /// Waker of the consumer awaiting the next completion.
  consumer: Option<Waker>,
}

impl CompletionQueue {
  pub fn new() -> Self {
    Self {
      core: Arc::new(Mutex::new(QueueCore {
        ready: VecDeque::new(),
        consumer: None,
      })),
    }
  }

  pub fn pop(&self) -> Option<Id> {
    lock(&self.core).ready.pop_front()
  }

  /// Pops the earliest completion, or registers the consumer waker if
  /// there is none yet. Both happen under one lock, so a completion
  /// arriving from a web worker in between cannot be missed.
  pub fn pop_or_register(&self, waker: &Waker) -> Option<Id> {
    let mut core = lock(&self.core);
    let popped = core.ready.pop_front();
    if popped.is_none() {
      core.consumer = Some(waker.clone());
    }
    popped
  }

  /// Creates the waker to register in one task's join channel.
  pub fn task_waker(&self, id: Id) -> Waker {
    Waker::from(Arc::new(TaskWaker {
      id,
      core: self.core.clone(),
    }))
  }
}

struct TaskWaker {
  id: Id,
  core: Arc<Mutex<QueueCore>>,
}

impl Wake for TaskWaker {
  fn wake(self: Arc<Self>) {
    let consumer = {
      let mut core = lock(&self.core);
      core.ready.push_back(self.id);
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
  use super::*;
  use crate::test_util::CountingWaker;
  use wasm_bindgen_test::wasm_bindgen_test;

  #[wasm_bindgen_test]
  fn ids_come_out_in_completion_order() {
    let queue = CompletionQueue::new();
    let (first, second) = (Id::next(), Id::next());
    queue.task_waker(second).wake();
    queue.task_waker(first).wake();
    assert_eq!(queue.pop(), Some(second));
    assert_eq!(queue.pop(), Some(first));
    assert_eq!(queue.pop(), None);
  }

  #[wasm_bindgen_test]
  fn a_completion_wakes_the_registered_consumer() {
    let queue = CompletionQueue::new();
    let counter = CountingWaker::new();
    let waker = counter.waker();
    assert_eq!(queue.pop_or_register(&waker), None);
    let id = Id::next();
    queue.task_waker(id).wake();
    assert_eq!(counter.count(), 1);
    // The pop finds the ID, so nothing is registered for the next one.
    assert_eq!(queue.pop_or_register(&waker), Some(id));
    queue.task_waker(Id::next()).wake();
    assert_eq!(counter.count(), 1);
  }
}
