#![cfg(all(
  target_arch = "wasm32",
  target_vendor = "unknown",
  target_os = "unknown"
))]

use std::time::Duration;
use tokio_with_wasm::alias as tokio;
use tokio_with_wasm::only_web::{get_script_path, set_path_provider};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn culled_worker_closes_without_thread_destroy() {
  set_path_provider(|| {
    // Wraps the real script, so that `init` resolves to no exports
    // and the worker holds a lock for as long as it lives.
    let wrap = js_sys::Function::new_with_args(
      "path",
      "
      const source = `
        import init from '${path}';
        export * from '${path}';
        export default async module => {
          await init(module);
          navigator.locks.request('worker', () => new Promise(() => {}));
          return {};
        };
      `;
      return URL.createObjectURL(
        new Blob([source], { type: 'text/javascript' }),
      );
      ",
    );
    let path = JsValue::from_str(&get_script_path()?);
    Ok(wrap.call1(&JsValue::NULL, &path)?.as_string().unwrap())
  });
  tokio::task::spawn_blocking(|| {}).await.unwrap();
  // The lock is granted once the worker is gone, after the 10 second cull.
  let acquire = js_sys::Function::new_no_args(
    "return navigator.locks.request('worker', () => true);",
  );
  let acquired = acquire.call0(&JsValue::NULL).unwrap();
  let acquired = JsFuture::from(acquired.unchecked_into::<js_sys::Promise>());
  let result = tokio::time::timeout(Duration::from_secs(20), acquired).await;
  assert!(result.is_ok(), "the culled worker is still alive");
}
