use crate::assert_elapsed;
use std::future::{Future, pending};
use std::io;
use std::pin::pin;
use std::task::{Context, Poll, Waker};
use std::thread;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::task;
use tokio::time::timeout;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn simultaneous_deadline_future_completion() {
  let fut = pin!(timeout(Duration::ZERO, async {}));
  let mut cx = Context::from_waker(Waker::noop());
  assert!(matches!(fut.poll(&mut cx), Poll::Ready(Ok(()))));
}

#[wasm_bindgen_test]
fn future_and_timeout_in_future() {
  let (tx, rx) = oneshot::channel();
  let mut fut = pin!(timeout(Duration::from_millis(100), rx));
  let mut cx = Context::from_waker(Waker::noop());
  assert!(fut.as_mut().poll(&mut cx).is_pending());
  tx.send(()).unwrap();
  assert!(matches!(fut.poll(&mut cx), Poll::Ready(Ok(Ok(())))));
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
  let slow = task::spawn_blocking(|| thread::sleep(Duration::from_millis(500)));
  assert!(timeout(Duration::from_millis(10), slow).await.is_err());
}

#[wasm_bindgen_test]
async fn elapsed_into_io_error() {
  let elapsed = timeout(Duration::ZERO, pending::<()>()).await.unwrap_err();
  assert_eq!(io::Error::from(elapsed).kind(), io::ErrorKind::TimedOut);
}
