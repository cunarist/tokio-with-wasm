use tokio::sync::oneshot;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn send_from_a_web_worker() {
  let (tx, rx) = oneshot::channel();
  tokio::task::spawn_blocking(move || tx.send("hello").unwrap());
  assert_eq!(rx.await, Ok("hello"));
}

#[wasm_bindgen_test]
async fn sender_dropped_in_a_web_worker() {
  let (tx, rx) = oneshot::channel::<()>();
  tokio::task::spawn_blocking(move || drop(tx));
  assert!(rx.await.is_err());
}
