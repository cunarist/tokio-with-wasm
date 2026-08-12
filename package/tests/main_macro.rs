//! Browser test for the `#[tokio::main]` attribute macro.
//! Run with `wasm-pack test --headless --chrome package`.

// The glue code only exists on the web target,
// so this file is empty everywhere else.
#![cfg(all(
  target_family = "wasm",
  target_vendor = "unknown",
  target_os = "unknown"
))]

use std::cell::Cell;
use tokio_with_wasm::alias as tokio;
use tokio_with_wasm::task::yield_now;
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

thread_local! {
  static RAN: Cell<bool> = const { Cell::new(false) };
  static RAN_WITH_ARGS: Cell<bool> = const { Cell::new(false) };
  static RAN_WITH_RESULT: Cell<bool> = const { Cell::new(false) };
  static RAN_RENAMED: Cell<bool> = const { Cell::new(false) };
}

// The macro turns this into a plain function
// that spawns the body onto the JavaScript event loop.
#[tokio::main]
async fn entry() {
  RAN.with(|ran| ran.set(true));
}

// Runtime arguments configure a native runtime that doesn't exist on
// the web, so they are accepted and ignored.
#[tokio::main(flavor = "current_thread", worker_threads = 4)]
async fn entry_with_args() {
  RAN_WITH_ARGS.with(|ran| ran.set(true));
}

// A `Result` return is unwrapped once the future completes.
#[tokio::main]
async fn entry_with_result() -> Result<(), std::io::Error> {
  RAN_WITH_RESULT.with(|ran| ran.set(true));
  Ok(())
}

// The expansion references the crate by the given path
// when the dependency is renamed in `Cargo.toml`.
mod renamed {
  use renamed_wasm::alias as tokio;
  pub use tokio_with_wasm as renamed_wasm;

  #[tokio::main(crate = "renamed_wasm")]
  pub async fn entry_renamed() {
    super::RAN_RENAMED.with(|ran| ran.set(true));
  }
}

#[wasm_bindgen_test]
async fn the_main_macro_spawns_the_future() {
  entry();
  // The body runs on the event loop, so yield to let it proceed.
  yield_now().await;
  assert!(RAN.with(|ran| ran.get()));
}

#[wasm_bindgen_test]
async fn the_main_macro_accepts_runtime_arguments() {
  entry_with_args();
  yield_now().await;
  assert!(RAN_WITH_ARGS.with(|ran| ran.get()));
}

#[wasm_bindgen_test]
async fn the_main_macro_unwraps_ok_results() {
  entry_with_result();
  yield_now().await;
  assert!(RAN_WITH_RESULT.with(|ran| ran.get()));
}

#[wasm_bindgen_test]
async fn the_main_macro_honors_a_renamed_crate() {
  renamed::entry_renamed();
  yield_now().await;
  assert!(RAN_RENAMED.with(|ran| ran.get()));
}
