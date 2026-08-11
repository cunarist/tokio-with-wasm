//! A collection of tasks spawned in JavaScript runtime, keyed by a value.
//!
//! This module provides the [`JoinMap`] type, a collection which stores a set
//! of spawned tasks and lets each of them be identified, aborted and awaited
//! by a key. See the documentation for the [`JoinMap`] type for details.
use crate::{JoinError, JoinHandle, spawn, spawn_blocking};
use std::borrow::Borrow;
use std::collections::VecDeque;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A collection of tasks spawned in JavaScript, associated with keys.
///
/// A `JoinMap` behaves like a [`JoinSet`] whose tasks each carry a key, so
/// that a single task can be aborted or looked up without holding on to its
/// [`AbortHandle`]. Completed tasks are returned together with their key,
/// in the order they complete.
///
/// All of the tasks must have the same key type `K` and return type `V`.
///
/// When the `JoinMap` is dropped, all tasks in it are immediately aborted.
///
/// The counterpart of this type on native platforms lives in `tokio-util`,
/// not in `tokio`, so [`alias`] does not cover it. Portable code picks the
/// type per target:
///
/// ```ignore
/// #[cfg(target_arch = "wasm32")]
/// use tokio_with_wasm::task::JoinMap;
/// #[cfg(not(target_arch = "wasm32"))]
/// use tokio_util::task::JoinMap;
/// ```
///
/// # Examples
///
/// Spawn multiple tasks and wait for them.
///
/// ```no_run
/// use tokio_with_wasm::alias as tokio;
/// use tokio_with_wasm::task::JoinMap;
///
/// #[tokio::main]
/// async fn main() {
///     let mut map = JoinMap::new();
///
///     for i in 0..10 {
///         map.spawn(i, async move { i * 2 });
///     }
///
///     let mut seen = [false; 10];
///     while let Some((key, result)) = map.join_next().await {
///         let output = result.unwrap();
///         assert_eq!(output, key * 2);
///         seen[key] = true;
///     }
///
///     for i in 0..10 {
///         assert!(seen[i]);
///     }
/// }
/// ```
///
/// [`JoinSet`]: crate::task::JoinSet
/// [`AbortHandle`]: crate::task::AbortHandle
/// [`alias`]: crate::alias
pub struct JoinMap<K, V> {
  inner: VecDeque<(K, JoinHandle<V>)>,
}

impl<K, V> JoinMap<K, V> {
  /// Creates a new `JoinMap`.
  pub fn new() -> Self {
    Self {
      inner: VecDeque::new(),
    }
  }

  /// Creates a new `JoinMap` with room for at least `capacity` tasks.
  pub fn with_capacity(capacity: usize) -> Self {
    Self {
      inner: VecDeque::with_capacity(capacity),
    }
  }

  /// Returns the number of tasks currently in the `JoinMap`.
  pub fn len(&self) -> usize {
    self.inner.len()
  }

  /// Returns whether the `JoinMap` is empty.
  pub fn is_empty(&self) -> bool {
    self.inner.is_empty()
  }

  /// Returns an iterator over the keys of the tasks in the `JoinMap`.
  ///
  /// The order is unspecified and changes as tasks complete.
  pub fn keys(&self) -> impl Iterator<Item = &K> {
    self.inner.iter().map(|(key, _)| key)
  }
}

