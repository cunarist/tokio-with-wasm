//! Stand-ins for the `tokio_test` items that the ported tests use.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::task::{Context, Poll, Wake, Waker};

/// Like `tokio_test::task::Spawn`: polls a future by hand, recording wakes.
pub struct Spawn<T> {
  future: Pin<Box<T>>,
  woken: Arc<Woken>,
}

struct Woken(AtomicBool);

impl Wake for Woken {
  fn wake(self: Arc<Self>) {
    self.0.store(true, SeqCst);
  }
}

pub fn spawn<T>(future: T) -> Spawn<T> {
  Spawn {
    future: Box::pin(future),
    woken: Arc::new(Woken(AtomicBool::new(false))),
  }
}

impl<T> Spawn<T> {
  pub fn enter<R>(
    &mut self,
    f: impl FnOnce(&mut Context<'_>, Pin<&mut T>) -> R,
  ) -> R {
    self.woken.0.store(false, SeqCst);
    let waker = Waker::from(self.woken.clone());
    f(&mut Context::from_waker(&waker), self.future.as_mut())
  }

  pub fn is_woken(&self) -> bool {
    self.woken.0.load(SeqCst)
  }

  pub fn waker_ref_count(&self) -> usize {
    Arc::strong_count(&self.woken)
  }
}

impl<T: Future> Spawn<T> {
  pub fn poll(&mut self) -> Poll<T::Output> {
    self.enter(|cx, future| future.poll(cx))
  }
}

macro_rules! assert_ok {
  ($e:expr) => {
    match $e {
      Ok(v) => v,
      Err(e) => panic!("error = {:?}", e),
    }
  };
}

macro_rules! assert_err {
  ($e:expr) => {
    match $e {
      Ok(v) => panic!("ok = {:?}", v),
      Err(e) => e,
    }
  };
}

macro_rules! assert_pending {
  ($e:expr) => {
    assert!($e.is_pending(), "ready")
  };
}

macro_rules! assert_ready {
  ($e:expr) => {
    match $e {
      std::task::Poll::Ready(v) => v,
      std::task::Poll::Pending => panic!("pending"),
    }
  };
}

macro_rules! assert_ready_ok {
  ($e:expr) => {
    $crate::support::assert_ok!($crate::support::assert_ready!($e))
  };
}

macro_rules! assert_ready_err {
  ($e:expr) => {
    $crate::support::assert_err!($crate::support::assert_ready!($e))
  };
}

pub(crate) use {
  assert_err, assert_ok, assert_pending, assert_ready, assert_ready_err,
  assert_ready_ok,
};

/// Asserts that `ms` milliseconds have passed since `start`, allowing for
/// the rounding of `Date.now`.
pub fn assert_elapsed(start: f64, ms: u64) {
  let elapsed = js_sys::Date::now() - start;
  assert!(
    elapsed + 1.0 >= ms as f64,
    "only {elapsed}ms of {ms}ms passed"
  );
}
