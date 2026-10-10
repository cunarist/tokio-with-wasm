use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use tokio::sync::Notify;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

struct Flag(AtomicBool);

impl Wake for Flag {
  fn wake(self: Arc<Self>) {
    self.0.store(true, Ordering::SeqCst);
  }
}

pub struct Task<F> {
  fut: Pin<Box<F>>,
  flag: Arc<Flag>,
}

pub fn spawn<F: Future>(fut: F) -> Task<F> {
  Task {
    fut: Box::pin(fut),
    flag: Arc::new(Flag(AtomicBool::new(false))),
  }
}

impl<F: Future> Task<F> {
  pub fn poll(&mut self) -> Poll<F::Output> {
    self.flag.0.store(false, Ordering::SeqCst);
    let waker = Waker::from(self.flag.clone());
    self.fut.as_mut().poll(&mut Context::from_waker(&waker))
  }

  pub fn is_woken(&self) -> bool {
    self.flag.0.load(Ordering::SeqCst)
  }

  pub fn enter<R>(&mut self, f: impl FnOnce(Pin<&mut F>) -> R) -> R {
    f(self.fut.as_mut())
  }
}

#[wasm_bindgen_test]
fn notify_notified_one() {
  let notify = Notify::new();
  let mut notified = spawn(async { notify.notified().await });

  notify.notify_one();
  assert!(notified.poll().is_ready());
}

#[wasm_bindgen_test]
fn notify_multi_notified_one() {
  let notify = Notify::new();
  let mut notified1 = spawn(async { notify.notified().await });
  let mut notified2 = spawn(async { notify.notified().await });

  assert!(notified1.poll().is_pending());
  assert!(notified2.poll().is_pending());

  notify.notify_one();
  assert!(notified1.poll().is_ready());
  assert!(notified2.poll().is_pending());
}

#[wasm_bindgen_test]
fn notify_multi_notified_last() {
  let notify = Notify::new();
  let mut notified1 = spawn(async { notify.notified().await });
  let mut notified2 = spawn(async { notify.notified().await });

  assert!(notified1.poll().is_pending());
  assert!(notified2.poll().is_pending());

  notify.notify_last();
  assert!(notified1.poll().is_pending());
  assert!(notified2.poll().is_ready());
}

#[wasm_bindgen_test]
fn notified_one_notify() {
  let notify = Notify::new();
  let mut notified = spawn(async { notify.notified().await });

  assert!(notified.poll().is_pending());

  notify.notify_one();
  assert!(notified.is_woken());
  assert!(notified.poll().is_ready());
}

#[wasm_bindgen_test]
fn notified_multi_notify() {
  let notify = Notify::new();
  let mut notified1 = spawn(async { notify.notified().await });
  let mut notified2 = spawn(async { notify.notified().await });

  assert!(notified1.poll().is_pending());
  assert!(notified2.poll().is_pending());

  notify.notify_one();
  assert!(notified1.is_woken());
  assert!(!notified2.is_woken());

  assert!(notified1.poll().is_ready());
  assert!(notified2.poll().is_pending());
}

#[wasm_bindgen_test]
fn notify_notified_multi() {
  let notify = Notify::new();

  notify.notify_one();

  let mut notified1 = spawn(async { notify.notified().await });
  let mut notified2 = spawn(async { notify.notified().await });

  assert!(notified1.poll().is_ready());
  assert!(notified2.poll().is_pending());

  notify.notify_one();

  assert!(notified2.is_woken());
  assert!(notified2.poll().is_ready());
}

#[wasm_bindgen_test]
fn notified_drop_notified_notify() {
  let notify = Notify::new();
  let mut notified1 = spawn(async { notify.notified().await });
  let mut notified2 = spawn(async { notify.notified().await });

  assert!(notified1.poll().is_pending());

  drop(notified1);

  assert!(notified2.poll().is_pending());

  notify.notify_one();
  assert!(notified2.is_woken());
  assert!(notified2.poll().is_ready());
}

#[wasm_bindgen_test]
fn notified_multi_notify_drop_one() {
  let notify = Notify::new();
  let mut notified1 = spawn(async { notify.notified().await });
  let mut notified2 = spawn(async { notify.notified().await });

  assert!(notified1.poll().is_pending());
  assert!(notified2.poll().is_pending());

  notify.notify_one();

  assert!(notified1.is_woken());
  assert!(!notified2.is_woken());

  drop(notified1);

  assert!(notified2.is_woken());
  assert!(notified2.poll().is_ready());
}

#[wasm_bindgen_test]
fn notified_multi_notify_one_drop() {
  let notify = Notify::new();
  let mut notified1 = spawn(async { notify.notified().await });
  let mut notified2 = spawn(async { notify.notified().await });
  let mut notified3 = spawn(async { notify.notified().await });

  assert!(notified1.poll().is_pending());
  assert!(notified2.poll().is_pending());
  assert!(notified3.poll().is_pending());

  notify.notify_one();

  drop(notified1);

  assert!(notified2.poll().is_ready());
  assert!(notified3.poll().is_pending());
}

