//! Timing assertions use generous bounds, because headless browsers on
//! loaded CI machines fire timers late. Where possible, the assertions use
//! the returned tick instants instead, which are exact deadline values.

use super::{cancel, spy};
use std::future::{Future, ready};
use std::pin::pin;
use std::task::{Context, Waker};
use tokio_with_wasm::task::{JoinError, spawn_blocking};
use tokio_with_wasm::time::error::Elapsed;
use tokio_with_wasm::time::{
  Duration, Instant, MissedTickBehavior, interval, interval_at, sleep,
  sleep_until, timeout, timeout_at,
};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;

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

/// JavaScript wraps longer timer delays around 32 bits.
#[wasm_bindgen_test]
async fn a_long_sleep_caps_its_timer() {
  let delay = spy(
    "const set = setTimeout;
     let delay;
     globalThis.setTimeout = (run, ms) => set(run, (delay = ms));
     return () => { globalThis.setTimeout = set; return delay; };",
    async { cancel(sleep(Duration::from_millis(1 << 31))) },
  )
  .await;
  assert_eq!(delay.as_f64(), Some(2_147_483_647.0));
}

#[wasm_bindgen_test]
async fn a_dropped_sleep_clears_its_timer() {
  let cleared = spy(
    "const clear = clearTimeout;
     let count = 0;
     globalThis.clearTimeout = id => { count += 1; clear(id); };
     return () => { globalThis.clearTimeout = clear; return count; };",
    async { cancel(sleep(Duration::from_secs(3600))) },
  )
  .await;
  assert_eq!(cleared.as_f64(), Some(1.0), "the timer is still set");
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
  let _ = timeout(Duration::from_secs(1), waiting).await;
  let elapsed = start.elapsed();
  assert!(elapsed < Duration::from_millis(500), "woke at {elapsed:?}");
}

#[wasm_bindgen_test]
async fn cancelled_sleeps_do_not_leak() {
  use js_sys::WebAssembly::Memory;
  let pages = || wasm_bindgen::memory().unchecked_into::<Memory>().grow(0);
  let before = pages();
  for _ in 0..50_000 {
    let _ = pin!(sleep(Duration::from_secs(3600)))
      .poll(&mut Context::from_waker(Waker::noop()));
  }
  let grown = pages() - before;
  assert!(grown < 16, "leaked {grown} pages of 64 KiB");
}

#[wasm_bindgen_test]
async fn timeout_at_returns_the_output_in_time() -> Result<(), Elapsed> {
  let deadline = Instant::now() + Duration::from_secs(5);
  assert_eq!(timeout_at(deadline, async { 42 }).await?, 42);
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
  assert_eq!(timeout_at(past, ready(42)).await?, 42);
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
  let mut wrapped = timeout(Duration::from_secs(5), ready(7));
  let _borrowed: &std::future::Ready<i32> = wrapped.get_ref();
  let _mutable: &mut std::future::Ready<i32> = wrapped.get_mut();
  assert_eq!(wrapped.into_inner().await, 7);
}

#[wasm_bindgen_test]
async fn elapsed_converts_into_a_timed_out_io_error() {
  let output =
    timeout(Duration::from_millis(50), sleep(Duration::from_secs(10))).await;
  assert!(output.is_err_and(|error| {
    std::io::Error::from(error).kind() == std::io::ErrorKind::TimedOut
  }));
}

#[wasm_bindgen_test]
async fn ticks_report_their_scheduled_instants() {
  let period = Duration::from_millis(100);
  let mut ticker = interval(period);
  assert_eq!(ticker.period(), period);
  let first = ticker.tick().await;
  let second = ticker.tick().await;
  let third = ticker.tick().await;
  // The reported instants are the scheduled deadlines,
  // so they sit exactly one period apart.
  assert_eq!(second - first, period);
  assert_eq!(third - second, period);
}

#[wasm_bindgen_test]
async fn interval_at_starts_at_the_given_instant() {
  let start = Instant::now() + Duration::from_millis(200);
  let mut ticker = interval_at(start, Duration::from_millis(100));
  let first = ticker.tick().await;
  assert_eq!(first, start);
  assert!(
    Instant::now() >= start,
    "the first tick came before `start`"
  );
}

#[wasm_bindgen_test]
async fn burst_delivers_missed_ticks_immediately() {
  let period = Duration::from_millis(100);
  let mut ticker = interval(period);
  let first = ticker.tick().await;
  sleep(Duration::from_millis(350)).await;
  // The missed ticks arrive in a burst, still on the original schedule.
  let second = ticker.tick().await;
  let third = ticker.tick().await;
  assert_eq!(second - first, period);
  assert_eq!(third - second, period);
  assert!(Instant::now() > third, "burst ticks were not overdue");
}

#[wasm_bindgen_test]
async fn delay_reschedules_from_the_late_tick() {
  let period = Duration::from_millis(200);
  let mut ticker = interval(period);
  ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
  assert_eq!(ticker.missed_tick_behavior(), MissedTickBehavior::Delay);
  ticker.tick().await; // Immediate.
  sleep(Duration::from_millis(500)).await;
  let late = ticker.tick().await; // Overdue, delivered right away.
  let next = ticker.tick().await;
  // The next tick runs a full period after the late one was consumed,
  // so it lies more than a period past the missed deadline.
  // `Burst` would put it exactly one period past.
  assert!(
    next - late > period,
    "the delayed tick was not pushed back: {:?}",
    next - late
  );
}

#[wasm_bindgen_test]
async fn skip_stays_on_the_original_grid() {
  let period = Duration::from_millis(200);
  let start = Instant::now();
  let mut ticker = interval_at(start, period);
  ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
  ticker.tick().await; // Immediate.
  sleep(Duration::from_millis(500)).await;
  let late = ticker.tick().await; // Overdue, delivered right away.
  let next = ticker.tick().await;
  // The skipped schedule stays on multiples of the period from `start`.
  let offset = (next - start).as_millis() % period.as_millis();
  assert_eq!(offset, 0, "the tick left the grid: {next:?}");
  // `Burst` would deliver the missed tick one period past the late one.
  assert!(next - late > period, "the missed tick was not skipped");
}

#[wasm_bindgen_test]
async fn resets_move_the_next_tick() {
  let period = Duration::from_millis(100);
  let mut ticker = interval(period);
  ticker.tick().await; // Immediate.
  let start = Instant::now();
  ticker.reset_immediately();
  assert!(ticker.tick().await < start + period);
  let start = Instant::now();
  ticker.reset();
  assert!(ticker.tick().await >= start + period);
  let start = Instant::now();
  ticker.reset_after(period * 2);
  assert!(ticker.tick().await >= start + period * 2);
  let deadline = Instant::now() + period;
  ticker.reset_at(deadline);
  assert_eq!(ticker.tick().await, deadline);
}

#[wasm_bindgen_test]
async fn poll_tick_is_pending_before_the_period() {
  let mut ticker = interval(Duration::from_millis(200));
  ticker.tick().await; // Immediate.
  // Nothing has been waited on, so the next tick cannot be ready.
  let mut context = Context::from_waker(Waker::noop());
  assert!(ticker.poll_tick(&mut context).is_pending());
}

#[wasm_bindgen_test]
#[should_panic(expected = "`period` must be non-zero.")]
fn a_zero_period_panics() {
  let _ticker = interval(Duration::ZERO);
}
