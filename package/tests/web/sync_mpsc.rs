use tokio::sync::mpsc;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

// Bounded channels lock a `Mutex` on `recv`, which traps on the main thread
// when a worker holds it.
#[wasm_bindgen_test]
async fn send_from_a_web_worker() {
  let (tx, mut rx) = mpsc::unbounded_channel();
  tokio::task::spawn_blocking(move || {
    for i in 0..3 {
      tx.send(i).unwrap();
    }
  });
  for i in 0..3 {
    assert_eq!(rx.recv().await, Some(i));
  }
  assert_eq!(rx.recv().await, None);
}
