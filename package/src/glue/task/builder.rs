//! Task builder, mirroring the unstable `tokio::task::Builder`.

use crate::task::{JoinHandle, spawn, spawn_blocking};
use std::future::Future;
use std::io;
use std::marker::PhantomData;

/// Factory which is used to configure the properties of a new task.
///
/// Like in `tokio`, this needs the `tracing` feature and `--cfg tokio_unstable`.
/// The web has no `tracing` instrumentation, so the task name is dropped.
///
/// # Examples
///
/// ```no_run
/// use tokio::task::Builder;
/// use tokio_with_wasm::alias as tokio;
///
/// async fn work() -> std::io::Result<()> {
///   let handle = Builder::new().name("my_task").spawn(async { 6 * 7 })?;
///   assert_eq!(handle.await.expect("the task failed"), 42);
///   Ok(())
/// }
/// ```
#[derive(Default, Debug)]
pub struct Builder<'a> {
  name: PhantomData<&'a str>,
}

impl<'a> Builder<'a> {
  /// Creates a new task builder.
  pub fn new() -> Self {
    Self::default()
  }

  /// Assigns a name to the task which will be spawned.
  pub fn name(&self, _name: &'a str) -> Self {
    Self::default()
  }

  /// Spawns a task with this builder's settings on the JavaScript
  /// event loop.
  pub fn spawn<F, T>(self, future: F) -> io::Result<JoinHandle<T>>
  where
    F: Future<Output = T> + 'static,
    T: 'static,
  {
    Ok(spawn(future))
  }

  /// Spawns a task on the current thread with this builder's settings.
  ///
  /// On the web every task runs on the current thread,
  /// so this is the same as [`spawn`](Self::spawn).
  pub fn spawn_local<F, T>(self, future: F) -> io::Result<JoinHandle<T>>
  where
    F: Future<Output = T> + 'static,
    T: 'static,
  {
    self.spawn(future)
  }

  /// Spawns blocking code on the blocking web worker pool
  /// with this builder's settings.
  pub fn spawn_blocking<C, T>(self, callable: C) -> io::Result<JoinHandle<T>>
  where
    C: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
  {
    Ok(spawn_blocking(callable))
  }
}
