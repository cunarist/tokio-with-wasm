//! A collection of tasks spawned in JavaScript runtime.
//!
//! This module provides the [`JoinSet`] type, a collection which stores a set
//! of spawned tasks and allows asynchronously awaiting the output of those
//! tasks as they complete. See the documentation for the [`JoinSet`] type for
//! details.
use crate::{
  AbortHandle, CompletionQueue, JoinError, JoinHandle, spawn, spawn_blocking,
};
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A collection of tasks spawned in JavaScript.
///
/// A `JoinSet` can be used to await the completion of some or all of the tasks
/// in the set. The set is not ordered, and the tasks will be returned in the
/// order they complete.
///
/// All of the tasks must have the same return type `T`.
///
/// When the `JoinSet` is dropped, all tasks in the `JoinSet` are immediately aborted.
///
/// # Examples
///
/// Spawn multiple tasks and wait for them.
///
/// ```no_run
/// use tokio_with_wasm::alias as tokio;
/// use tokio::task::JoinSet;
///
/// #[tokio::main]
/// async fn main() {
///     let mut set = JoinSet::new();
///
///     for i in 0..10 {
///         set.spawn(async move { i });
///     }
///
///     let mut seen = [false; 10];
///     while let Some(res) = set.join_next().await {
///         let idx = res.unwrap();
///         seen[idx] = true;
///     }
///
///     for i in 0..10 {
///         assert!(seen[i]);
///     }
/// }
/// ```
pub struct JoinSet<T> {
  /// Tasks that have not been joined yet, by task tag.
  tasks: HashMap<u64, JoinHandle<T>>,
  /// Completed task tags, in the order they completed.
  queue: CompletionQueue<u64>,
  /// The tag for the next spawned task.
  next_tag: u64,
}

impl<T> JoinSet<T> {
  /// Create a new `JoinSet`.
  pub fn new() -> Self {
    Self {
      tasks: HashMap::new(),
      queue: CompletionQueue::new(),
      next_tag: 0,
    }
  }

  /// Returns the number of tasks currently in the `JoinSet`.
  pub fn len(&self) -> usize {
    self.tasks.len()
  }

  /// Returns whether the `JoinSet` is empty.
  pub fn is_empty(&self) -> bool {
    self.tasks.is_empty()
  }

  /// Aborts all tasks on this `JoinSet`.
  ///
  /// This does not remove the tasks from the `JoinSet`. To wait for the tasks to complete
  /// cancellation, you should call `join_next` in a loop until the `JoinSet` is empty.
  pub fn abort_all(&mut self) {
    self.tasks.values().for_each(|jh| jh.abort());
  }

  /// Removes all tasks from this `JoinSet` without aborting them.
  ///
  /// The tasks removed by this call will continue to run in the background even if the `JoinSet`
  /// is dropped.
  pub fn detach_all(&mut self) {
    self.tasks.clear();
  }

  /// Stores a spawned task's handle and hooks its completion
  /// into the completion queue.
  fn store(&mut self, join_handle: JoinHandle<T>) {
    let tag = self.next_tag;
    self.next_tag += 1;
    join_handle.register_waker(self.queue.task_waker(tag));
    self.tasks.insert(tag, join_handle);
  }
}

