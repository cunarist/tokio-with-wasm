use crate::assert_elapsed;
use std::future::{Future, pending};
use std::pin::pin;
use std::task::{Context, Poll, Waker};
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
