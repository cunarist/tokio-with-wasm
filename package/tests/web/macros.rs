use std::cell::Cell;
use tokio_with_wasm::alias as tokio;
use tokio_with_wasm::task::{JoinError, yield_now};
use wasm_bindgen_test::wasm_bindgen_test;

thread_local! {
  static RAN: Cell<bool> = const { Cell::new(false) };
  static RAN_RENAMED: Cell<bool> = const { Cell::new(false) };
}

// The macro turns this into a plain function that spawns the body onto the
// JavaScript event loop. Runtime arguments configure a native runtime that
// doesn't exist on the web, so they are accepted and ignored.
#[tokio::main(flavor = "current_thread", worker_threads = 4, name = "entry")]
async fn entry() -> Result<(), std::io::Error> {
  RAN.set(true);
  Ok(())
}

// The expansion references the crate by the given path
// when the dependency is renamed in `Cargo.toml`.
mod renamed {
  use renamed_wasm::alias as tokio;
  pub use tokio_with_wasm as renamed_wasm;

  #[tokio::main(crate = "renamed_wasm")]
  pub async fn entry() {
    super::RAN_RENAMED.set(true);
  }
}

#[wasm_bindgen_test]
async fn the_main_macro_spawns_the_future() {
  entry();
  // The body runs on the event loop, so yield to let it proceed.
  yield_now().await;
  assert!(RAN.get());
}

#[wasm_bindgen_test]
async fn the_main_macro_honors_a_renamed_crate() {
  renamed::entry();
  yield_now().await;
  assert!(RAN_RENAMED.get());
}

// On the web, this expands to an async `wasm-bindgen-test` test.
#[tokio::test(flavor = "multi_thread", worker_threads = 2, crate = "tokio")]
async fn the_test_macro_runs_async_tests() -> Result<(), JoinError> {
  assert_eq!(tokio::spawn(async { 6 * 7 }).await?, 42);
  Ok(())
}

#[tokio::test]
#[should_panic]
async fn the_test_macro_fails_err_results() -> Result<(), std::io::Error> {
  Err(std::io::Error::other("failed"))
}
