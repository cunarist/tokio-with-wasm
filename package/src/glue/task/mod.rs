//! Asynchronous green-threads.
//!
//! Resembling the familiar `tokio::task` patterns,
//! this module leverages web workers to execute tasks in parallel,
//! making it ideal for high-performance web applications.

mod join_map;
mod join_set;
mod pool;

pub use join_map::*;
pub use join_set::*;
use wasm_bindgen::prelude::JsValue;

use crate::{
  LogError, OnceReceiver, OnceSender, SelectFuture, is_main_thread,
  once_channel, set_timeout,
};
use js_sys::Promise;
use pool::WorkerPool;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use wasm_bindgen_futures::{JsFuture, spawn_local};

thread_local! {
    static WORKER_POOL: WorkerPool = WorkerPool::new();
}

/// Manages the worker pool by periodically checking for
/// inactive web workers and queued tasks.
///
/// This returns as soon as the pool holds no workers and no queued tasks,
/// so that an application that is done with blocking tasks doesn't keep a
/// timer running for the rest of the page's lifetime.
/// It is started again when the next blocking task arrives.
async fn manage_pool() {
  loop {
    let is_needed = WORKER_POOL.with(|worker_pool| {
      worker_pool.remove_inactive_workers();
      worker_pool.flush_queued_tasks();
      worker_pool.keep_managing()
    });
    if !is_needed {
      break;
    }
    let promise = Promise::new(&mut |resolve, _reject| {
      set_timeout(&resolve, 100.0);
    });
    JsFuture::from(promise).await.log_error("MANAGE_POOL");
  }
}

/// Spawns a new asynchronous task, returning a
/// [`JoinHandle`] for it.
///
/// The provided future will start running in the JavaScript event loop
/// when `spawn` is called, even if you don't await the returned
/// `JoinHandle`.
///
/// Spawning a task enables the task to execute concurrently to other tasks. The
/// spawned task will always execute on the current web worker(thread),
/// as that's how JavaScript's `Promise` basically works.
///
/// # Examples
///
/// In this example, a job is handed to a new task
/// that processes it concurrently.
///
/// ```no_run
/// use std::io;
/// use tokio_with_wasm::alias as tokio;
///
/// async fn process(job: u32) -> io::Result<u32> {
///     // Some process...
///     Ok(job)
/// }
///
/// async fn work(job: u32) -> io::Result<u32> {
///     tokio::spawn(async move {
///         // Process this job concurrently.
///         process(job).await
///     })
///     .await
///     .expect("the task was cancelled")
/// }
/// ```
///
/// To run multiple tasks in parallel and receive their results, join
/// handles can be stored in a vector.
///
/// ```no_run
/// use tokio_with_wasm::alias as tokio;
///
/// async fn my_background_op(id: i32) -> String {
///     format!("Finished background task {id}.")
/// }
///
/// async fn work() {
///     let ops = vec![1, 2, 3];
///     let mut tasks = Vec::with_capacity(ops.len());
///     for op in ops {
///         // This call will make them start running in the background
///         // immediately.
///         tasks.push(tokio::spawn(my_background_op(op)));
///     }
///
///     let mut outputs = Vec::with_capacity(tasks.len());
///     for task in tasks {
///         match task.await {
///             Ok(output) => outputs.push(output),
///             Err(error) => println!("An error occurred: {error}"),
///         }
///     }
///     println!("{outputs:?}");
/// }
/// ```
///
/// This example pushes the tasks to `outputs` in the order they were
/// started in.
///
/// # Using `!Send` values from a task
///
/// The task supplied to `spawn` is not required to implement `Send`.
/// This is different from multi-threaded native async runtimes,
/// because JavaScript environment is inherently single-threaded.
///
/// For example, this will work:
///
/// ```no_run
/// use std::rc::Rc;
/// use tokio_with_wasm::alias as tokio;
///
/// fn use_rc(rc: Rc<()>) {
///     // Do stuff w/ rc
///     drop(rc);
/// }
///
/// async fn work() {
///     let _ = tokio::spawn(async {
///         // Force the `Rc` to stay in a scope with no `.await`
///         {
///             let rc = Rc::new(());
///             use_rc(rc.clone());
///         }
///
///         tokio::task::yield_now().await;
///     }).await;
/// }
/// ```
///
/// This will work too, unlike multi-threaded native runtimes
/// where `!Send` values cannot live across `.await`:
///
/// ```no_run
/// use std::rc::Rc;
/// use tokio_with_wasm::alias as tokio;
///
/// fn use_rc(rc: Rc<()>) {
///     // Do stuff w/ rc
///     drop(rc);
/// }
///
/// async fn work() {
///     let _ = tokio::spawn(async {
///         let rc = Rc::new(());
///
///         tokio::task::yield_now().await;
///
///         use_rc(rc.clone());
///     }).await;
/// }
/// ```
pub fn spawn<F, T>(future: F) -> JoinHandle<T>
where
  F: std::future::Future<Output = T> + 'static,
  T: 'static,
{
  if !is_main_thread() {
    JsValue::from_str(concat!(
      "Calling `spawn` in a blocking thread is not allowed. ",
      "While this is possible in real `tokio`, ",
      "it may cause undefined behavior in the JavaScript environment. ",
      "Instead, use `tokio::sync::mpsc::channel` ",
      "to listen for messages from the main thread ",
      "and spawn a task there."
    ))
    .log_error("SPAWN");
    panic!();
  }
  let (join_sender, join_receiver) = once_channel();
  let (cancel_sender, cancel_receiver) = once_channel::<()>();
  spawn_local(async move {
    let result = SelectFuture::new(
      async move {
        let output = future.await;
        Ok(output)
      },
      async move {
        cancel_receiver.await;
        Err(JoinError::cancelled())
      },
    )
    .await;
    join_sender.send(result);
  });
  JoinHandle {
    join_receiver,
    cancel_sender,
  }
}

