use std::cell::Cell;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

thread_local! {
  static RAN: Cell<bool> = const { Cell::new(false) };
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
  RAN.set(true);
  Ok(())
}

#[wasm_bindgen_test]
async fn main_runs_its_body() {
  main();
  tokio::task::yield_now().await;
  assert!(RAN.get());
}
