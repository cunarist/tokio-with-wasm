//! Browser tests for the `#[tokio::test]` attribute macro,
//! which expands to an async `wasm-bindgen-test` test on the web.
//! Run with `wasm-pack test --headless --chrome package`.

// The glue code only exists on the web target,
// so this file is empty everywhere else.
#![cfg(all(
  target_family = "wasm",
  target_vendor = "unknown",
  target_os = "unknown"
))]

use tokio_with_wasm::alias as tokio;
use tokio_with_wasm::time::{Duration, sleep};

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[tokio::test]
async fn the_test_macro_runs_async_tests() {
  let handle = tokio::spawn(async { 6 * 7 });
  let Ok(output) = handle.await else {
    panic!("the spawned task failed");
  };
  assert_eq!(output, 42);
}

// Runtime arguments configure a native runtime that doesn't exist on
// the web, so they are accepted and ignored.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_test_macro_accepts_runtime_arguments() {
  sleep(Duration::from_millis(10)).await;
}

// An `Ok` result passes; an `Err` would panic and fail the test.
#[tokio::test]
async fn the_test_macro_unwraps_ok_results() -> Result<(), std::io::Error> {
  sleep(Duration::from_millis(10)).await;
  Ok(())
}

// The expansion references the crate by the given path
// when the dependency is renamed in `Cargo.toml`.
mod renamed {
  pub use tokio_with_wasm as renamed_wasm;
  use renamed_wasm::alias as tokio;

  #[tokio::test(crate = "renamed_wasm")]
  async fn the_test_macro_honors_a_renamed_crate() {
    let handle = tokio::spawn(async { 6 * 7 });
    let Ok(output) = handle.await else {
      panic!("the spawned task failed");
    };
    assert_eq!(output, 42);
  }
}
