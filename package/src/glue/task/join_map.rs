//! A collection of tasks spawned in JavaScript runtime, keyed by a value.
//!
//! This module provides the [`JoinMap`] type, a collection which stores a set
//! of spawned tasks and lets each of them be identified, aborted and awaited
//! by a key. See the documentation for the [`JoinMap`] type for details.
use crate::task::{AbortHandle, Id, JoinError, JoinSet};
use hashbrown::HashTable;
use hashbrown::hash_table::Entry;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::hash::{BuildHasher, Hash};
use std::iter::FusedIterator;

/// A collection of tasks spawned in JavaScript, associated with keys.
///
/// A `JoinMap` behaves like a [`JoinSet`] whose tasks each carry a key, so
/// that a single task can be aborted or looked up without holding on to its
/// [`AbortHandle`]. Completed tasks are returned together with their key,
/// in the order they complete.
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
///   target_family = "wasm",
///   target_vendor = "unknown",
///   target_os = "unknown"
/// ))]
/// use tokio_with_wasm::task::JoinMap;
/// #[cfg(not(all(
///   target_family = "wasm",
///   target_vendor = "unknown",
///   target_os = "unknown"
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
///   let mut map = JoinMap::new();
///
///   for i in 0..10 {
///     map.spawn(i, async move { i * 2 });
///   }
///
///   let mut seen = [false; 10];
///   while let Some((key, result)) = map.join_next().await {
///     let output = result.unwrap();
///     assert_eq!(output, key * 2);
///     seen[key] = true;
///   }
///
///   for i in 0..10 {
///     assert!(seen[i]);
///   }
/// }
/// ```
///
/// [`JoinSet`]: crate::task::JoinSet
/// [`AbortHandle`]: crate::task::AbortHandle
/// [`alias`]: crate::alias
pub struct JoinMap<K, V, S = RandomState> {
  /// The key of every task that has not been joined yet,
  /// addressed by the hash of the key.
  tasks: HashTable<(K, AbortHandle)>,
  /// The key hash of every task, to find its key once it completes.
  hashes: HashMap<Id, u64>,
  hasher: S,
  set: JoinSet<V>,
}

impl<K, V> JoinMap<K, V> {
  /// Creates a new `JoinMap`.
  pub fn new() -> Self {
    Self::with_hasher(RandomState::new())
  }

  /// Creates a new `JoinMap` with room for at least `capacity` tasks.
  pub fn with_capacity(capacity: usize) -> Self {
    Self::with_capacity_and_hasher(capacity, RandomState::new())
  }
}

impl<K, V, S> JoinMap<K, V, S> {
  /// Creates a new `JoinMap` using `hash_builder` to hash the keys.
  pub fn with_hasher(hash_builder: S) -> Self {
    Self::with_capacity_and_hasher(0, hash_builder)
  }

  /// Creates a new `JoinMap` with room for at least `capacity` tasks,
  /// using `hash_builder` to hash the keys.
  pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> Self {
    Self {
      tasks: HashTable::with_capacity(capacity),
      hashes: HashMap::with_capacity(capacity),
      hasher: hash_builder,
      set: JoinSet::new(),
    }
  }

  /// Returns the number of tasks the map can hold without reallocating.
  pub fn capacity(&self) -> usize {
    self.tasks.capacity()
  }

  /// Returns the number of tasks currently in the `JoinMap`.
  pub fn len(&self) -> usize {
    self.tasks.len()
  }

  /// Returns whether the `JoinMap` is empty.
  pub fn is_empty(&self) -> bool {
    self.tasks.is_empty()
  }

  /// Returns an iterator over the keys of the tasks in the `JoinMap`.
  ///
  /// The order is unspecified and changes as tasks complete.
  pub fn keys(&self) -> impl ExactSizeIterator<Item = &K> + FusedIterator {
    self.tasks.iter().map(|(key, _)| key)
  }

  /// Aborts all tasks on this `JoinMap`.
  ///
  /// This does not remove the tasks from the `JoinMap`. To wait for the tasks
  /// to complete cancellation, you should call `join_next` in a loop until
  /// the `JoinMap` is empty.
  pub fn abort_all(&mut self) {
    self.set.abort_all();
  }

  /// Removes all tasks from this `JoinMap` without aborting them.
  ///
  /// The tasks removed by this call will continue to run in the background
  /// even if the `JoinMap` is dropped.
  pub fn detach_all(&mut self) {
    self.set.detach_all();
    self.tasks.clear();
    self.hashes.clear();
  }

  /// Returns whether the `JoinMap` holds the task with this ID.
  pub fn contains_task(&self, task_id: &Id) -> bool {
    self.hashes.contains_key(task_id)
  }
}

