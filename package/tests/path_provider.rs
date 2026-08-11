//! Browser tests for the worker script path provider.
//! Run with `wasm-pack test --headless --chrome package`.

// The glue code only exists on the web target,
// so this file is empty everywhere else.
#![cfg(all(
  target_arch = "wasm32",
  target_vendor = "unknown",
  target_os = "unknown"
))]

use tokio_with_wasm::only_web::{get_script_path, set_path_provider};
use tokio_with_wasm::task::{JoinError, spawn_blocking};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn the_default_provider_finds_the_running_script() -> Result<(), JsValue> {
  let path = get_script_path()?;
  assert!(path.starts_with("http"), "unexpected path: {path}");
  Ok(())
}

/// This is what a misconfigured path (or a content security policy that
/// blocks the script) turns into: a failed task, not a silent hang.
#[wasm_bindgen_test]
async fn a_bad_script_path_fails_the_task_instead_of_hanging()
-> Result<(), JoinError> {
  set_path_provider(|| Ok(String::from("/definitely-missing-glue.js")));
  let failed = spawn_blocking(|| 5).await;
  assert!(failed.is_err_and(|error| error.is_panic()));

  // With the default provider restored, the pool must recover:
  // the failed worker's slot was given back.
  set_path_provider(get_script_path);
  assert_eq!(spawn_blocking(|| 5).await?, 5);
  Ok(())
}
