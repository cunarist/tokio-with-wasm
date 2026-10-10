use crate::support::{assert_pending, assert_ready, spawn};
use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Wake, Waker};
use tokio::sync::Notify;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn notify_notified_one() {
  let notify = Notify::new();
  let mut notified = spawn(async { notify.notified().await });

  notify.notify_one();
  assert_ready!(notified.poll());
}

#[wasm_bindgen_test]
fn notify_multi_notified_one() {
  let notify = Notify::new();
  let mut notified1 = spawn(async { notify.notified().await });
  let mut notified2 = spawn(async { notify.notified().await });

  assert_pending!(notified1.poll());
  assert_pending!(notified2.poll());

  notify.notify_one();
  assert_ready!(notified1.poll());
  assert_pending!(notified2.poll());
}

#[wasm_bindgen_test]
fn notify_multi_notified_last() {
  let notify = Notify::new();
  let mut notified1 = spawn(async { notify.notified().await });
  let mut notified2 = spawn(async { notify.notified().await });

  assert_pending!(notified1.poll());
  assert_pending!(notified2.poll());

  notify.notify_last();
  assert_pending!(notified1.poll());
  assert_ready!(notified2.poll());
}

#[wasm_bindgen_test]
fn notified_one_notify() {
  let notify = Notify::new();
  let mut notified = spawn(async { notify.notified().await });

  assert_pending!(notified.poll());

  notify.notify_one();
  assert!(notified.is_woken());
  assert_ready!(notified.poll());
}

#[wasm_bindgen_test]
fn notified_multi_notify() {
  let notify = Notify::new();
  let mut notified1 = spawn(async { notify.notified().await });
  let mut notified2 = spawn(async { notify.notified().await });

  assert_pending!(notified1.poll());
  assert_pending!(notified2.poll());

  notify.notify_one();
  assert!(notified1.is_woken());
  assert!(!notified2.is_woken());

  assert_ready!(notified1.poll());
  assert_pending!(notified2.poll());
}

#[wasm_bindgen_test]
fn notify_notified_multi() {
  let notify = Notify::new();

  notify.notify_one();

  let mut notified1 = spawn(async { notify.notified().await });
  let mut notified2 = spawn(async { notify.notified().await });

  assert_ready!(notified1.poll());
  assert_pending!(notified2.poll());

  notify.notify_one();

  assert!(notified2.is_woken());
  assert_ready!(notified2.poll());
}

#[wasm_bindgen_test]
fn notified_drop_notified_notify() {
  let notify = Notify::new();
  let mut notified1 = spawn(async { notify.notified().await });
  let mut notified2 = spawn(async { notify.notified().await });

  assert_pending!(notified1.poll());

  drop(notified1);

  assert_pending!(notified2.poll());

  notify.notify_one();
  assert!(notified2.is_woken());
  assert_ready!(notified2.poll());
}

#[wasm_bindgen_test]
fn notified_multi_notify_drop_one() {
  let notify = Notify::new();
  let mut notified1 = spawn(async { notify.notified().await });
  let mut notified2 = spawn(async { notify.notified().await });

  assert_pending!(notified1.poll());
  assert_pending!(notified2.poll());

  notify.notify_one();

  assert!(notified1.is_woken());
  assert!(!notified2.is_woken());

  drop(notified1);

  assert!(notified2.is_woken());
  assert_ready!(notified2.poll());
}

#[wasm_bindgen_test]
fn notified_multi_notify_one_drop() {
  let notify = Notify::new();
  let mut notified1 = spawn(async { notify.notified().await });
  let mut notified2 = spawn(async { notify.notified().await });
  let mut notified3 = spawn(async { notify.notified().await });

  assert_pending!(notified1.poll());
  assert_pending!(notified2.poll());
  assert_pending!(notified3.poll());

  notify.notify_one();

  drop(notified1);

  assert_ready!(notified2.poll());
  assert_pending!(notified3.poll());
}

#[wasm_bindgen_test]
fn notified_multi_notify_last_drop() {
  let notify = Notify::new();
  let mut notified1 = spawn(async { notify.notified().await });
  let mut notified2 = spawn(async { notify.notified().await });
  let mut notified3 = spawn(async { notify.notified().await });

  assert_pending!(notified1.poll());
  assert_pending!(notified2.poll());
  assert_pending!(notified3.poll());

  notify.notify_last();

  drop(notified3);

  assert_ready!(notified2.poll());
  assert_pending!(notified1.poll());
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
    assert_pending!(fut.as_mut().poll(&mut cx));
  }

  notify.notify_waiters();
}

#[wasm_bindgen_test]
fn notify_one_after_dropped_all() {
  let notify = Notify::new();
  let mut notified1 = spawn(async { notify.notified().await });

  assert_pending!(notified1.poll());

  notify.notify_waiters();
  notify.notify_one();

  drop(notified1);

  let mut notified2 = spawn(async { notify.notified().await });

  assert_ready!(notified2.poll());
}

#[wasm_bindgen_test]
fn test_notify_one_not_enabled() {
  let notify = Notify::new();
  let mut future = spawn(notify.notified());

  notify.notify_one();
  assert_ready!(future.poll());
}

#[wasm_bindgen_test]
fn test_notify_one_after_enable() {
  let notify = Notify::new();
  let mut future = spawn(notify.notified());

  future.enter(|_, fut| assert!(!fut.enable()));

  notify.notify_one();
  assert_ready!(future.poll());
  future.enter(|_, fut| assert!(fut.enable()));
}

#[wasm_bindgen_test]
fn test_poll_after_enable() {
  let notify = Notify::new();
  let mut future = spawn(notify.notified());

  future.enter(|_, fut| assert!(!fut.enable()));
  assert_pending!(future.poll());
}

#[wasm_bindgen_test]
fn test_enable_after_poll() {
  let notify = Notify::new();
  let mut future = spawn(notify.notified());

  assert_pending!(future.poll());
  future.enter(|_, fut| assert!(!fut.enable()));
}

#[wasm_bindgen_test]
fn test_enable_consumes_permit() {
  let notify = Notify::new();

  notify.notify_one();

  let mut future1 = spawn(notify.notified());
  future1.enter(|_, fut| assert!(fut.enable()));

  let mut future2 = spawn(notify.notified());
  future2.enter(|_, fut| assert!(!fut.enable()));
}

#[wasm_bindgen_test]
fn test_waker_update() {
  let notify = Notify::new();
  let mut future = spawn(notify.notified());

  let noop = Waker::noop();
  future
    .enter(|_, fut| assert_pending!(fut.poll(&mut Context::from_waker(noop))));

  assert_pending!(future.poll());
  notify.notify_one();

  assert!(future.is_woken());
}

#[wasm_bindgen_test]
fn unpolled_future_completed_by_notify_waiters_preserves_notify_one_permit() {
  let notify = Notify::new();
  let notified1 = notify.notified();
  notify.notify_waiters();
  notify.notify_one();
  assert_ready!(spawn(notified1).poll());
  let notified2 = notify.notified();
  assert_ready!(spawn(notified2).poll());
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
