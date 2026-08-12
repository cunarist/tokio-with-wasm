//! Browser tests for the worker script provider.
//! Run with `wasm-pack test --headless --chrome package`.

// The glue code only exists on the web target,
// so this file is empty everywhere else.
#![cfg(all(
  target_arch = "wasm32",
  target_vendor = "unknown",
  target_os = "unknown"
))]

use tokio_with_wasm::only_web::{
  get_worker_script, set_worker_script_provider,
};
use tokio_with_wasm::task::{JoinError, spawn_blocking};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn the_default_provider_builds_a_blob_url() -> Result<(), JsValue> {
  let url = get_worker_script()?;
  assert!(url.starts_with("blob:"), "unexpected url: {url}");
  // The URL is reused, so that workers don't leak one object URL each.
  assert_eq!(url, get_worker_script()?);
  Ok(())
}

/// A content security policy that forbids `blob:` workers is answered by
/// serving the worker script as a file, which is what this provider does.
/// A missing file fails the task instead of hanging it.
#[wasm_bindgen_test]
async fn a_custom_worker_script_is_used() -> Result<(), JoinError> {
  set_worker_script_provider(|| {
    Ok(String::from("/definitely-missing-worker.js"))
  });
  let failed = spawn_blocking(|| 5).await;
  assert!(failed.is_err_and(|error| error.is_panic()));

  // With the default provider restored, the pool must recover:
  // the failed worker's slot was given back.
  set_worker_script_provider(get_worker_script);
  assert_eq!(spawn_blocking(|| 5).await?, 5);
  Ok(())
}
