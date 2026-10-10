use tokio::sync::oneshot;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn err_abort_early() {
  let (tx1, rx1) = oneshot::channel::<&str>();
  let (tx2, rx2) = oneshot::channel::<u32>();
  let (_tx3, rx3) = oneshot::channel::<u32>();
  tokio::spawn(async move {
    tx2.send(123).unwrap();
    drop(tx1);
  });
  assert!(tokio::try_join!(rx1, rx2, rx3).is_err());
}
