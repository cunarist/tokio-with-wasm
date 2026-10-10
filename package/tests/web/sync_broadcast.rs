use tokio::sync::broadcast;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

// The channel sits behind a lock, so the worker finishes before the main
// thread receives.
#[wasm_bindgen_test]
async fn send_from_a_web_worker() {
  let (tx, mut rx1) = broadcast::channel(4);
  let mut rx2 = tx.subscribe();
  tokio::task::spawn_blocking(move || {
    tx.send(1).unwrap();
    tx.send(2).unwrap();
  })
  .await
  .unwrap();
  for rx in [&mut rx1, &mut rx2] {
    assert_eq!(rx.recv().await, Ok(1));
    assert_eq!(rx.recv().await, Ok(2));
    assert!(rx.recv().await.is_err());
  }
}
