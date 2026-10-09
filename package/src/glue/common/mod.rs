#![allow(dead_code, unused_imports)]

mod completion_queue;
mod once_channel;
#[cfg(test)]
pub(crate) mod test_util;
mod thread_check;

pub use completion_queue::*;
pub use once_channel::*;
pub use thread_check::*;

use js_sys::Function;
use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::{Mutex, MutexGuard, TryLockError};
use std::task::{Context, Poll, Waker};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::{Closure, JsValue, wasm_bindgen};

/// Locks a mutex, taking the state inside even if the lock is poisoned.
/// It spins, because `Atomics.wait` throws on the browser main thread.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
  loop {
    match mutex.try_lock() {
      Ok(guard) => return guard,
      Err(TryLockError::Poisoned(error)) => return error.into_inner(),
      Err(TryLockError::WouldBlock) => std::hint::spin_loop(),
    }
  }
}

/// A `setTimeout` that wakes its task, cleared when dropped.
pub(crate) struct Timer {
  id: JsValue,
  /// Emptied when the timer fires; taking it out fires it early.
  pub waker: Rc<Cell<Option<Waker>>>,
  _callback: Closure<dyn FnMut()>,
}

impl Timer {
  pub fn new(milliseconds: f64) -> Timer {
    let waker = Rc::new(Cell::new(Some(Waker::noop().clone())));
    let callback = Closure::<dyn FnMut()>::new({
      let waker = waker.clone();
      move || {
        if let Some(waker) = waker.take() {
          waker.wake();
        }
      }
    });
    Timer {
      id: set_timeout(callback.as_ref().unchecked_ref(), milliseconds),
      waker,
      _callback: callback,
    }
  }
}

impl Future for Timer {
  type Output = ();
  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
    if self.waker.take().is_none() {
      return Poll::Ready(());
    }
    self.waker.set(Some(cx.waker().clone()));
    Poll::Pending
  }
}

impl Drop for Timer {
  fn drop(&mut self) {
    clear_timeout(&self.id);
  }
}

#[wasm_bindgen]
extern "C" {
  #[wasm_bindgen(js_namespace = console, js_name = error)]
  pub fn error(s: &str);
  #[wasm_bindgen(js_namespace = Date, js_name = now)]
  pub fn now() -> f64;
  #[wasm_bindgen(js_namespace = globalThis, js_name = setTimeout)]
  fn set_timeout(callback: &Function, milliseconds: f64) -> JsValue;
  #[wasm_bindgen(js_namespace = globalThis, js_name = clearTimeout)]
  fn clear_timeout(id: &JsValue);
}

pub trait LogError {
  fn log_error(&self, code: &str);
}

impl LogError for JsValue {
  fn log_error(&self, code: &str) {
    error(&format!("Error `{code}` in `tokio_with_wasm`:\n{self:?}"));
  }
}

impl<T> LogError for Result<T, JsValue> {
  fn log_error(&self, code: &str) {
    if let Err(js_value) = self {
      js_value.log_error(code);
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::task::{spawn_blocking, yield_now};
  use std::sync::atomic::{AtomicBool, Ordering};
  use wasm_bindgen_test::wasm_bindgen_test;

  /// `Mutex::lock` would call `Atomics.wait`, which throws here.
  #[wasm_bindgen_test]
  async fn the_main_thread_waits_out_a_contended_lock() {
    static MUTEX: Mutex<()> = Mutex::new(());
    static HELD: AtomicBool = AtomicBool::new(false);
    let worker = spawn_blocking(|| {
      let _guard = lock(&MUTEX);
      HELD.store(true, Ordering::SeqCst);
      std::thread::sleep(std::time::Duration::from_millis(100));
    });
    while !HELD.load(Ordering::SeqCst) {
      yield_now().await;
    }
    drop(lock(&MUTEX));
    assert!(worker.await.is_ok());
  }
}
