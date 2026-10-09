//! Browser tests for `task`.
//! Run with `wasm-pack test --headless --chrome package`.

// The glue code only exists on the web target,
// so this file is empty everywhere else.
#![cfg(all(
  target_family = "wasm",
  target_vendor = "unknown",
  target_os = "unknown"
))]

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;
use tokio_with_wasm::alias as tokio;
use tokio_with_wasm::task::{JoinError, JoinHandle, spawn, yield_now};
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn spawn_returns_the_output() -> Result<(), JoinError> {
  let handle = spawn(async { 6 * 7 });
  assert_eq!(handle.await?, 42);
  Ok(())
}

#[wasm_bindgen_test]
async fn spawn_runs_without_being_awaited() {
  let flag = Rc::new(Cell::new(false));
  let cloned = flag.clone();
  let _detached: JoinHandle<()> = spawn(async move {
    cloned.set(true);
  });
  // The task runs on the same event loop, so yielding lets it proceed.
  yield_now().await;
  assert!(flag.get());
}

#[wasm_bindgen_test]
async fn spawn_accepts_non_send_futures() -> Result<(), JoinError> {
  let handle = spawn(async {
    let rc = Rc::new(5);
    // The `Rc` lives across an await point.
    yield_now().await;
    *rc
  });
  assert_eq!(handle.await?, 5);
  Ok(())
}

#[wasm_bindgen_test]
async fn abort_cancels_a_pending_task() {
  let handle = spawn(std::future::pending::<()>());
  // The task is waiting by now, with nothing left to wake it but the abort.
  yield_now().await;
  handle.abort();
  let joined = tokio::time::timeout(Duration::from_secs(1), handle).await;
  let Ok(Err(error)) = joined else {
    panic!("the aborted task did not finish as cancelled");
  };
  assert!(error.is_cancelled());
  assert!(!error.is_panic());
}

#[wasm_bindgen_test]
async fn abort_after_completion_keeps_the_output() -> Result<(), JoinError> {
  let handle = spawn(async { 7 });
  // Give the task time to finish before aborting.
  tokio::time::sleep(Duration::from_millis(50)).await;
  handle.abort();
  handle.abort(); // A second abort must be harmless too.
  assert_eq!(handle.await?, 7);
  Ok(())
}

#[wasm_bindgen_test]
async fn tasks_interleave_on_the_event_loop() -> Result<(), JoinError> {
  let log = Rc::new(std::cell::RefCell::new(Vec::new()));
  let mut handles = Vec::new();
  for id in 0..3 {
    let log = log.clone();
    handles.push(spawn(async move {
      for _ in 0..2 {
        log.borrow_mut().push(id);
        yield_now().await;
      }
    }));
  }
  for handle in handles {
    handle.await?;
  }
  // Every task yielded, so no task finished both
  // rounds before the others started.
  let log = log.borrow();
  assert_eq!(log.len(), 6);
  assert_eq!(&log[..3], &[0, 1, 2]);
  Ok(())
}