/// Runs the provided closure on a web worker(thread) where blocking is acceptable.
///
/// In general, issuing a blocking call or performing a lot of compute in a
/// future without yielding is problematic, as it may prevent the JavaScript runtime from
/// driving other futures forward. This function runs the provided closure on a
/// web worker dedicated to blocking operations.
///
/// More and more web workers will be spawned when they are requested through this
/// function until the upper limit of 512 is reached.
/// After reaching the upper limit, the tasks will wait for
/// any of the web workers to become idle.
/// When a web worker remains idle for 10 seconds, it will be terminated
/// and get removed from the worker pool, which is a similiar behavior to that of `tokio`.
/// The web worker limit is very large by default, because `spawn_blocking` is often
/// used for various kinds of IO operations that cannot be performed
/// asynchronously.  When you run CPU-bound code using `spawn_blocking`, you
/// should keep this large upper limit in mind.
///
/// A panic inside the closure takes down the web worker that runs it,
/// because a panic cannot be unwound on `wasm32-unknown-unknown`. The
/// returned handle then resolves to a [`JoinError`] whose
/// [`is_panic`](JoinError::is_panic) is `true`.
///
/// # Examples
///
/// Pass an input value and receive result of computation:
///
/// ```no_run
/// use tokio_with_wasm::alias as tokio;
///
/// async fn work() {
///     // Initial input
///     let mut data = "Hello, ".to_string();
///     let output = tokio::task::spawn_blocking(move || {
///         // Stand-in for compute-heavy work or using synchronous APIs
///         data.push_str("world");
///         // Pass ownership of the value back to the asynchronous context
///         data
///     })
///     .await
///     .expect("the blocking task did not finish");
///
///     // `output` is the value returned from the thread
///     assert_eq!(output.as_str(), "Hello, world");
/// }
/// ```
pub fn spawn_blocking<C, T>(callable: C) -> JoinHandle<T>
where
  C: FnOnce() -> T + Send + 'static,
  T: Send + 'static,
{
  if !is_main_thread() {
    JsValue::from_str(concat!(
      "Calling `spawn_blocking` in a blocking thread is not allowed. ",
      "While this is possible in real `tokio`, ",
      "it may cause undefined behavior in the JavaScript environment. ",
      "Instead, use `tokio::sync::mpsc::channel` ",
      "to listen for messages from the main thread ",
      "and spawn a task there."
    ))
    .log_error("SPAWN_BLOCKING");
    panic!();
  }
  let (join_sender, join_receiver) = once_channel();
  let (cancel_sender, cancel_receiver) = once_channel::<()>();
  let failure_sender = join_sender.clone();
  WORKER_POOL.with(move |worker_pool| {
    worker_pool.queue_task(
      move || {
        if cancel_receiver.is_done() {
          join_sender.send(Err(JoinError::cancelled()));
          return;
        }
        let returned = callable();
        join_sender.send(Ok(returned));
      },
      // Called when the web worker cannot run the task or dies while
      // running it. Without this, the `JoinHandle` would never resolve.
      move || {
        failure_sender.send(Err(JoinError::panicked()));
      },
    );
    if worker_pool.needs_managing() {
      spawn_local(manage_pool());
    }
  });
  JoinHandle {
    join_receiver,
    cancel_sender,
  }
}

