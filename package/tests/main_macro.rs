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
}

// The macro turns this into a plain function
// that spawns the body onto the JavaScript event loop.
#[tokio::main]
async fn entry() {
  RAN.with(|ran| ran.set(true));
}

#[wasm_bindgen_test]
async fn the_main_macro_spawns_the_future() {
  entry();
  // The body runs on the event loop, so yield to let it proceed.
  yield_now().await;
  assert!(RAN.with(|ran| ran.get()));
}
