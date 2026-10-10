use super::sync_notify::spawn;
use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Wake, Waker};
use tokio::sync::Notify;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn notify_notified_one() {
  let notify = Arc::new(Notify::new());
  let mut notified = spawn(async { notify.clone().notified_owned().await });

  notify.notify_one();
  assert!(notified.poll().is_ready());
}

#[wasm_bindgen_test]
fn notify_multi_notified_one() {
  let notify = Arc::new(Notify::new());
  let mut notified1 = spawn(async { notify.clone().notified_owned().await });
  let mut notified2 = spawn(async { notify.clone().notified_owned().await });

  assert!(notified1.poll().is_pending());
  assert!(notified2.poll().is_pending());

  notify.notify_one();
  assert!(notified1.poll().is_ready());
  assert!(notified2.poll().is_pending());
}

#[wasm_bindgen_test]
fn notify_multi_notified_last() {
  let notify = Arc::new(Notify::new());
  let mut notified1 = spawn(async { notify.clone().notified_owned().await });
  let mut notified2 = spawn(async { notify.clone().notified_owned().await });

  assert!(notified1.poll().is_pending());
  assert!(notified2.poll().is_pending());

  notify.notify_last();
  assert!(notified1.poll().is_pending());
  assert!(notified2.poll().is_ready());
}

#[wasm_bindgen_test]
fn notified_one_notify() {
  let notify = Arc::new(Notify::new());
  let mut notified = spawn(async { notify.clone().notified_owned().await });

  assert!(notified.poll().is_pending());

  notify.notify_one();
  assert!(notified.is_woken());
  assert!(notified.poll().is_ready());
}

#[wasm_bindgen_test]
fn notified_multi_notify() {
  let notify = Arc::new(Notify::new());
  let mut notified1 = spawn(async { notify.clone().notified_owned().await });
  let mut notified2 = spawn(async { notify.clone().notified_owned().await });

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
  let notify = Arc::new(Notify::new());

  notify.notify_one();

  let mut notified1 = spawn(async { notify.clone().notified_owned().await });
  let mut notified2 = spawn(async { notify.clone().notified_owned().await });

  assert!(notified1.poll().is_ready());
  assert!(notified2.poll().is_pending());

  notify.notify_one();

  assert!(notified2.is_woken());
  assert!(notified2.poll().is_ready());
}

#[wasm_bindgen_test]
fn notified_drop_notified_notify() {
  let notify = Arc::new(Notify::new());
  let mut notified1 = spawn(async { notify.clone().notified_owned().await });
  let mut notified2 = spawn(async { notify.clone().notified_owned().await });

  assert!(notified1.poll().is_pending());

  drop(notified1);

  assert!(notified2.poll().is_pending());

  notify.notify_one();
  assert!(notified2.is_woken());
  assert!(notified2.poll().is_ready());
}

#[wasm_bindgen_test]
fn notified_multi_notify_drop_one() {
  let notify = Arc::new(Notify::new());
  let mut notified1 = spawn(async { notify.clone().notified_owned().await });
  let mut notified2 = spawn(async { notify.clone().notified_owned().await });

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
  let notify = Arc::new(Notify::new());
  let mut notified1 = spawn(async { notify.clone().notified_owned().await });
  let mut notified2 = spawn(async { notify.clone().notified_owned().await });
  let mut notified3 = spawn(async { notify.clone().notified_owned().await });

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
  let notify = Arc::new(Notify::new());
  let mut notified1 = spawn(async { notify.clone().notified_owned().await });
  let mut notified2 = spawn(async { notify.clone().notified_owned().await });
  let mut notified3 = spawn(async { notify.clone().notified_owned().await });

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
    notify.clone().notified_owned().await;
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
  let notify = Arc::new(Notify::new());
  let mut notified1 = spawn(async { notify.clone().notified_owned().await });

  assert!(notified1.poll().is_pending());

  notify.notify_waiters();
  notify.notify_one();

  drop(notified1);

  let mut notified2 = spawn(async { notify.clone().notified_owned().await });

  assert!(notified2.poll().is_ready());
}

#[wasm_bindgen_test]
fn test_notify_one_not_enabled() {
  let notify = Arc::new(Notify::new());
  let mut future = spawn(notify.clone().notified_owned());

  notify.notify_one();
  assert!(future.poll().is_ready());
}

#[wasm_bindgen_test]
fn test_notify_one_after_enable() {
  let notify = Arc::new(Notify::new());
  let mut future = spawn(notify.clone().notified_owned());

  future.enter(|fut| assert!(!fut.enable()));

  notify.notify_one();
  assert!(future.poll().is_ready());
  future.enter(|fut| assert!(fut.enable()));
}

#[wasm_bindgen_test]
fn test_poll_after_enable() {
  let notify = Arc::new(Notify::new());
  let mut future = spawn(notify.clone().notified_owned());

  future.enter(|fut| assert!(!fut.enable()));
  assert!(future.poll().is_pending());
}

#[wasm_bindgen_test]
fn test_enable_after_poll() {
  let notify = Arc::new(Notify::new());
  let mut future = spawn(notify.clone().notified_owned());

  assert!(future.poll().is_pending());
  future.enter(|fut| assert!(!fut.enable()));
}

#[wasm_bindgen_test]
fn test_enable_consumes_permit() {
  let notify = Arc::new(Notify::new());

  notify.notify_one();

  let mut future1 = spawn(notify.clone().notified_owned());
  future1.enter(|fut| assert!(fut.enable()));

  let mut future2 = spawn(notify.clone().notified_owned());
  future2.enter(|fut| assert!(!fut.enable()));
}

#[wasm_bindgen_test]
fn test_waker_update() {
  let notify = Arc::new(Notify::new());
  let mut future = spawn(notify.clone().notified_owned());

  let noop = Waker::noop();
  future.enter(|fut| assert!(fut.poll(&mut Context::from_waker(noop)).is_pending()));

  assert!(future.poll().is_pending());
  notify.notify_one();

  assert!(future.is_woken());
}

#[wasm_bindgen_test]
async fn notify_one_from_a_web_worker() {
  let notify = Arc::new(Notify::new());
  let worker_notify = notify.clone();
  let handle = tokio::task::spawn_blocking(move || worker_notify.notify_one());
  notify.notified_owned().await;
  handle.await.unwrap();
}