impl<K, V, S> JoinMap<K, V, S>
where
  K: Hash + Eq,
  V: 'static,
  S: BuildHasher,
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
    let abort = self.set.spawn(task);
    self.store(key, abort);
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
  ///   let mut map = JoinMap::new();
  ///
  ///   for i in 0..10 {
  ///     map.spawn_blocking(i, move || i * 2);
  ///   }
  ///
  ///   while let Some((key, result)) = map.join_next().await {
  ///     assert_eq!(result.unwrap(), key * 2);
  ///   }
  /// }
  /// ```
  #[track_caller]
  pub fn spawn_blocking<F>(&mut self, key: K, f: F)
  where
    F: FnOnce() -> V,
    F: Send + 'static,
    V: Send,
  {
    let abort = self.set.spawn_blocking(f);
    self.store(key, abort);
  }

  /// Stores a spawned task under `key`, aborting and replacing the
  /// previous task for that key if there was one.
  fn store(&mut self, key: K, abort: AbortHandle) {
    let hash = self.hasher.hash_one(&key);
    self.hashes.insert(abort.id(), hash);
    let hashes = &self.hashes;
    let entry = self.tasks.entry(
      hash,
      |(stored, _)| *stored == key,
      |(_, stored)| hashes[&stored.id()],
    );
    match entry {
      Entry::Occupied(mut occupied) => {
        let (_, replaced) = std::mem::replace(occupied.get_mut(), (key, abort));
        replaced.abort();
        // Once it completes, the replaced task finds no key and is skipped.
        self.hashes.remove(&replaced.id());
      }
      Entry::Vacant(vacant) => {
        vacant.insert((key, abort));
      }
    }
  }

  /// Returns whether the `JoinMap` holds a task for `key`.
  pub fn contains_key<Q>(&self, key: &Q) -> bool
  where
    K: Borrow<Q>,
    Q: Hash + Eq + ?Sized,
  {
    let hash = self.hasher.hash_one(key);
    self
      .tasks
      .find(hash, |(stored, _)| stored.borrow() == key)
      .is_some()
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
    let hash = self.hasher.hash_one(key);
    let found = self.tasks.find(hash, |(stored, _)| stored.borrow() == key);
    found.map(|(_, abort)| abort.abort()).is_some()
  }

  /// Aborts every task whose key matches the predicate.
  ///
  /// Like [`abort`](Self::abort), the aborted tasks stay in the `JoinMap`
  /// until they are joined.
  pub fn abort_matching(&mut self, mut predicate: impl FnMut(&K) -> bool) {
    for (key, abort) in self.tasks.iter() {
      if predicate(key) {
        abort.abort();
      }
    }
  }

  /// Reserves capacity for at least `additional` more tasks.
  pub fn reserve(&mut self, additional: usize) {
    let hashes = &self.hashes;
    self
      .tasks
      .reserve(additional, |(_, abort)| hashes[&abort.id()]);
    self.hashes.reserve(additional);
  }

  /// Shrinks the capacity of the map as much as possible.
  pub fn shrink_to_fit(&mut self) {
    let hashes = &self.hashes;
    self.tasks.shrink_to_fit(|(_, abort)| hashes[&abort.id()]);
    self.hashes.shrink_to_fit();
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
    loop {
      let joined = self.set.join_next_with_id().await?;
      if let Some(entry) = self.take(joined) {
        return Some(entry);
      }
    }
  }

  /// Tries to join one of the tasks in the map that has completed and return
  /// its key and output.
  ///
  /// Returns `None` if there are no completed tasks, or if the map is empty.
  pub fn try_join_next(&mut self) -> Option<(K, Result<V, JoinError>)> {
    while let Some(joined) = self.set.try_join_next_with_id() {
      if let Some(entry) = self.take(joined) {
        return Some(entry);
      }
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

  /// Removes a joined task, returning its key with its output.
  /// Returns `None` for a task that was replaced under its key.
  fn take(
    &mut self,
    joined: Result<(Id, V), JoinError>,
  ) -> Option<(K, Result<V, JoinError>)> {
    let (id, result) = match joined {
      Ok((id, output)) => (id, Ok(output)),
      Err(error) => (error.id(), Err(error)),
    };
    let hash = self.hashes.remove(&id)?;
    let found = self.tasks.find_entry(hash, |(_, abort)| abort.id() == id);
    let ((key, _), _) = found.ok()?.remove();
    Some((key, result))
  }
}

impl<K, V, S> Debug for JoinMap<K, V, S> {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("JoinMap").field("len", &self.len()).finish()
  }
}

impl<K, V, S: Default> Default for JoinMap<K, V, S> {
  fn default() -> Self {
    Self::with_hasher(S::default())
  }
}