/// Yields execution back to the JavaScript event loop.
///
/// To avoid blocking inside a long-running function,
/// you have to yield to the async event loop regularly.
///
/// The async task may resume when it has its turn back.
/// Meanwhile, any other pending tasks will be scheduled
/// by the JavaScript runtime.
pub async fn yield_now() {
  let promise = Promise::new(&mut |resolve, _reject| {
    set_timeout(&resolve, 0.0);
  });
  JsFuture::from(promise).await.log_error("YIELD_NOW");
}

/// An owned permission to join on a task (awaiting its termination).
///
/// This can be thought of as the equivalent of
/// [`std::thread::JoinHandle`] or `tokio::task::JoinHandle` for
/// a task that is executed concurrently.
///
/// A `JoinHandle` *detaches* the associated task when it is dropped, which
/// means that there is no longer any handle to the task, and no way to `join`
/// on it.
///
/// This struct is created by the [`spawn`] and [`spawn_blocking`]
/// functions.
///
/// # Examples
///
/// Creation from [`spawn`]:
///
/// ```no_run
/// use tokio_with_wasm::alias as tokio;
/// use tokio::spawn;
///
/// let join_handle: tokio::task::JoinHandle<_> = spawn(async {
///     // some work here
/// });
/// ```
///
/// Creation from [`spawn_blocking`]:
///
/// ```no_run
/// use tokio_with_wasm::alias as tokio;
/// use tokio::task::spawn_blocking;
///
/// let join_handle: tokio::task::JoinHandle<_> = spawn_blocking(|| {
///     // some blocking work here
/// });
/// ```
///
/// Child being detached and outliving its parent:
///
/// ```no_run
/// use tokio_with_wasm::alias as tokio;
/// use tokio::spawn;
///
/// async fn work() {
///     let original_task = spawn(async {
///         let _detached_task = spawn(async {
///             // Here we sleep to make sure that the first task returns
///             // before. Assume that code takes a few seconds to execute
///             // here. This will be called, even though the `JoinHandle`
///             // is dropped.
///             println!("♫ Still alive ♫");
///         });
///     });
///
///     let _ = original_task.await;
///     println!("Original task is joined.");
/// }
/// ```
pub struct JoinHandle<T> {
  join_receiver: OnceReceiver<Result<T, JoinError>>,
  cancel_sender: OnceSender<()>,
}

impl<T> Future for JoinHandle<T> {
  type Output = Result<T, JoinError>;
  fn poll(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Self::Output> {
    let pinned_receiver = Pin::new(&mut self.join_receiver);
    pinned_receiver.poll(cx)
  }
}

impl<T> Debug for JoinHandle<T>
where
  T: Debug,
{
  fn fmt(&self, fmt: &mut Formatter<'_>) -> std::fmt::Result {
    fmt.debug_struct("JoinHandle").finish()
  }
}

impl<T> JoinHandle<T> {
  /// Abort the task associated with the handle.
  ///
  /// Awaiting a cancelled task might complete as usual if the task was
  /// already completed at the time it was cancelled, but most likely it
  /// will fail with a cancelled `JoinError`.
  ///
  /// Be aware that tasks spawned using [`spawn_blocking`] cannot be aborted
  /// because they are not async. If you call `abort` on a `spawn_blocking`
  /// task, then this *will not have any effect*, and the task will continue
  /// running normally. The exception is if the task has not started running
  /// yet; in that case, calling `abort` may prevent the task from starting.
  ///
  /// ```no_run
  /// use tokio_with_wasm::alias as tokio;
  /// use tokio::time;
  ///
  /// async fn work() {
  ///     let mut handles = Vec::new();
  ///
  ///     handles.push(tokio::spawn(async {
  ///         time::sleep(time::Duration::from_secs(10)).await;
  ///         true
  ///     }));
  ///
  ///     handles.push(tokio::spawn(async {
  ///         time::sleep(time::Duration::from_secs(10)).await;
  ///         false
  ///     }));
  ///
  ///     for handle in &handles {
  ///         handle.abort();
  ///     }
  ///
  ///     for handle in handles {
  ///         assert!(handle.await.unwrap_err().is_cancelled());
  ///     }
  /// }
  /// ```
  pub fn abort(&self) {
    self.cancel_sender.send(());
  }

