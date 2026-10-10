#![cfg(all(
  target_arch = "wasm32",
  target_vendor = "unknown",
  target_os = "unknown"
))]

use std::sync::atomic::{AtomicUsize, Ordering};
use tokio_with_wasm::alias as tokio;
use tokio_with_wasm::only_web::{get_script_path, set_path_provider};
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

static CALLS: AtomicUsize = AtomicUsize::new(0);

#[wasm_bindgen_test]
async fn custom_provider_is_used() {
  set_path_provider(|| {
    CALLS.fetch_add(1, Ordering::SeqCst);
    get_script_path()
  });
  assert_eq!(tokio::task::spawn_blocking(|| 5).await.unwrap(), 5);
  assert_eq!(CALLS.load(Ordering::SeqCst), 1);
}
