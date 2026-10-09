use js_sys::SharedArrayBuffer;
use js_sys::WebAssembly::Memory;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio_with_wasm::task::{JoinError, spawn_blocking};
use tokio_with_wasm::time::{Duration, Instant, sleep};
use wasm_bindgen::{JsCast, memory};
use wasm_bindgen_test::wasm_bindgen_test;

thread_local! {
  static RUNS: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Returns how many tasks have run on this worker, this one included.
fn count_run() -> u32 {
  RUNS.with(|runs| runs.replace(runs.get() + 1) + 1)
}

#[wasm_bindgen_test]
async fn blocking_tasks_run_in_parallel() -> Result<(), JoinError> {
  static ARRIVED: AtomicUsize = AtomicUsize::new(0);
  // Each task waits up to five seconds for the other one to arrive.
  let meet = || {
    ARRIVED.fetch_add(1, Ordering::SeqCst);
    let start = Instant::now();
    while ARRIVED.load(Ordering::SeqCst) < 2 {
      if start.elapsed() > Duration::from_secs(5) {
        return false;
      }
    }
    true
  };
  let (first, second) = (spawn_blocking(meet), spawn_blocking(meet));
  assert!(first.await? && second.await?, "the tasks did not overlap");
  Ok(())
}

/// The worker that hosted a panic must not poison the pool:
/// tasks spawned afterwards still have to run.
#[wasm_bindgen_test]
async fn a_panic_is_reported_and_the_pool_survives() -> Result<(), JoinError> {
  let error = spawn_blocking(|| panic!("boom")).await.unwrap_err();
  assert!(error.is_panic() && !error.is_cancelled());
  assert_eq!(error.to_string(), "task panicked");
  assert_eq!(spawn_blocking(|| 21 * 2).await?, 42);
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
