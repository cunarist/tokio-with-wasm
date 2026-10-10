use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

async fn one() {}
async fn two() {}

#[wasm_bindgen_test]
async fn multi_pin() {
  tokio::pin! {
    let f1 = one();
    let f2 = two();
  }
  (&mut f1).await;
  (&mut f2).await;
}
