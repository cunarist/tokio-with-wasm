//! Helpers for the unit tests in this module.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Wake, Waker};

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

/// A waker that counts how many times it has been woken,
/// for asserting that a primitive wakes its consumer
/// exactly when it should.
pub(crate) struct CountingWaker {
  count: AtomicUsize,
}

impl CountingWaker {
  pub fn new() -> Arc<Self> {
    Arc::new(CountingWaker {
      count: AtomicUsize::new(0),
    })
  }

  pub fn count(&self) -> usize {
    self.count.load(Ordering::SeqCst)
  }

  pub fn waker(self: &Arc<Self>) -> Waker {
    Waker::from(self.clone())
  }
}

impl Wake for CountingWaker {
  fn wake(self: Arc<Self>) {
    self.count.fetch_add(1, Ordering::SeqCst);
  }

  fn wake_by_ref(self: &Arc<Self>) {
    self.count.fetch_add(1, Ordering::SeqCst);
  }
}
