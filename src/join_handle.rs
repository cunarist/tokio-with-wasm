use futures_channel::oneshot;
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// An owned permission to join on a task (awaiting its termination).
///
/// This can be thought of as the equivalent of
/// [`std::thread::JoinHandle`] or [`tokio::task::JoinHandle`] for
/// a task that is executed concurrently.
///
/// A `JoinHandle` *detaches* the associated task when it is dropped, which
/// means that there is no longer any handle to the task, and no way to `join`
/// on it.
///
/// This struct is created by the [crate::spawn] and [crate::spawn_blocking]
/// functions.
///
/// # Examples
///
/// Creation from [`crate::spawn`]:
///
/// ```
/// # async fn doc() {
/// let join_handle: async_wasm_task::JoinHandle<_> = async_wasm_task::spawn(async {
///     // some work here
/// });
/// # }
/// ```
///
/// Creation from [`crate::spawn_blocking`]:
///
/// ```
/// # async fn doc() {
/// let join_handle: async_wasm_task::JoinHandle<_> = async_wasm_task::spawn_blocking(|| {
///     // some blocking work here
/// });
/// # }
/// ```
///
/// Child being detached and outliving its parent:
///
/// ```no_run
/// # async fn start() {
/// let original_task = async_wasm_task::spawn(async {
///     let _detached_task = async_wasm_task::spawn(async {
///         // Here we sleep to make sure that the first task returns before.
///         // Assume that code takes a few seconds to execute here.
///         // This will be called, even though the JoinHandle is dropped.
///         println!("♫ Still alive ♫");
///     });
/// });
///
/// original_task.await.expect("The task being joined has panicked");
/// println!("Original task is joined.");
/// # }
/// ```
pub struct JoinHandle<T> {
    receiver: oneshot::Receiver<T>,
}

unsafe impl<T: Send> Send for JoinHandle<T> {}
unsafe impl<T: Send> Sync for JoinHandle<T> {}
impl<T> Unpin for JoinHandle<T> {}

impl<T> JoinHandle<T> {
    pub(super) fn new(receiver: oneshot::Receiver<T>) -> JoinHandle<T> {
        JoinHandle { receiver }
    }
}

impl<T> Future for JoinHandle<T> {
    type Output = Result<T, JoinError>;
    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        if let Ok(received) = self.receiver.try_recv() {
            if let Some(payload) = received {
                Poll::Ready(Ok(payload))
            } else {
                let waker = context.waker().clone();
                let wake_future = async move {
                    waker.wake();
                };
                wasm_bindgen_futures::spawn_local(wake_future);
                Poll::Pending
            }
        } else {
            Poll::Ready(Err(JoinError))
        }
    }
}

impl<T> fmt::Debug for JoinHandle<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_struct("JoinHandle").finish()
    }
}

#[derive(Debug)]
pub struct JoinError;
