use std::cell::Cell;
use std::time::Duration;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

thread_local! {
  static RAN: Cell<bool> = const { Cell::new(false) };
  static AWAITED: Cell<bool> = const { Cell::new(false) };
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
  RAN.set(true);
  Ok(())
}

#[tokio::main(flavor = "current_thread")]
pub async fn main_with_awaits() {
  tokio::task::yield_now().await;
  AWAITED.set(true);
}

#[wasm_bindgen_test]
async fn main_runs_its_body() {
  main();
  tokio::task::yield_now().await;
  assert!(RAN.get());
}

#[wasm_bindgen_test]
async fn main_runs_until_its_body_finishes() {
  main_with_awaits();
  assert!(!AWAITED.get());
  tokio::time::sleep(Duration::from_millis(50)).await;
  assert!(AWAITED.get());
}

#[rustfmt::skip]
#[tokio::main]
pub async fn unused_braces_main() { println!("hello") }

#[tokio::main]
pub async fn never_returns() -> ! {
  panic!();
}
