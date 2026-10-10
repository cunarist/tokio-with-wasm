use std::future::pending;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn one_ready() {
  let (tx1, rx1) = oneshot::channel::<i32>();
  let (_tx2, rx2) = oneshot::channel::<i32>();
  tx1.send(1).unwrap();
  let v = tokio::select! {
    res = rx1 => res.unwrap(),
    _ = rx2 => unreachable!(),
  };
  assert_eq!(1, v);
}

#[wasm_bindgen_test]
async fn sleep_wins_over_pending() {
  tokio::select! {
    _ = pending::<()>() => unreachable!(),
    _ = tokio::time::sleep(Duration::from_millis(1)) => {}
  }
}
