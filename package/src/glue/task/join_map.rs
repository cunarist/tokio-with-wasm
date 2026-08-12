//! A collection of tasks spawned in JavaScript runtime, keyed by a value.
//!
//! This module provides the [`JoinMap`] type, a collection which stores a set
//! of spawned tasks and lets each of them be identified, aborted and awaited
//! by a key. See the documentation for the [`JoinMap`] type for details.
use crate::{JoinError, JoinHandle, spawn, spawn_blocking};
use std::borrow::Borrow;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::hash::Hash;
use std::iter::FusedIterator;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A collection of tasks spawned in JavaScript, associated with keys.
///
/// A `JoinMap` behaves like a [`JoinSet`] whose tasks each carry a key, so
/// that a single task can be aborted or looked up without holding on to its
/// [`AbortHandle`]. Completed tasks are returned together with their key.
/// The map is not ordered: when several tasks have already completed,
/// which of them is returned first is unspecified.
///
/// All of the tasks must have the same key type `K` and return type `V`.
/// Like in `tokio-util`, keys must be `Hash + Eq`.
///
/// When the `JoinMap` is dropped, all tasks in it are immediately aborted.
///
/// The counterpart of this type on native platforms lives in `tokio-util`,
/// not in `tokio`, so [`alias`] does not cover it. Portable code picks the
/// type per target, with the same gate this crate uses:
///
/// ```ignore
/// #[cfg(all(
///     target_family = "wasm",
///     target_vendor = "unknown",
///     target_os = "unknown"
/// ))]
/// use tokio_with_wasm::task::JoinMap;
/// #[cfg(not(all(
///     target_family = "wasm",
///     target_vendor = "unknown",
///     target_os = "unknown"
/// )))]
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
  inner: HashMap<K, JoinHandle<V>>,
}

impl<K, V> JoinMap<K, V> {
  /// Creates a new `JoinMap`.
  pub fn new() -> Self {
    Self {
      inner: HashMap::new(),
    }
  }

  /// Creates a new `JoinMap` with room for at least `capacity` tasks.
  pub fn with_capacity(capacity: usize) -> Self {
    Self {
      inner: HashMap::with_capacity(capacity),
    }
  }

  /// Returns the number of tasks the map can hold without reallocating.
  pub fn capacity(&self) -> usize {
    self.inner.capacity()
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
  pub fn keys(&self) -> impl ExactSizeIterator<Item = &K> + FusedIterator {
    self.inner.keys()
  }

  /// Aborts all tasks on this `JoinMap`.
  ///
  /// This does not remove the tasks from the `JoinMap`. To wait for the tasks
  /// to complete cancellation, you should call `join_next` in a loop until
  /// the `JoinMap` is empty.
  pub fn abort_all(&mut self) {
    self.inner.values().for_each(|join_handle| join_handle.abort());
  }

  /// Removes all tasks from this `JoinMap` without aborting them.
  ///
  /// The tasks removed by this call will continue to run in the background
  /// even if the `JoinMap` is dropped.
  pub fn detach_all(&mut self) {
    self.inner.clear();
  }
}

impl<K, V> JoinMap<K, V>
where
  K: Hash + Eq,
  V: 'static,
{
  /// Spawns the provided task on the `JoinMap` and stores it under `key`.
  ///
  /// The provided future will start running in the background immediately
  /// when this method is called, even if you don't await anything on this
  /// `JoinMap`.
  ///
  /// If a task previously existed in the `JoinMap` for this key, that task
  /// is aborted and dropped; its output is never returned. Note that
  /// aborting cannot interrupt code that is already running: the previous
  /// task stops at its next `.await`, and a blocking task that has already
  /// started on a worker runs to completion in the background
  /// (see [`JoinHandle::abort`]).
  ///
  /// [`JoinHandle::abort`]: crate::task::JoinHandle::abort
  #[track_caller]
  pub fn spawn<F>(&mut self, key: K, task: F)
  where
    F: Future<Output = V>,
    F: 'static,
  {
    let join_handle = spawn(task);
    if let Some(old_handle) = self.inner.insert(key, join_handle) {
      old_handle.abort();
    }
  }

  /// Spawns the provided task on the `JoinMap` and stores it under `key`.
  ///
  /// On the web there is no separate thread-local executor, so this is
  /// the same as [`spawn`](Self::spawn). It exists so that code written
  /// against `tokio-util` compiles unchanged.
  #[track_caller]
  pub fn spawn_local<F>(&mut self, key: K, task: F)
  where
    F: Future<Output = V>,
    F: 'static,
  {
    self.spawn(key, task);
  }

  /// Spawns the blocking code on the blocking threadpool, stored under `key`.
  ///
  /// If a task previously existed in the `JoinMap` for this key, that task
  /// is aborted and dropped; its output is never returned. A blocking task
  /// that has already started on a worker cannot be interrupted and runs to
  /// completion in the background; only its output is discarded
  /// (see [`JoinHandle::abort`]).
  ///
  /// [`JoinHandle::abort`]: crate::task::JoinHandle::abort
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
    let join_handle = spawn_blocking(f);
    if let Some(old_handle) = self.inner.insert(key, join_handle) {
      old_handle.abort();
    }
  }

