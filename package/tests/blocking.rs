//! Browser tests for `spawn_blocking`, which runs on web workers.
//! These need the shared memory build that `.cargo/config.toml` sets up.
//! Run with `wasm-pack test --headless --chrome package`.

// The glue code only exists on the web target,
// so this file is empty everywhere else.
#![cfg(all(
  target_family = "wasm",
  target_vendor = "unknown",
  target_os = "unknown"
))]

use tokio_with_wasm::task::{JoinError, JoinSet, spawn_blocking};
use tokio_with_wasm::time::{Duration, sleep};
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn blocking_task_returns_the_output() -> Result<(), JoinError> {
  let handle = spawn_blocking(|| {
    let mut data = "Hello, ".to_string();
    data.push_str("world");
    data
  });
  assert_eq!(handle.await?, "Hello, world");
  Ok(())
}

#[wasm_bindgen_test]
async fn blocking_tasks_run_in_parallel() -> Result<(), JoinError> {
  let start = js_sys::Date::now();
  let first = spawn_blocking(|| {
    std::thread::sleep(std::time::Duration::from_millis(400));
  });
  let second = spawn_blocking(|| {
    std::thread::sleep(std::time::Duration::from_millis(400));
  });
  first.await?;
  second.await?;
  let elapsed = js_sys::Date::now() - start;
  // Sequential runs would take 800ms or more.
  assert!(elapsed < 750.0, "the tasks did not overlap: {elapsed}ms");
  Ok(())
}

#[wasm_bindgen_test]
async fn panicking_blocking_task_reports_a_panic() {
  let handle = spawn_blocking(|| panic!("boom"));
  let Err(error) = handle.await else {
    panic!("the panicking task reported success");
  };
  assert!(error.is_panic());
  assert!(!error.is_cancelled());
}

/// The worker that hosted a panic must not poison the pool:
/// tasks spawned afterwards still have to run.
#[wasm_bindgen_test]
async fn the_pool_survives_a_panic() -> Result<(), JoinError> {
  let poisoned = spawn_blocking(|| panic!("boom"));
  assert!(poisoned.await.is_err_and(|error| error.is_panic()));
  let healthy = spawn_blocking(|| 21 * 2);
  assert_eq!(healthy.await?, 42);
  Ok(())
}

#[wasm_bindgen_test]
async fn abort_before_start_cancels_a_blocking_task() {
  let handle = spawn_blocking(|| 5);
  // The task has not been sent to a worker yet in this microtask,
  // so aborting now prevents it from running.
  handle.abort();
  assert!(handle.await.is_err_and(|error| error.is_cancelled()));
}

#[wasm_bindgen_test]
async fn a_worker_is_reused_between_tasks() -> Result<(), JoinError> {
  // The first task creates a worker; after it finishes, the pool
  // should hand the same worker to the next task instead of hanging.
  for round in 0..3 {
    let output = spawn_blocking(move || round).await?;
    assert_eq!(output, round);
    // Let the pool reclaim the worker.
    sleep(Duration::from_millis(50)).await;
  }
  Ok(())
}

/// `spawn` is forbidden inside a blocking thread and panics there.
/// The panic must come back to the caller as a `JoinError`.
#[wasm_bindgen_test]
async fn spawning_inside_a_worker_is_reported_as_a_panic() {
  let handle = spawn_blocking(|| {
    drop(tokio_with_wasm::task::spawn(async {}));
  });
  assert!(handle.await.is_err_and(|error| error.is_panic()));
}

/// A worker is culled after ten idle seconds. The pool's management
/// timer stops with it, so this also proves that the timer starts
/// again for the task spawned afterwards.
#[wasm_bindgen_test]
async fn an_idle_worker_is_culled_and_the_pool_recovers()
-> Result<(), JoinError> {
  spawn_blocking(|| 1).await?;
  sleep(Duration::from_millis(10_500)).await;
  assert_eq!(spawn_blocking(|| 2).await?, 2);
  Ok(())
}

#[wasm_bindgen_test]
async fn join_set_collects_blocking_tasks() {
  let mut set = JoinSet::new();
  for i in 0..5 {
    set.spawn_blocking(move || i);
  }
  let mut output = set.join_all().await;
  output.sort();
  assert_eq!(output, vec![0, 1, 2, 3, 4]);
}
