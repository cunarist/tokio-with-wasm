use tokio::sync::watch;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

// The value sits behind a lock, so the worker finishes before the main
// thread reads it.
#[wasm_bindgen_test]
async fn send_from_a_web_worker() {
  let (tx, mut rx) = watch::channel(0);
  tokio::task::spawn_blocking(move || tx.send(1).unwrap())
    .await
    .unwrap();
  rx.changed().await.unwrap();
  assert_eq!(*rx.borrow_and_update(), 1);
}