impl<T: 'static> JoinSet<T> {
  /// Spawn the provided task on the `JoinSet`, returning an [`AbortHandle`]
  /// that can be used to remotely cancel the task.
  ///
  /// The provided future will start running in the background immediately
  /// when this method is called, even if you don't await anything on this
  /// `JoinSet`.
  ///
  /// [`AbortHandle`]: crate::task::AbortHandle
  #[track_caller]
  pub fn spawn<F>(&mut self, task: F) -> AbortHandle
  where
    F: Future<Output = T>,
    F: 'static,
  {
    let join_handle = spawn(task);
    let abort_handle = join_handle.abort_handle();
    self.store(join_handle);
    abort_handle
  }

  /// Spawn the blocking code on the blocking threadpool and store
  /// it in this `JoinSet`, returning an [`AbortHandle`] that can be
  /// used to remotely cancel the task.
  ///
  /// # Examples
  ///
  /// Spawn multiple blocking tasks and wait for them.
  ///
  /// ```no_run
  /// use tokio_with_wasm::alias as tokio;
  /// use tokio::task::JoinSet;
  ///
  /// #[tokio::main]
  /// async fn main() {
  ///     let mut set = JoinSet::new();
  ///
  ///     for i in 0..10 {
  ///         set.spawn_blocking(move || { i });
  ///     }
  ///
  ///     let mut seen = [false; 10];
  ///     while let Some(res) = set.join_next().await {
  ///         let idx = res.unwrap();
  ///         seen[idx] = true;
  ///     }
  ///
  ///     for i in 0..10 {
  ///         assert!(seen[i]);
  ///     }
  /// }
  /// ```
  ///
  /// [`AbortHandle`]: crate::task::AbortHandle
  #[track_caller]
  pub fn spawn_blocking<F>(&mut self, f: F) -> AbortHandle
  where
    F: FnOnce() -> T,
    F: Send + 'static,
    T: Send,
  {
    let join_handle = spawn_blocking(f);
    let abort_handle = join_handle.abort_handle();
    self.store(join_handle);
    abort_handle
  }

  /// Waits until one of the tasks in the set completes and returns its output.
  ///
  /// Returns `None` if the set is empty.
  ///
  /// # Cancel Safety
  ///
  /// This method is cancel safe. If `join_next` is used as the event in a `tokio::select!`
  /// statement and some other branch completes first, it is guaranteed that no tasks were
  /// removed from this `JoinSet`.
  pub async fn join_next(&mut self) -> Option<Result<T, JoinError>> {
    std::future::poll_fn(|cx| self.poll_join_next(cx)).await
  }

  /// Tries to join one of the tasks in the set that has completed and return its output.
  ///
  /// Returns `None` if there are no completed tasks, or if the set is empty.
  pub fn try_join_next(&mut self) -> Option<Result<T, JoinError>> {
    while let Some(tag) = self.queue.pop() {
      // A tag can be stale if its task was detached earlier.
      let Some(mut handle) = self.tasks.remove(&tag) else {
        continue;
      };
      // The task queued its tag on completion, so its result is stored;
      // this poll with a no-op waker just takes the result out.
      let mut cx = Context::from_waker(std::task::Waker::noop());
      if let Poll::Ready(result) = Pin::new(&mut handle).poll(&mut cx) {
        return Some(result);
      }
    }
    None
  }

  /// Aborts all tasks and waits for them to finish shutting down.
  ///
  /// Calling this method is equivalent to calling [`abort_all`] and then calling [`join_next`] in
  /// a loop until it returns `None`.
  ///
  /// This method ignores any panics in the tasks shutting down. When this call returns, the
  /// `JoinSet` will be empty.
  ///
  /// [`abort_all`]: fn@Self::abort_all
  /// [`join_next`]: fn@Self::join_next
  pub async fn shutdown(&mut self) {
    self.abort_all();
    while self.join_next().await.is_some() {}
  }

  /// Awaits the completion of all tasks in this `JoinSet`, returning a vector of their results.
  ///
  /// The results will be stored in the order they completed not the order they were spawned.
  /// This is a convenience method that is equivalent to calling [`join_next`] in
  /// a loop. Tasks that fail with a [`JoinError`] are left out of the returned
  /// vector, unlike in `tokio`, where `join_all` panics instead. To see those
  /// errors, call [`join_next`] in a loop.
  ///
  /// # Examples
  ///
  /// Spawn multiple tasks and `join_all` them.
  ///
  /// ```no_run
  /// use tokio_with_wasm::alias as tokio;
  /// use tokio::task::JoinSet;
  /// use std::time::Duration;
  ///
  /// #[tokio::main]
  /// async fn main() {
  ///     let mut set = JoinSet::new();
  ///
  ///     for i in 0..3 {
  ///        set.spawn(async move {
  ///            tokio::time::sleep(Duration::from_secs(3 - i)).await;
  ///            i
  ///        });
  ///     }
  ///
  ///     let output = set.join_all().await;
  ///     assert_eq!(output, vec![2, 1, 0]);
  /// }
  /// ```
  ///
  /// Equivalent implementation of `join_all`, using [`join_next`] and loop.
  ///
  /// ```no_run
  /// use tokio_with_wasm::alias as tokio;
  /// use tokio::task::JoinSet;
  ///
  /// #[tokio::main]
  /// async fn main() {
  ///     let mut set = JoinSet::new();
  ///
  ///     for i in 0..3 {
  ///        set.spawn(async move {i});
  ///     }
  ///
  ///     let mut output = Vec::new();
  ///     while let Some(res) = set.join_next().await{
  ///         match res {
  ///             Ok(t) => output.push(t),
  ///             Err(_) => (),
  ///         }
  ///     }
  ///     assert_eq!(output.len(),3);
  /// }
  /// ```
  /// [`join_next`]: fn@Self::join_next
  pub async fn join_all(mut self) -> Vec<T> {
    let mut output = Vec::with_capacity(self.len());

    while let Some(res) = self.join_next().await {
      if let Ok(t) = res {
        output.push(t)
      }
    }
    output
  }

  /// Polls for one of the tasks in the set to complete.
  ///
  /// If this returns `Poll::Ready(Some(_))`, then the task that completed is removed from the set.
  ///
  /// When the method returns `Poll::Pending`, the `Waker` in the provided `Context` is scheduled
  /// to receive a wakeup when a task in the `JoinSet` completes. Note that on multiple calls to
  /// `poll_join_next`, only the `Waker` from the `Context` passed to the most recent call is
  /// scheduled to receive a wakeup.
  ///
  /// # Returns
  ///
  /// This function returns:
  ///
  ///  * `Poll::Pending` if the `JoinSet` is not empty but there is no task whose output is
  ///    available right now.
  ///  * `Poll::Ready(Some(Ok(value)))` if one of the tasks in this `JoinSet` has completed.
  ///    The `value` is the return value of one of the tasks that completed.
  ///  * `Poll::Ready(Some(Err(err)))` if one of the tasks in this `JoinSet` has panicked or been
  ///    aborted. The `err` is the `JoinError` from the panicked/aborted task.
  ///  * `Poll::Ready(None)` if the `JoinSet` is empty.
  pub fn poll_join_next(
    &mut self,
    cx: &mut Context<'_>,
  ) -> Poll<Option<Result<T, JoinError>>> {
    loop {
      let Some(tag) = self.queue.pop_or_register(cx.waker()) else {
        if self.tasks.is_empty() {
          return Poll::Ready(None);
        }
        return Poll::Pending;
      };
      // A tag can be stale if its task was detached earlier.
      let Some(mut handle) = self.tasks.remove(&tag) else {
        continue;
      };
      // The task queued its tag on completion, so its result is stored;
      // this poll with a no-op waker just takes the result out.
      let mut noop_cx = Context::from_waker(std::task::Waker::noop());
      if let Poll::Ready(result) = Pin::new(&mut handle).poll(&mut noop_cx) {
        return Poll::Ready(Some(result));
      }
    }
  }
}

impl<T> Drop for JoinSet<T> {
  fn drop(&mut self) {
    self.abort_all();
  }
}

impl<T> Debug for JoinSet<T> {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("JoinSet").field("len", &self.len()).finish()
  }
}

impl<T> Default for JoinSet<T> {
  fn default() -> Self {
    Self::new()
  }
}
