use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard};
use std::task::{Context, Poll, Waker};

/// Creates a channel that carries a single value.
/// Both halves are `Send`, so a value can be sent
/// from a web worker back to the main thread.
pub fn once_channel<T>() -> (OnceSender<T>, OnceReceiver<T>) {
  let core = Arc::new(Mutex::new(ChannelCore {
    value: None,
    waker: None,
    sent: false,
  }));
  let sender = OnceSender { core: core.clone() };
  let receiver = OnceReceiver { core };
  (sender, receiver)
}

struct ChannelCore<T> {
  /// The sent value, until the receiver takes it.
  value: Option<T>,
  /// Waker for the task waiting on the value.
  waker: Option<Waker>,
  /// Indicates that the value has been sent.
  /// This stays `true` after the value is taken.
  sent: bool,
}

/// Locks the shared state, recovering it
/// even if another thread panicked while holding the lock.
/// A poisoned lock would otherwise make the channel hang forever.
fn lock<T>(core: &Mutex<ChannelCore<T>>) -> MutexGuard<'_, ChannelCore<T>> {
  core.lock().unwrap_or_else(|error| error.into_inner())
}

pub struct OnceSender<T> {
  core: Arc<Mutex<ChannelCore<T>>>,
}

// Written by hand, because deriving `Clone`
// would require the value type to be cloneable as well.
impl<T> Clone for OnceSender<T> {
  fn clone(&self) -> Self {
    OnceSender {
      core: self.core.clone(),
    }
  }
}

impl<T> OnceSender<T> {
  /// Sends the value, waking the receiver if it is waiting.
  /// Only the first call has an effect.
  pub fn send(&self, value: T) {
    let waker = {
      let mut core = lock(&self.core);
      if core.sent {
        return;
      }
      core.sent = true;
      core.value = Some(value);
      core.waker.take()
    };
    // Wake after releasing the lock,
    // because waking can poll the receiver again on this thread.
    if let Some(waker) = waker {
      waker.wake();
    }
  }
}

pub struct OnceReceiver<T> {
  core: Arc<Mutex<ChannelCore<T>>>,
}

impl<T> OnceReceiver<T> {
  /// Returns whether the value has been sent,
  /// whether or not it has been received yet.
  pub fn is_done(&self) -> bool {
    lock(&self.core).sent
  }
}

impl<T> Future for OnceReceiver<T> {
  type Output = T;
  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    let mut core = lock(&self.core);
    if let Some(value) = core.value.take() {
      return Poll::Ready(value);
    }
    // The waker is registered while the lock is still held,
    // so a value sent from another thread cannot slip in between
    // the check above and the registration below.
    core.waker = Some(cx.waker().clone());
    Poll::Pending
  }
}
