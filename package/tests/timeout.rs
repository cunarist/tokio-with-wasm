//! Browser tests for `Timeout` and `timeout_at`.
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

use std::future::ready;
use tokio_with_wasm::time::{
  Duration, Instant, error::Elapsed, sleep, timeout, timeout_at,
};
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn timeout_at_returns_the_output_in_time() -> Result<(), Elapsed> {
  let deadline = Instant::now() + Duration::from_secs(5);
  let output = timeout_at(deadline, async { 42 }).await;
  assert_eq!(output?, 42);
  Ok(())
}

#[wasm_bindgen_test]
async fn timeout_at_elapses_on_a_slow_future() {
  let deadline = Instant::now() + Duration::from_millis(50);
  let output = timeout_at(deadline, sleep(Duration::from_secs(10))).await;
  assert!(output.is_err(), "the slow future was not cut off");
}

#[wasm_bindgen_test]
async fn timeout_at_a_past_deadline_still_delivers_a_ready_output()
-> Result<(), Elapsed> {
  // The future is polled before the clock, like in `tokio`.
  let past = Instant::now() - Duration::from_secs(1);
  let output = timeout_at(past, ready(42)).await;
  assert_eq!(output?, 42);
  Ok(())
}

#[wasm_bindgen_test]
async fn timeout_at_a_past_deadline_elapses_on_a_pending_future() {
  let past = Instant::now() - Duration::from_secs(1);
  let output = timeout_at(past, std::future::pending::<()>()).await;
  assert!(output.is_err(), "a pending future beat a past deadline");
}

#[wasm_bindgen_test]
async fn the_inner_future_can_be_taken_back_out() {
  let wrapped = timeout(Duration::from_secs(5), ready(7));
  let _borrowed: &std::future::Ready<i32> = wrapped.get_ref();
  let mut wrapped = wrapped;
  let _mutable: &mut std::future::Ready<i32> = wrapped.get_mut();
  let inner = wrapped.into_inner();
  assert_eq!(inner.await, 7);
}

/// The error type lives at `time::error::Elapsed`, like in `tokio`.
/// The old `time::Elapsed` re-export stays for compatibility.
#[wasm_bindgen_test]
async fn the_error_paths_point_at_the_same_type() {
  let output =
    timeout(Duration::from_millis(50), sleep(Duration::from_secs(10))).await;
  let Err(error) = output else {
    panic!("the slow future was not cut off");
  };
  let at_error_path: tokio_with_wasm::time::error::Elapsed = error;
  let at_old_path: tokio_with_wasm::time::Elapsed = at_error_path;
  // The error converts into a timed-out IO error.
  let io_error: std::io::Error = at_old_path.into();
  assert_eq!(io_error.kind(), std::io::ErrorKind::TimedOut);
}
