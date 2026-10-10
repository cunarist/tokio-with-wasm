use crate::support::{assert_elapsed, assert_pending, assert_ready_ok, spawn};
use std::future::pending;
use std::io;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::task;
use tokio::time::timeout;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn simultaneous_deadline_future_completion() {
  let mut fut = spawn(timeout(Duration::ZERO, async {}));
  assert_ready_ok!(fut.poll());
}

#[wasm_bindgen_test]
fn future_and_timeout_in_future() {
  let (tx, rx) = oneshot::channel();
  let mut fut = spawn(timeout(Duration::from_millis(100), rx));
  assert_pending!(fut.poll());
  tx.send(()).unwrap();
  assert_ready_ok!(fut.poll()).unwrap();
}

#[wasm_bindgen_test]
async fn deadline_now_elapses() {
  assert!(timeout(Duration::ZERO, pending::<()>()).await.is_err());
}

#[wasm_bindgen_test]
async fn deadline_future_elapses() {
  let now = js_sys::Date::now();
  assert!(
    timeout(Duration::from_millis(30), pending::<()>())
      .await
      .is_err()
  );
  assert_elapsed(now, 30);
}

#[wasm_bindgen_test]
async fn timeout_is_not_exhausted_by_future() {
  let fut = timeout(Duration::from_millis(1), async {
    loop {
      task::yield_now().await;
    }
  });
  assert!(fut.await.is_err());
}

#[wasm_bindgen_test]
async fn nested_timeouts() {
  let inner = timeout(Duration::from_millis(10), pending::<()>());
  assert!(matches!(
    timeout(Duration::from_secs(10), inner).await,
    Ok(Err(_))
  ));
  let inner = timeout(Duration::from_secs(10), pending::<()>());
  assert!(timeout(Duration::from_millis(10), inner).await.is_err());
}

#[wasm_bindgen_test]
async fn timeout_around_spawn_blocking() {
  let fast = task::spawn_blocking(|| 42);
  assert_eq!(
    timeout(Duration::from_secs(10), fast)
      .await
      .unwrap()
      .unwrap(),
    42
  );
  let (tx, rx) = oneshot::channel::<()>();
  let slow = task::spawn_blocking(move || rx.blocking_recv());
  assert!(timeout(Duration::from_millis(10), slow).await.is_err());
  drop(tx);
}

#[wasm_bindgen_test]
async fn elapsed_into_io_error() {
  let elapsed = timeout(Duration::ZERO, pending::<()>()).await.unwrap_err();
  assert_eq!(io::Error::from(elapsed).kind(), io::ErrorKind::TimedOut);
}
