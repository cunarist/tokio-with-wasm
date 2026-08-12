//! Browser tests for the re-exported `task_local!` machinery.
//! Run with `wasm-pack test --headless --chrome package`.

// The glue code only exists on the web target,
// so this file is empty everywhere else.
#![cfg(all(
  target_family = "wasm",
  target_vendor = "unknown",
  target_os = "unknown"
))]

use tokio_with_wasm::alias as tokio;
use tokio_with_wasm::task::{JoinError, LocalKey, spawn, yield_now};
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

tokio::task_local! {
  static NUMBER: u32;
  static NAME: String;
}

#[wasm_bindgen_test]
async fn scope_provides_the_value() {
  NUMBER
    .scope(7, async {
      assert_eq!(NUMBER.get(), 7);
      // The value stays in place across await points.
      yield_now().await;
      assert_eq!(NUMBER.get(), 7);
    })
    .await;
}

#[wasm_bindgen_test]
async fn scopes_nest_and_restore() {
  NUMBER
    .scope(1, async {
      assert_eq!(NUMBER.get(), 1);
      NUMBER
        .scope(2, async {
          assert_eq!(NUMBER.get(), 2);
        })
        .await;
      assert_eq!(NUMBER.get(), 1);
    })
    .await;
}

#[wasm_bindgen_test]
fn sync_scope_works_without_awaiting() {
  NAME.sync_scope("hello".to_string(), || {
    NAME.with(|value| assert_eq!(value, "hello"));
  });
}

#[wasm_bindgen_test]
async fn a_spawned_task_gets_its_own_scope() -> Result<(), JoinError> {
  let handle = spawn(NUMBER.scope(5, async { NUMBER.get() }));
  assert_eq!(handle.await?, 5);
  Ok(())
}

#[wasm_bindgen_test]
async fn try_with_reports_a_missing_value() {
  assert!(NUMBER.try_with(|_| ()).is_err());
  NUMBER
    .scope(3, async {
      assert_eq!(NUMBER.try_with(|value| *value), Ok(3));
    })
    .await;
}

#[wasm_bindgen_test]
fn the_key_type_is_reachable_like_in_tokio() {
  // `task_local!` statics must have the re-exported `LocalKey` type.
  let _key: &LocalKey<u32> = &NUMBER;
  let _also: &tokio::task::LocalKey<u32> = &NUMBER;
}
