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

use js_sys::SharedArrayBuffer;
use js_sys::WebAssembly::Memory;
use tokio_with_wasm::task::{JoinError, JoinSet, spawn_blocking};
use tokio_with_wasm::time::{Duration, sleep};
use wasm_bindgen::{JsCast, memory};
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

thread_local! {
  static RUNS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Returns how many tasks have run on this worker, this one included.
fn count_run() -> u32 {
  RUNS.with(|runs| runs.replace(runs.get() + 1) + 1)
}

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
  assert!(
    handle
      .await
      .is_err_and(|error| error.is_panic() && !error.is_cancelled())
  );
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
async fn a_worker_is_reused_between_tasks() -> Result<(), JoinError> {
  // A worker that comes back to the pool is the next one handed out,
  // so each task runs on the thread the previous one ran on.
  let mut last = 0;
  for _ in 0..3 {
    let runs = spawn_blocking(count_run).await?;
    assert!(runs > last, "a fresh worker took the task");
    last = runs;
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

/// Workers are culled after ten idle seconds. The pool's management
/// timer stops with them, so this also proves that the timer starts
/// again for the tasks spawned afterwards, and that the culled workers
/// gave their stacks back for the new ones to reuse.
#[wasm_bindgen_test]
async fn idle_workers_are_culled_and_the_pool_recovers() -> Result<(), JoinError>
{
  async fn run_four() -> Result<(Vec<u32>, f64), JoinError> {
    let handles: Vec<_> = (0..4)
      .map(|_| {
        spawn_blocking(|| {
          std::thread::sleep(Duration::from_millis(100));
          count_run()
        })
      })
      .collect();
    let mut runs = Vec::new();
    for handle in handles {
      runs.push(handle.await?);
    }
    let memory = memory().unchecked_into::<Memory>();
    let bytes = memory
      .buffer()
      .unchecked_into::<SharedArrayBuffer>()
      .byte_length();
    Ok((runs, bytes as f64))
  }
  let (_, before) = run_four().await?;
  sleep(Duration::from_millis(10_500)).await;
  let (runs, after) = run_four().await?;
  // Fresh workers have run nothing before.
  assert_eq!(runs, [1; 4], "the idle workers were not culled");
  let grown = after - before;
  // Four leaked stacks would take eight megabytes. The allocator may still
  // grow by one stack while it settles, depending on what ran before.
  assert!(
    grown < 4_194_304.0,
    "the culled workers leaked {grown} bytes"
  );
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
