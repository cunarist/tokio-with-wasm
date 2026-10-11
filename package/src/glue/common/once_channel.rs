use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

pub fn once_channel<T>() -> (OnceSender<T>, OnceReceiver<T>) {
  let notified = Arc::new(AtomicBool::new(false));
  let value = Arc::new(Mutex::new(None));
  let waker = Arc::new(Mutex::new(None));

  let sender = OnceSender {
    notified: notified.clone(),
    value: value.clone(),
    waker: waker.clone(),
  };
  let receiver = OnceReceiver {
    notified,
    value,
    waker,
  };

  (sender, receiver)
}

#[derive(Clone)]
pub struct OnceSender<T> {
  notified: Arc<AtomicBool>,
  value: Arc<Mutex<Option<T>>>,
  waker: Arc<Mutex<Option<Waker>>>,
}

impl<T> OnceSender<T> {
  pub fn send(&self, value: T) {
    if let Ok(mut guard) = self.value.lock() {
      guard.replace(value);
    }
    self.notified.store(true, Ordering::SeqCst);
    let waker = self.waker.lock().ok().and_then(|mut guard| guard.take());
    if let Some(waker) = waker {
      waker.wake();
    }
  }
}

pub struct OnceReceiver<T> {
  notified: Arc<AtomicBool>,
  value: Arc<Mutex<Option<T>>>,
  waker: Arc<Mutex<Option<Waker>>>,
}

impl<T> OnceReceiver<T> {
  pub fn is_done(&self) -> bool {
    self.notified.load(Ordering::SeqCst)
  }
}

impl<T> Future for OnceReceiver<T> {
  type Output = T;
  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    // Store the waker before checking, so a send is never missed. Only try,
    // as the main thread can't block; `send` holds it after `notified` is set.
    if let Ok(mut guard) = self.waker.try_lock() {
      guard.replace(cx.waker().clone());
    }
    if self.notified.load(Ordering::SeqCst) {
      if let Ok(mut guard) = self.value.lock() {
        if let Some(value) = guard.take() {
          return Poll::Ready(value);
        }
      }
    }
    Poll::Pending
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::glue::common::tests::counting_waker;
  use wasm_bindgen_test::wasm_bindgen_test;

  #[wasm_bindgen_test]
  fn delivers_sent_value() {
    let (tx, mut rx) = once_channel();
    assert!(!rx.is_done());
    tx.send(1);
    assert!(rx.is_done());
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(Pin::new(&mut rx).poll(&mut cx), Poll::Ready(1));
  }

  #[wasm_bindgen_test]
  fn send_wakes_receiver() {
    let (tx, mut rx) = once_channel();
    let (waker, count) = counting_waker();
    let mut cx = Context::from_waker(&waker);
    assert!(Pin::new(&mut rx).poll(&mut cx).is_pending());
    tx.send(1);
    assert_eq!(count.get(), 1);
    assert_eq!(Pin::new(&mut rx).poll(&mut cx), Poll::Ready(1));
  }

  #[wasm_bindgen_test]
  fn cloned_sender_delivers() {
    let (tx, mut rx) = once_channel();
    let tx2 = tx.clone();
    drop(tx);
    tx2.send(2);
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(Pin::new(&mut rx).poll(&mut cx), Poll::Ready(2));
  }

  #[wasm_bindgen_test]
  fn poll_skips_a_held_waker_lock() {
    let (tx, mut rx) = once_channel();
    let _sending = tx.waker.lock().unwrap();
    tx.value.lock().unwrap().replace(1);
    tx.notified.store(true, Ordering::SeqCst);
    let mut cx = Context::from_waker(Waker::noop());
    assert_eq!(Pin::new(&mut rx).poll(&mut cx), Poll::Ready(1));
  }
}