  /// Checks if the task associated with this `JoinHandle` has finished.
  ///
  /// Please note that this method can return `false` even if [`abort`] has been
  /// called on the task. This is because the cancellation process may take
  /// some time, and this method does not return `true` until it has
  /// completed.
  ///
  /// [`abort`]: JoinHandle::abort
  pub fn is_finished(&self) -> bool {
    self.join_receiver.is_done()
  }

  /// Returns a new `AbortHandle` that can be used to remotely abort this task.
  pub fn abort_handle(&self) -> AbortHandle {
    AbortHandle {
      cancel_sender: self.cancel_sender.clone(),
    }
  }
}

/// Returned when a task failed to execute to completion.
#[derive(Debug)]
pub struct JoinError {
  cancelled: bool,
  panicked: bool,
}

impl JoinError {
  /// The task was aborted before it could finish.
  fn cancelled() -> Self {
    JoinError {
      cancelled: true,
      panicked: false,
    }
  }

  /// The web worker running the task died,
  /// which is what a panic looks like on the web.
  fn panicked() -> Self {
    JoinError {
      cancelled: false,
      panicked: true,
    }
  }
}

impl Display for JoinError {
  fn fmt(&self, fmt: &mut Formatter<'_>) -> std::fmt::Result {
    if self.cancelled {
      fmt.write_str("task was cancelled")
    } else if self.panicked {
      fmt.write_str("task panicked")
    } else {
      fmt.write_str("task failed to execute to completion")
    }
  }
}

impl Error for JoinError {}

impl JoinError {
  /// Returns whether the error was caused by the task being cancelled.
  pub fn is_cancelled(&self) -> bool {
    self.cancelled
  }

  /// Returns whether the error was caused by the task panicking.
  ///
  /// A panic on `wasm32-unknown-unknown` cannot be caught and unwound,
  /// so a panicking `spawn_blocking` task takes down the web worker that
  /// runs it. That is reported here, but the panic payload is lost and
  /// [`into_panic`] has no equivalent in this crate.
  ///
  /// [`into_panic`]: https://docs.rs/tokio/latest/tokio/task/struct.JoinError.html#method.into_panic
  pub fn is_panic(&self) -> bool {
    self.panicked
  }
}

/// An owned permission to abort a spawned task, without awaiting its completion.
///
/// Unlike a [`JoinHandle`], an `AbortHandle` does *not* represent the
/// permission to await the task's completion, only to terminate it.
///
/// The task may be aborted by calling the [`AbortHandle::abort`] method.
/// Dropping an `AbortHandle` releases the permission to terminate the task
/// --- it does *not* abort the task.
///
/// Be aware that tasks spawned using [`spawn_blocking`] cannot be aborted
/// because they are not async. If you call `abort` on a `spawn_blocking` task,
/// then this *will not have any effect*, and the task will continue running
/// normally. The exception is if the task has not started running yet; in that
/// case, calling `abort` may prevent the task from starting.
///
/// [`JoinHandle`]: crate::task::JoinHandle
/// [`spawn_blocking`]: crate::task::spawn_blocking
#[derive(Clone)]
pub struct AbortHandle {
  cancel_sender: OnceSender<()>,
}

impl AbortHandle {
  /// Abort the task associated with the handle.
  ///
  /// Awaiting a cancelled task might complete as usual if the task was
  /// already completed at the time it was cancelled, but most likely it
  /// will fail with a [`JoinError`] that reports itself as cancelled.
  ///
  /// If the task was already cancelled, such as by [`JoinHandle::abort`],
  /// this method will do nothing.
  ///
  /// Be aware that tasks spawned using [`spawn_blocking`] cannot be aborted
  /// because they are not async. If you call `abort` on a `spawn_blocking`
  /// task, then this *will not have any effect*, and the task will continue
  /// running normally. The exception is if the task has not started running
  /// yet; in that case, calling `abort` may prevent the task from starting.
  pub fn abort(&self) {
    self.cancel_sender.send(());
  }
}

impl Debug for AbortHandle {
  fn fmt(&self, fmt: &mut Formatter<'_>) -> std::fmt::Result {
    fmt.debug_struct("AbortHandle").finish()
  }
}
