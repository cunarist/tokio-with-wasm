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

#[cfg(test)]
mod tests {
  use super::super::test_util::CountingWaker;
  use super::*;
  use std::task::Context;
  use wasm_bindgen_test::wasm_bindgen_test;

  #[wasm_bindgen_test]
  fn value_sent_before_polling_is_ready() {
    let (sender, mut receiver) = once_channel();
    sender.send(5);
    let counter = CountingWaker::new();
    let waker = counter.waker();
    let mut cx = Context::from_waker(&waker);
    let polled = Pin::new(&mut receiver).poll(&mut cx);
    assert_eq!(polled, Poll::Ready(5));
  }

  #[wasm_bindgen_test]
  fn send_wakes_the_registered_waker_once() {
    let (sender, mut receiver) = once_channel();
    let counter = CountingWaker::new();
    let waker = counter.waker();
    let mut cx = Context::from_waker(&waker);

    assert_eq!(Pin::new(&mut receiver).poll(&mut cx), Poll::Pending);
    assert_eq!(counter.count(), 0);

    sender.send(7);
    assert_eq!(counter.count(), 1);
    assert_eq!(Pin::new(&mut receiver).poll(&mut cx), Poll::Ready(7));
  }

  #[wasm_bindgen_test]
  fn is_done_stays_true_after_the_value_is_taken() {
    let (sender, mut receiver) = once_channel();
    assert!(!receiver.is_done());
    sender.send(1);
    assert!(receiver.is_done());

    let counter = CountingWaker::new();
    let waker = counter.waker();
    let mut cx = Context::from_waker(&waker);
    let _ = Pin::new(&mut receiver).poll(&mut cx);
    assert!(receiver.is_done());
  }

  #[wasm_bindgen_test]
  fn only_the_first_send_delivers() {
    let (sender, mut receiver) = once_channel();
    sender.send(1);
    sender.send(2);
    let counter = CountingWaker::new();
    let waker = counter.waker();
    let mut cx = Context::from_waker(&waker);
    assert_eq!(Pin::new(&mut receiver).poll(&mut cx), Poll::Ready(1));
  }

  #[wasm_bindgen_test]
  fn cloned_senders_share_the_single_slot() {
    let (sender, mut receiver) = once_channel();
    let cloned = sender.clone();
    sender.send(1);
    cloned.send(2);
    let counter = CountingWaker::new();
    let waker = counter.waker();
    let mut cx = Context::from_waker(&waker);
    assert_eq!(Pin::new(&mut receiver).poll(&mut cx), Poll::Ready(1));
  }
}