impl<K, V> JoinMap<K, V>
where
  K: Eq,
  V: 'static,
{
  /// Spawns the provided task on the `JoinMap` and stores it under `key`.
  ///
  /// The provided future will start running in the background immediately
  /// when this method is called, even if you don't await anything on this
  /// `JoinMap`.
  ///
  /// If a task previously existed in the `JoinMap` for this key, that task
  /// is cancelled and dropped. Its output is never returned.
  #[track_caller]
  pub fn spawn<F>(&mut self, key: K, task: F)
  where
    F: Future<Output = V>,
    F: 'static,
  {
    self.replace(&key);
    let join_handle = spawn(task);
    self.inner.push_back((key, join_handle));
  }

  /// Spawns the blocking code on the blocking threadpool, stored under `key`.
  ///
  /// If a task previously existed in the `JoinMap` for this key, that task
  /// is cancelled and dropped. Its output is never returned.
  ///
  /// # Examples
  ///
  /// Spawn multiple blocking tasks and wait for them.
  ///
  /// ```no_run
  /// use tokio_with_wasm::alias as tokio;
  /// use tokio_with_wasm::task::JoinMap;
  ///
  /// #[tokio::main]
  /// async fn main() {
  ///     let mut map = JoinMap::new();
  ///
  ///     for i in 0..10 {
  ///         map.spawn_blocking(i, move || i * 2);
  ///     }
  ///
  ///     while let Some((key, result)) = map.join_next().await {
  ///         assert_eq!(result.unwrap(), key * 2);
  ///     }
  /// }
  /// ```
  #[track_caller]
  pub fn spawn_blocking<F>(&mut self, key: K, f: F)
  where
    F: FnOnce() -> V,
    F: Send + 'static,
    V: Send,
  {
    self.replace(&key);
    let join_handle = spawn_blocking(f);
    self.inner.push_back((key, join_handle));
  }

  /// Aborts and forgets the task stored under `key`, if there is one.
  /// Called before a new task takes the key over.
  fn replace(&mut self, key: &K) {
    self.inner.retain(|(stored_key, join_handle)| {
      let is_other = stored_key != key;
      if !is_other {
        join_handle.abort();
      }
      is_other
    });
  }

  /// Returns whether the `JoinMap` holds a task for `key`.
  pub fn contains_key<Q>(&self, key: &Q) -> bool
  where
    K: Borrow<Q>,
    Q: Eq + ?Sized,
  {
    self
      .inner
      .iter()
      .any(|(stored_key, _)| stored_key.borrow() == key)
  }

  /// Aborts the task stored under `key`.
  ///
  /// Returns whether a task was found for that key. The task stays in the
  /// `JoinMap` until it is joined, at which point it reports a cancelled
  /// [`JoinError`].
  pub fn abort<Q>(&mut self, key: &Q) -> bool
  where
    K: Borrow<Q>,
    Q: Eq + ?Sized,
  {
    let mut is_found = false;
    for (stored_key, join_handle) in &self.inner {
      if stored_key.borrow() == key {
        join_handle.abort();
        is_found = true;
      }
    }
    is_found
  }

  /// Waits until one of the tasks in the map completes and returns its key
  /// and output.
  ///
  /// Returns `None` if the map is empty.
  ///
  /// # Cancel Safety
  ///
  /// This method is cancel safe. If `join_next` is used as the event in a
  /// `tokio::select!` statement and some other branch completes first, it is
  /// guaranteed that no tasks were removed from this `JoinMap`.
  pub async fn join_next(&mut self) -> Option<(K, Result<V, JoinError>)> {
    std::future::poll_fn(|cx| self.poll_join_next(cx)).await
  }

  /// Tries to join one of the tasks in the map that has completed and return
  /// its key and output.
  ///
  /// Returns `None` if there are no completed tasks, or if the map is empty.
  pub fn try_join_next(&mut self) -> Option<(K, Result<V, JoinError>)> {
    // Get the number of `JoinHandle`s.
    let handle_count = self.inner.len();
    if handle_count == 0 {
      return None;
    }

    // Create an async context with a waker that does nothing.
    // It is only ever handed to a task that is already finished,
    // because polling a pending task would make it register this waker
    // and lose the real one from the last `poll_join_next` call.
    let mut cx = Context::from_waker(std::task::Waker::noop());

    // Loop over all `JoinHandle`s to find one that's ready.
    for _ in 0..handle_count {
      let (key, mut handle) = match self.inner.pop_front() {
        Some(inner) => inner,
        None => continue, // Logically never none
      };
      if handle.is_finished() {
        let polled = Pin::new(&mut handle).poll(&mut cx);
        if let Poll::Ready(result) = polled {
          return Some((key, result));
        }
      }
      self.inner.push_back((key, handle));
    }

    None
  }

  /// Aborts all tasks and waits for them to finish shutting down.
  ///
  /// Calling this method is equivalent to calling [`abort_all`] and then
  /// calling [`join_next`] in a loop until it returns `None`.
  ///
  /// This method ignores any panics in the tasks shutting down. When this
  /// call returns, the `JoinMap` will be empty.
  ///
  /// [`abort_all`]: fn@Self::abort_all
  /// [`join_next`]: fn@Self::join_next
  pub async fn shutdown(&mut self) {
    self.abort_all();
    while self.join_next().await.is_some() {}
  }

  /// Polls for one of the tasks in the map to complete.
  ///
  /// If this returns `Poll::Ready(Some(_))`, then the task that completed is
  /// removed from the map.
  ///
  /// When the method returns `Poll::Pending`, the `Waker` in the provided
  /// `Context` is scheduled to receive a wakeup when a task in the `JoinMap`
  /// completes. Note that on multiple calls to `poll_join_next`, only the
  /// `Waker` from the `Context` passed to the most recent call is scheduled
  /// to receive a wakeup.
  ///
  /// # Returns
  ///
  /// This function returns:
  ///
  ///  * `Poll::Pending` if the `JoinMap` is not empty but there is no task
  ///    whose output is available right now.
  ///  * `Poll::Ready(Some((key, Ok(value))))` if one of the tasks in this
  ///    `JoinMap` has completed.
  ///  * `Poll::Ready(Some((key, Err(err))))` if one of the tasks in this
  ///    `JoinMap` has panicked or been aborted.
  ///  * `Poll::Ready(None)` if the `JoinMap` is empty.
  pub fn poll_join_next(
    &mut self,
    cx: &mut Context<'_>,
  ) -> Poll<Option<(K, Result<V, JoinError>)>> {
    // Get the number of `JoinHandle`s.
    let handle_count = self.inner.len();
    if handle_count == 0 {
      return Poll::Ready(None);
    }

    // Loop over all `JoinHandle`s to find one that's ready.
    for _ in 0..handle_count {
      let (key, mut handle) = match self.inner.pop_front() {
        Some(inner) => inner,
        None => continue, // Logically never none
      };
      let polled = Pin::new(&mut handle).poll(cx);
      if let Poll::Ready(result) = polled {
        return Poll::Ready(Some((key, result)));
      }
      self.inner.push_back((key, handle));
    }

    Poll::Pending
  }
}

impl<K, V> JoinMap<K, V> {
  /// Aborts all tasks on this `JoinMap`.
  ///
  /// This does not remove the tasks from the `JoinMap`. To wait for the tasks
  /// to complete cancellation, you should call `join_next` in a loop until
  /// the `JoinMap` is empty.
  pub fn abort_all(&mut self) {
    self
      .inner
      .iter()
      .for_each(|(_, join_handle)| join_handle.abort());
  }

  /// Removes all tasks from this `JoinMap` without aborting them.
  ///
  /// The tasks removed by this call will continue to run in the background
  /// even if the `JoinMap` is dropped.
  pub fn detach_all(&mut self) {
    self.inner.clear();
  }
}

impl<K, V> Drop for JoinMap<K, V> {
  fn drop(&mut self) {
    self
      .inner
      .iter()
      .for_each(|(_, join_handle)| join_handle.abort());
  }
}

impl<K, V> Debug for JoinMap<K, V> {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("JoinMap").field("len", &self.len()).finish()
  }
}

impl<K, V> Default for JoinMap<K, V> {
  fn default() -> Self {
    Self::new()
  }
}