#[wasm_bindgen_test]
fn notified_multi_notify_last_drop() {
  let notify = Notify::new();
  let mut notified1 = spawn(async { notify.notified().await });
  let mut notified2 = spawn(async { notify.notified().await });
  let mut notified3 = spawn(async { notify.notified().await });

  assert!(notified1.poll().is_pending());
  assert!(notified2.poll().is_pending());
  assert!(notified3.poll().is_pending());

  notify.notify_last();

  drop(notified3);

  assert!(notified2.poll().is_ready());
  assert!(notified1.poll().is_pending());
}

#[wasm_bindgen_test]
fn notify_in_drop_after_wake() {
  let notify = Arc::new(Notify::new());

  struct NotifyOnDrop(Arc<Notify>);

  impl Wake for NotifyOnDrop {
    fn wake(self: Arc<Self>) {}
  }

  impl Drop for NotifyOnDrop {
    fn drop(&mut self) {
      self.0.notify_waiters();
    }
  }

  let mut fut = Box::pin(async {
    notify.notified().await;
  });

  {
    let waker = Waker::from(Arc::new(NotifyOnDrop(notify.clone())));
    let mut cx = std::task::Context::from_waker(&waker);
    assert!(fut.as_mut().poll(&mut cx).is_pending());
  }

  notify.notify_waiters();
}

#[wasm_bindgen_test]
fn notify_one_after_dropped_all() {
  let notify = Notify::new();
  let mut notified1 = spawn(async { notify.notified().await });

  assert!(notified1.poll().is_pending());

  notify.notify_waiters();
  notify.notify_one();

  drop(notified1);

  let mut notified2 = spawn(async { notify.notified().await });

  assert!(notified2.poll().is_ready());
}

#[wasm_bindgen_test]
fn test_notify_one_not_enabled() {
  let notify = Notify::new();
  let mut future = spawn(notify.notified());

  notify.notify_one();
  assert!(future.poll().is_ready());
}

#[wasm_bindgen_test]
fn test_notify_one_after_enable() {
  let notify = Notify::new();
  let mut future = spawn(notify.notified());

  future.enter(|fut| assert!(!fut.enable()));

  notify.notify_one();
  assert!(future.poll().is_ready());
  future.enter(|fut| assert!(fut.enable()));
}

#[wasm_bindgen_test]
fn test_poll_after_enable() {
  let notify = Notify::new();
  let mut future = spawn(notify.notified());

  future.enter(|fut| assert!(!fut.enable()));
  assert!(future.poll().is_pending());
}

#[wasm_bindgen_test]
fn test_enable_after_poll() {
  let notify = Notify::new();
  let mut future = spawn(notify.notified());

  assert!(future.poll().is_pending());
  future.enter(|fut| assert!(!fut.enable()));
}

#[wasm_bindgen_test]
fn test_enable_consumes_permit() {
  let notify = Notify::new();

  notify.notify_one();

  let mut future1 = spawn(notify.notified());
  future1.enter(|fut| assert!(fut.enable()));

  let mut future2 = spawn(notify.notified());
  future2.enter(|fut| assert!(!fut.enable()));
}

#[wasm_bindgen_test]
fn test_waker_update() {
  let notify = Notify::new();
  let mut future = spawn(notify.notified());

  let noop = Waker::noop();
  future.enter(|fut| assert!(fut.poll(&mut Context::from_waker(noop)).is_pending()));

  assert!(future.poll().is_pending());
  notify.notify_one();

  assert!(future.is_woken());
}

#[wasm_bindgen_test]
fn unpolled_future_completed_by_notify_waiters_preserves_notify_one_permit() {
  let notify = Notify::new();
  let notified1 = notify.notified();
  notify.notify_waiters();
  notify.notify_one();
  assert!(spawn(notified1).poll().is_ready());
  let notified2 = notify.notified();
  assert!(spawn(notified2).poll().is_ready());
}

#[wasm_bindgen_test]
async fn notify_one_from_a_web_worker() {
  let notify = Arc::new(Notify::new());
  let worker_notify = notify.clone();
  let handle = tokio::task::spawn_blocking(move || worker_notify.notify_one());
  notify.notified().await;
  handle.await.unwrap();
}

#[wasm_bindgen_test]
async fn notify_waiters_from_a_web_worker() {
  let notify = Arc::new(Notify::new());
  let waiter = tokio::spawn({
    let notify = notify.clone();
    async move { notify.notified().await }
  });
  tokio::task::yield_now().await;
  tokio::task::spawn_blocking(move || notify.notify_waiters())
    .await
    .unwrap();
  waiter.await.unwrap();
}
