//! Browser tests for `time`.
//! Run with `wasm-pack test --headless --chrome package`.
//!
//! Timing assertions use generous bounds, because headless browsers on
//! loaded CI machines fire timers late.

// The glue code only exists on the web target,
// so this file is empty everywhere else.
#![cfg(all(
  target_arch = "wasm32",
  target_vendor = "unknown",
  target_os = "unknown"
))]

use std::io;
// `Duration` must be reachable through `time`, like in `tokio`.
use tokio_with_wasm::alias as tokio;
use tokio_with_wasm::time::{Duration, interval, sleep, timeout};
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

/// Current time in milliseconds.
fn now() -> f64 {
  js_sys::Date::now()
}

#[wasm_bindgen_test]
async fn sleep_waits_roughly_the_duration() {
  let start = now();
  sleep(Duration::from_millis(200)).await;
  let elapsed = now() - start;
  assert!(elapsed >= 150.0, "woke up too early: {elapsed}ms");
  assert!(elapsed < 5000.0, "woke up too late: {elapsed}ms");
}

#[wasm_bindgen_test]
async fn sleep_zero_completes() {
  sleep(Duration::from_millis(0)).await;
}

#[wasm_bindgen_test]
async fn timeout_returns_the_output_in_time() {
  let output = timeout(Duration::from_secs(5), async { 42 }).await;
  assert_eq!(output.unwrap(), 42);
}

#[wasm_bindgen_test]
async fn timeout_elapses_on_a_slow_future() {
  let output =
    timeout(Duration::from_millis(50), sleep(Duration::from_secs(10))).await;
  let elapsed = output.unwrap_err();
  // The error converts into a timed-out IO error.
  let io_error: io::Error = elapsed.into();
  assert_eq!(io_error.kind(), io::ErrorKind::TimedOut);
}

#[wasm_bindgen_test]
async fn interval_ticks_immediately_first() {
  let mut ticker = interval(Duration::from_millis(300));
  let start = now();
  ticker.tick().await;
  let elapsed = now() - start;
  assert!(
    elapsed < 150.0,
    "first tick took a whole period: {elapsed}ms"
  );
}

#[wasm_bindgen_test]
async fn interval_ticks_after_each_period() {
  let mut ticker = interval(Duration::from_millis(200));
  ticker.tick().await; // Immediate.
  let start = now();
  ticker.tick().await;
  ticker.tick().await;
  let elapsed = now() - start;
  assert!(
    elapsed >= 250.0,
    "two periods passed too quickly: {elapsed}ms"
  );
}

#[wasm_bindgen_test]
async fn interval_reset_delays_the_next_tick() {
  let mut ticker = interval(Duration::from_millis(200));
  ticker.tick().await; // Immediate.
  ticker.reset();
  let start = now();
  ticker.tick().await;
  let elapsed = now() - start;
  assert!(elapsed >= 120.0, "tick came before the period: {elapsed}ms");
}

#[wasm_bindgen_test]
async fn timeout_zero_still_delivers_a_ready_output() {
  // The future is polled before the clock, like in `tokio`.
  let output = timeout(Duration::ZERO, std::future::ready(42)).await;
  assert_eq!(output.unwrap(), 42);
}

/// Ticks that pile up while nobody is awaiting are delivered in a
/// burst, which matches `tokio`'s default missed-tick behavior.
#[wasm_bindgen_test]
async fn missed_interval_ticks_are_delivered_in_a_burst() {
  let mut ticker = interval(Duration::from_millis(100));
  sleep(Duration::from_millis(350)).await;
  let start = now();
  ticker.tick().await;
  ticker.tick().await;
  ticker.tick().await;
  let elapsed = now() - start;
  assert!(elapsed < 100.0, "queued ticks were not ready: {elapsed}ms");
}

#[wasm_bindgen_test]
async fn sleeps_run_concurrently_in_tasks() {
  let start = now();
  let first = tokio::spawn(sleep(Duration::from_millis(200)));
  let second = tokio::spawn(sleep(Duration::from_millis(200)));
  first.await.unwrap();
  second.await.unwrap();
  let elapsed = now() - start;
  // Sequential sleeps would take 400ms or more.
  assert!(elapsed < 390.0, "sleeps did not overlap: {elapsed}ms");
}
