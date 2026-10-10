use std::time::Duration;
use tokio::sync::oneshot;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn two_await() {
  let (tx1, rx1) = oneshot::channel::<&str>();
  let (tx2, rx2) = oneshot::channel::<u32>();
  tokio::spawn(async move {
    tokio::time::sleep(Duration::from_millis(1)).await;
    tx2.send(123).unwrap();
    tx1.send("hello").unwrap();
  });
  let res =
    tokio::join!(async { rx1.await.unwrap() }, async { rx2.await.unwrap() });
  assert_eq!(res, ("hello", 123));
}
