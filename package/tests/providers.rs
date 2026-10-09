//! Browser tests for the worker providers, in a page of their own:
//! they replace the providers, and need a pool with no idle workers.

#![cfg(all(
  target_family = "wasm",
  target_vendor = "unknown",
  target_os = "unknown"
))]
#![allow(clippy::unwrap_used)]

use tokio_with_wasm::only_web::{
  get_script_path, get_worker_script, set_path_provider,
  set_worker_script_provider,
};
use tokio_with_wasm::task::{JoinError, spawn_blocking};
use tokio_with_wasm::time::{Duration, timeout};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn the_default_providers_find_the_scripts() -> Result<(), JsValue> {
  let path = get_script_path()?;
  assert!(path.starts_with("http"), "unexpected path: {path}");
  let url = get_worker_script()?;
  assert!(url.starts_with("blob:"), "unexpected url: {url}");
  // The URL is reused, so that workers don't leak one object URL each.
  assert_eq!(url, get_worker_script()?);
  Ok(())
}

/// A misconfigured path, or a content security policy that blocks the
/// script, turns into a failed task, not a silent hang.
#[wasm_bindgen_test]
async fn bad_scripts_fail_the_task_and_the_pool_recovers()
-> Result<(), JoinError> {
  let run = || timeout(Duration::from_secs(5), spawn_blocking(|| 5));
  set_path_provider(|| Ok("/definitely-missing-glue.js".into()));
  assert!(run().await.unwrap().is_err_and(|error| error.is_panic()));
  set_path_provider(get_script_path);

  // Serving the worker script as a file answers a policy against `blob:`.
  set_worker_script_provider(|| Ok("/definitely-missing-worker.js".into()));
  assert!(run().await.unwrap().is_err_and(|error| error.is_panic()));
  // A worker that cannot even be created never ran the task.
  set_worker_script_provider(|| Err("no script".into()));
  assert!(
    run()
      .await
      .unwrap()
      .is_err_and(|error| error.is_cancelled())
  );

  // With the default restored, the failed workers' slots are free again.
  set_worker_script_provider(get_worker_script);
  assert_eq!(run().await.unwrap()?, 5);
  Ok(())
}
