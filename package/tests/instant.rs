//! Browser tests for `time::Instant`.
//! Run with `wasm-pack test --headless --chrome package`.
//!
//! Timing assertions use generous bounds, because headless browsers on
//! loaded CI machines fire timers late.

// The glue code only exists on the web target,
// so this file is empty everywhere else.
#![cfg(all(
  target_family = "wasm",
  target_vendor = "unknown",
  target_os = "unknown"
))]

use tokio_with_wasm::task::{JoinError, spawn_blocking};
use tokio_with_wasm::time::{Duration, Instant, sleep};
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn instants_never_run_backwards() {
  let mut previous = Instant::now();
  for _ in 0..1000 {
    let current = Instant::now();
    assert!(current >= previous);
    previous = current;
  }
}

#[wasm_bindgen_test]
async fn elapsed_grows_while_sleeping() {
  let start = Instant::now();
  sleep(Duration::from_millis(200)).await;
  let elapsed = start.elapsed();
  assert!(
    elapsed >= Duration::from_millis(150),
    "woke up too early: {elapsed:?}"
  );
  assert!(
    elapsed < Duration::from_secs(5),
    "woke up too late: {elapsed:?}"
  );
}

#[wasm_bindgen_test]
fn duration_arithmetic_is_exact() {
  let base = Instant::now();
  let step = Duration::from_millis(1500);
  let later = base + step;
  assert_eq!(later - base, step);
  assert_eq!(later.duration_since(base), step);
  assert_eq!(later.checked_duration_since(base), Some(step));
  assert_eq!(later - step, base);

  let mut cursor = base;
  cursor += step;
  assert_eq!(cursor, later);
  cursor -= step;
  assert_eq!(cursor, base);
}

#[wasm_bindgen_test]
fn earlier_instants_saturate_to_zero() {
  let base = Instant::now();
  let later = base + Duration::from_secs(9);
  assert_eq!(base.duration_since(later), Duration::ZERO);
  assert_eq!(base.saturating_duration_since(later), Duration::ZERO);
  assert_eq!(base.checked_duration_since(later), None);
  assert_eq!(base - later, Duration::ZERO);
}

#[wasm_bindgen_test]
fn checked_arithmetic_reports_out_of_range() {
  let base = Instant::now();
  assert_eq!(base.checked_add(Duration::MAX), None);
  // The JavaScript epoch is decades in the past, not centuries.
  assert_eq!(
    base.checked_sub(Duration::from_secs(86400 * 365 * 200)),
    None
  );
  assert!(base.checked_add(Duration::from_secs(60)).is_some());
  assert!(base.checked_sub(Duration::from_secs(60)).is_some());
}

#[wasm_bindgen_test]
fn instants_are_ordered_and_hashable() {
  use std::collections::HashSet;
  let base = Instant::now();
  let later = base + Duration::from_secs(1);
  assert!(later > base);
  assert_eq!(base.max(later), later);
  let mut set = HashSet::new();
  set.insert(base);
  set.insert(later);
  set.insert(base);
  assert_eq!(set.len(), 2);
}

/// A web worker has its own `performance.timeOrigin`, so a raw
/// `performance.now()` there would sit near zero, decades before any
/// instant from the main thread. Adding the origin back in must keep
/// instants from both threads on one clock.
#[wasm_bindgen_test]
async fn worker_instants_share_the_main_thread_clock() -> Result<(), JoinError>
{
  let slack = Duration::from_millis(500);
  let before = Instant::now();
  let worker_instant = spawn_blocking(Instant::now).await?;
  let after = Instant::now();
  assert!(
    worker_instant >= before - slack,
    "the worker clock is behind: {worker_instant:?} < {before:?}"
  );
  assert!(
    worker_instant <= after + slack,
    "the worker clock is ahead: {worker_instant:?} > {after:?}"
  );
  Ok(())
}
