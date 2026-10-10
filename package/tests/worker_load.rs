#![cfg(all(
  target_arch = "wasm32",
  target_vendor = "unknown",
  target_os = "unknown"
))]

use tokio_with_wasm::alias as tokio;
use tokio_with_wasm::only_web::{get_script_path, set_path_provider};
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

// Own binary, as an idle worker from another test would skip the load.
#[wasm_bindgen_test]
async fn failed_script_load_fails_the_task() {
  set_path_provider(|| Ok("/nonexistent.js".into()));
  assert!(tokio::task::spawn_blocking(|| 5).await.is_err());
  set_path_provider(get_script_path);
  assert_eq!(tokio::task::spawn_blocking(|| 5).await.unwrap(), 5);
}
