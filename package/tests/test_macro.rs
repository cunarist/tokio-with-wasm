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
#[tokio::test(flavor = "multi_thread", worker_threads = 2, crate = "tokio")]
async fn the_test_macro_accepts_runtime_arguments() {
  sleep(Duration::from_millis(10)).await;
}

#[tokio::test]
async fn the_test_macro_passes_ok_results() -> Result<(), std::io::Error> {
  sleep(Duration::from_millis(10)).await;
  Ok(())
}

#[tokio::test]
#[should_panic]
async fn the_test_macro_fails_err_results() -> Result<(), std::io::Error> {
  Err(std::io::Error::other("failed"))
}