  /// Returns whether the `JoinMap` holds a task for `key`.
  pub fn contains_key<Q>(&self, key: &Q) -> bool
  where
    K: Borrow<Q>,
    Q: Hash + Eq + ?Sized,
  {
    self.inner.contains_key(key)
  }

  /// Aborts the task stored under `key`.
  ///
  /// Returns whether a task was found for that key. The task stays in the
  /// `JoinMap` until it is joined. A task aborted in time reports a
  /// cancelled [`JoinError`]; a task that had already finished, or a
  /// blocking task that had already started on a worker, still yields its
  /// output as usual (see [`JoinHandle::abort`]).
  ///
  /// [`JoinHandle::abort`]: crate::task::JoinHandle::abort
  pub fn abort<Q>(&mut self, key: &Q) -> bool
  where
    K: Borrow<Q>,
    Q: Hash + Eq + ?Sized,
  {
    match self.inner.get(key) {
      Some(join_handle) => {
        join_handle.abort();
        true
      }
      None => false,
    }
  }

  /// Aborts every task whose key matches the predicate.
  ///
  /// Like [`abort`](Self::abort), the aborted tasks stay in the `JoinMap`
  /// until they are joined.
  pub fn abort_matching(&mut self, mut predicate: impl FnMut(&K) -> bool) {
    for (key, join_handle) in &self.inner {
      if predicate(key) {
        join_handle.abort();
      }
    }
  }

  /// Reserves capacity for at least `additional` more tasks.
  pub fn reserve(&mut self, additional: usize) {
    self.inner.reserve(additional);
  }

  /// Shrinks the capacity of the map as much as possible.
  pub fn shrink_to_fit(&mut self) {
    self.inner.shrink_to_fit();
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
    // Take one finished task out of the map, leaving the rest untouched.
    let (key, mut handle) = self
      .inner
      .extract_if(|_, join_handle| join_handle.is_finished())
      .next()?;

    // The waker is a no-op because only a finished task is ever polled
    // here: the poll just takes the stored result out of the join channel.
    // Polling a pending task instead would make it register this waker
    // and lose the real one from the last `poll_join_next` call.
    let mut cx = Context::from_waker(std::task::Waker::noop());
    match Pin::new(&mut handle).poll(&mut cx) {
      Poll::Ready(result) => Some((key, result)),
      // A finished task always has its result stored, so this is
      // logically unreachable.
      Poll::Pending => None,
    }
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
  /// When this returns `Poll::Pending`, the `Waker` in the provided
  /// `Context` is scheduled to receive a wakeup when a task in the
  /// `JoinMap` completes; only the `Waker` from the most recent call is
  /// scheduled. This method is private because `tokio-util`'s `JoinMap`
  /// exposes no polling API; use [`join_next`](Self::join_next) instead.
  fn poll_join_next(
    &mut self,
    cx: &mut Context<'_>,
  ) -> Poll<Option<(K, Result<V, JoinError>)>> {
    if self.inner.is_empty() {
      return Poll::Ready(None);
    }

    // Take one finished task out of the map if there is one.
    let found = self
      .inner
      .extract_if(|_, join_handle| join_handle.is_finished())
      .next();
    if let Some((key, mut handle)) = found {
      return match Pin::new(&mut handle).poll(cx) {
        Poll::Ready(result) => Poll::Ready(Some((key, result))),
        // A finished task always has its result stored, so this is
        // logically unreachable.
        Poll::Pending => Poll::Pending,
      };
    }

    // No task has finished yet: poll every pending handle so that each of
    // them registers this waker. An unfinished task never returns `Ready`
    // from this poll, because its join channel holds no value yet.
    for join_handle in self.inner.values_mut() {
      let _ = Pin::new(join_handle).poll(cx);
    }
    Poll::Pending
  }
}

impl<K, V> Drop for JoinMap<K, V> {
  fn drop(&mut self) {
    self.abort_all();
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
