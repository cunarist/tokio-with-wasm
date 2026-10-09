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
use std::marker::PhantomData;

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
/// The native counterpart lives in `tokio-util` behind its `join-map`
/// feature, so [`alias`] does not cover it. Portable code picks the type per
/// target, with the same gate this crate uses:
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
  tasks: HashTable<(K, AbortHandle)>,
  /// The key hash of every task, to find its key once it completes.
  /// Its hasher also hashes the keys.
  hashes: HashMap<Id, u64, S>,
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
      hashes: HashMap::with_capacity_and_hasher(capacity, hash_builder),
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
}

impl<K, V: 'static, S> JoinMap<K, V, S> {
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
  /// is aborted and dropped; its output is never returned
  /// (see [`AbortHandle`] for how far aborting reaches).
  pub fn spawn<F>(&mut self, key: K, task: F)
  where
    F: Future<Output = V>,
    F: 'static,
  {
    let abort = self.set.spawn(task);
    self.store(key, abort);
  }

  /// Like [`spawn`](Self::spawn), as every task runs on the current thread.
  pub fn spawn_local<F>(&mut self, key: K, task: F)
  where
    F: Future<Output = V>,
    F: 'static,
  {
    self.spawn(key, task);
  }

  /// Like [`spawn`](Self::spawn), but runs blocking code on a web worker.
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
    let hasher = self.hashes.hasher();
    let hash = hasher.hash_one(&key);
    let id = abort.id();
    let entry = self.tasks.entry(
      hash,
      |(stored, _)| *stored == key,
      |(stored, _)| hasher.hash_one(stored),
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
    self.hashes.insert(id, hash);
  }

  /// Returns an iterator over the keys of the tasks in the `JoinMap`,
  /// including tasks that completed but were not joined yet.
  pub fn keys(&self) -> JoinMapKeys<'_, K, V> {
    JoinMapKeys {
      iter: self.tasks.iter(),
      _value: PhantomData,
    }
  }

  /// Returns whether the `JoinMap` holds a task for `key`.
  pub fn contains_key<Q>(&self, key: &Q) -> bool
  where
    K: Borrow<Q>,
    Q: Hash + Eq + ?Sized,
  {
    self.find(key).is_some()
  }

  /// Returns whether the `JoinMap` holds the task with this ID.
  pub fn contains_task(&self, task_id: &Id) -> bool {
    self.hashes.contains_key(task_id)
  }

  fn find<Q>(&self, key: &Q) -> Option<&(K, AbortHandle)>
  where
    K: Borrow<Q>,
    Q: Hash + Eq + ?Sized,
  {
    let hash = self.hashes.hasher().hash_one(key);
    self.tasks.find(hash, |(stored, _)| stored.borrow() == key)
  }

  /// Aborts the task stored under `key`.
  ///
  /// Returns whether a task was found for that key. The task stays in the
  /// `JoinMap` until it is joined, like with [`AbortHandle::abort`].
  pub fn abort<Q>(&mut self, key: &Q) -> bool
  where
    K: Borrow<Q>,
    Q: Hash + Eq + ?Sized,
  {
    self.find(key).map(|(_, abort)| abort.abort()).is_some()
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
    let hasher = self.hashes.hasher();
    self
      .tasks
      .reserve(additional, |(key, _)| hasher.hash_one(key));
    self.hashes.reserve(additional);
  }

  /// Shrinks the capacity of the map as much as possible.
  pub fn shrink_to_fit(&mut self) {
    self.shrink_to(0);
  }

  /// Shrinks the capacity of the map, keeping at least `min_capacity`.
  pub fn shrink_to(&mut self, min_capacity: usize) {
    self.hashes.shrink_to(min_capacity);
    let hasher = self.hashes.hasher();
    self
      .tasks
      .shrink_to(min_capacity, |(key, _)| hasher.hash_one(key));
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

impl<K: Debug, V, S> Debug for JoinMap<K, V, S> {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
    let tasks = self.tasks.iter().map(|(key, abort)| (key, abort.id()));
    f.debug_map().entries(tasks).finish()
  }
}

impl<K, V> Default for JoinMap<K, V> {
  fn default() -> Self {
    Self::new()
  }
}

/// An iterator over the keys of a [`JoinMap`].
#[derive(Debug, Clone)]
pub struct JoinMapKeys<'a, K, V> {
  iter: hashbrown::hash_table::Iter<'a, (K, AbortHandle)>,
  _value: PhantomData<&'a V>,
}

impl<'a, K, V> Iterator for JoinMapKeys<'a, K, V> {
  type Item = &'a K;

  fn next(&mut self) -> Option<&'a K> {
    self.iter.next().map(|(key, _)| key)
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    self.iter.size_hint()
  }
}

impl<K, V> ExactSizeIterator for JoinMapKeys<'_, K, V> {}

impl<K, V> FusedIterator for JoinMapKeys<'_, K, V> {}
