//! Browser tests for `Sleep` and `sleep_until`.
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

use js_sys::WebAssembly::Memory;
use js_sys::{Array, Function, Reflect, global};
use std::future::Future;
use std::pin::pin;
use std::task::{Context, Waker};
use tokio_with_wasm::time::{Duration, Instant, sleep, sleep_until};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn sleep_until_waits_for_the_deadline() {
  let start = Instant::now();
  sleep_until(start + Duration::from_millis(200)).await;
  let elapsed = start.elapsed();
  assert!(
    elapsed >= Duration::from_millis(150),
    "woke up too early: {elapsed:?}"
  );
}

#[wasm_bindgen_test]
async fn sleep_until_a_past_deadline_completes_right_away() {
  let start = Instant::now();
  sleep_until(start - Duration::from_millis(500)).await;
  let elapsed = start.elapsed();
  assert!(
    elapsed < Duration::from_millis(150),
    "a past deadline still waited: {elapsed:?}"
  );
}

#[wasm_bindgen_test]
fn the_deadline_is_reported_back() {
  let deadline = Instant::now() + Duration::from_secs(3);
  let sleep_future = sleep_until(deadline);
  assert_eq!(sleep_future.deadline(), deadline);
}

#[wasm_bindgen_test]
async fn is_elapsed_flips_after_completion() {
  let sleep_future = sleep(Duration::from_millis(100));
  assert!(!sleep_future.is_elapsed());
  let mut pinned = pin!(sleep_future);
  pinned.as_mut().await;
  assert!(pinned.is_elapsed());
}

#[wasm_bindgen_test]
async fn reset_moves_the_deadline_back() {
  let start = Instant::now();
  let mut sleep_future = pin!(sleep(Duration::from_millis(100)));
  // The wait grows from 100ms to 300ms before it is awaited.
  let new_deadline = start + Duration::from_millis(300);
  sleep_future.as_mut().reset(new_deadline);
  assert_eq!(sleep_future.deadline(), new_deadline);
  sleep_future.await;
  let elapsed = start.elapsed();
  assert!(
    elapsed >= Duration::from_millis(250),
    "the reset deadline was ignored: {elapsed:?}"
  );
}

#[wasm_bindgen_test]
async fn reset_revives_a_completed_sleep() {
  let mut sleep_future = pin!(sleep(Duration::from_millis(50)));
  sleep_future.as_mut().await;
  assert!(sleep_future.is_elapsed());

  let start = Instant::now();
  sleep_future
    .as_mut()
    .reset(start + Duration::from_millis(200));
  assert!(!sleep_future.is_elapsed());
  sleep_future.as_mut().await;
  let elapsed = start.elapsed();
  assert!(
    elapsed >= Duration::from_millis(150),
    "the revived sleep completed early: {elapsed:?}"
  );
}

/// A wait far over the 32-bit millisecond limit of JavaScript timers
/// must stay asleep instead of firing immediately.
#[wasm_bindgen_test]
async fn very_long_sleeps_do_not_fire_early() {
  let far_away = Instant::now() + Duration::from_secs(86400 * 365);
  let long_sleep = sleep_until(far_away);
  let quick_nap = sleep(Duration::from_millis(200));
  let output =
    tokio_with_wasm::time::timeout(Duration::from_millis(100), long_sleep)
      .await;
  assert!(output.is_err(), "the year-long sleep completed");
  quick_nap.await;
}

#[wasm_bindgen_test]
async fn a_dropped_sleep_clears_its_timer() {
  // Records every timer that gets cleared, then puts the original back.
  let spy = Function::new_no_args(
    "const clear = globalThis.clearTimeout;
     globalThis.cleared = [];
     globalThis.clearTimeout = id => { cleared.push(id); clear(id); };
     return clear;",
  );
  let original = spy.call0(&global()).unwrap_or_default();
  {
    let mut sleeping = Box::pin(sleep(Duration::from_secs(3600)));
    let mut context = Context::from_waker(Waker::noop());
    assert!(sleeping.as_mut().poll(&mut context).is_pending());
  }
  let cleared = Reflect::get(&global(), &"cleared".into()).unwrap_or_default();
  let _ = Reflect::set(&global(), &"clearTimeout".into(), &original);
  assert_eq!(Array::from(&cleared).length(), 1, "the timer is still set");
}

/// Like `tokio`, a reset wakes the task waiting on the old deadline.
#[wasm_bindgen_test]
async fn reset_keeps_the_waiting_task_awake() {
  let start = Instant::now();
  let mut sleeping = pin!(sleep(Duration::from_secs(3600)));
  let mut is_reset = false;
  let waiting = std::future::poll_fn(|cx| {
    let poll = sleeping.as_mut().poll(cx);
    if !is_reset {
      is_reset = true;
      sleeping.as_mut().reset(start + Duration::from_millis(50));
    }
    poll
  });
  let _ = tokio_with_wasm::time::timeout(Duration::from_secs(1), waiting).await;
  let elapsed = start.elapsed();
  assert!(elapsed < Duration::from_millis(500), "woke at {elapsed:?}");
}

#[wasm_bindgen_test]
async fn cancelled_sleeps_do_not_leak() {
  let pages = || wasm_bindgen::memory().unchecked_into::<Memory>().grow(0);
  let before = pages();
  for _ in 0..50_000 {
    let _ = pin!(sleep(Duration::from_secs(3600)))
      .poll(&mut Context::from_waker(Waker::noop()));
  }
  let grown = pages() - before;
  assert!(grown < 16, "leaked {grown} pages of 64 KiB");
}
