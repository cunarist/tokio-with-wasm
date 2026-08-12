//! Browser tests for task IDs.
//! Run with `wasm-pack test --headless --chrome package`.

// The glue code only exists on the web target,
// so this file is empty everywhere else.
#![cfg(all(
  target_family = "wasm",
  target_vendor = "unknown",
  target_os = "unknown"
))]

use std::time::Duration;
use tokio_with_wasm::alias as tokio;
use tokio_with_wasm::task::{Id, JoinError, spawn, spawn_blocking};
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn every_task_gets_its_own_id() {
  let first = spawn(async {});
  let second = spawn(async {});
  assert_ne!(first.id(), second.id());
}

#[wasm_bindgen_test]
async fn the_abort_handle_shares_the_task_id() {
  let handle = spawn(async {});
  assert_eq!(handle.id(), handle.abort_handle().id());
}

#[wasm_bindgen_test]
async fn a_join_error_carries_the_task_id() {
  let handle = spawn(async {
    tokio::time::sleep(Duration::from_secs(10)).await;
  });
  let task_id = handle.id();
  handle.abort();
  let Err(error) = handle.await else {
    panic!("the aborted task returned an output");
  };
  assert_eq!(error.id(), task_id);
}

#[wasm_bindgen_test]
async fn a_task_observes_its_own_id() -> Result<(), JoinError> {
  let handle = spawn(async { tokio::task::id() });
  let task_id = handle.id();
  assert_eq!(handle.await?, task_id);
  Ok(())
}

#[wasm_bindgen_test]
async fn a_blocking_task_observes_its_own_id() -> Result<(), JoinError> {
  let handle = spawn_blocking(tokio::task::try_id);
  let task_id = handle.id();
  assert_eq!(handle.await?, Some(task_id));
  Ok(())
}

#[wasm_bindgen_test]
fn try_id_is_none_outside_of_tasks() {
  assert_eq!(tokio::task::try_id(), None);
}

#[wasm_bindgen_test]
async fn ids_survive_display_and_comparison() -> Result<(), JoinError> {
  let handle = spawn(async { 5 });
  let task_id: Id = handle.id();
  let text = task_id.to_string();
  assert!(!text.is_empty());
  assert!(text.chars().all(|ch| ch.is_ascii_digit()));
  handle.await?;
  Ok(())
}

#[wasm_bindgen_test]
async fn abort_handle_reports_completion() {
  let handle = spawn(std::future::ready(()));
  let abort_handle = handle.abort_handle();
  assert!(!abort_handle.is_finished());
  tokio::time::sleep(Duration::from_millis(100)).await;
  assert!(abort_handle.is_finished());
  assert!(handle.is_finished());
}

#[wasm_bindgen_test]
async fn abort_handle_of_a_pending_task_is_unfinished() {
  let handle = spawn(async {
    tokio::time::sleep(Duration::from_secs(10)).await;
  });
  let abort_handle = handle.abort_handle();
  tokio::time::sleep(Duration::from_millis(50)).await;
  assert!(!abort_handle.is_finished());
  abort_handle.abort();
  let Err(error) = handle.await else {
    panic!("the aborted task returned an output");
  };
  assert!(error.is_cancelled());
  // The cancellation has been delivered by now.
  assert!(abort_handle.is_finished());
}
