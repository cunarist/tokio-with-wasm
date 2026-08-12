//! Browser tests for `Interval`, tick instants, and missed-tick behavior.
//! Run with `wasm-pack test --headless --chrome package`.
//!
//! Timing assertions use generous bounds, because headless browsers on
//! loaded CI machines fire timers late. Where possible, the assertions use
//! the returned tick instants instead, which are exact deadline values.

// The glue code only exists on the web target,
// so this file is empty everywhere else.
#![cfg(all(
  target_family = "wasm",
  target_vendor = "unknown",
  target_os = "unknown"
))]

use tokio_with_wasm::time::{
  Duration, Instant, MissedTickBehavior, interval, interval_at, sleep,
};
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn ticks_report_their_scheduled_instants() {
  let period = Duration::from_millis(100);
  let mut ticker = interval(period);
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
  // so its deadline lies at least a period past the missed deadline.
  assert!(
    next - late >= period,
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
  assert!(next > late, "the next tick did not move forward");
}

#[wasm_bindgen_test]
async fn reset_immediately_makes_the_next_tick_instant() {
  let mut ticker = interval(Duration::from_millis(300));
  ticker.tick().await; // Immediate.
  let start = Instant::now();
  ticker.reset_immediately();
  ticker.tick().await;
  let elapsed = start.elapsed();
  assert!(
    elapsed < Duration::from_millis(150),
    "the reset tick still waited: {elapsed:?}"
  );
}

#[wasm_bindgen_test]
async fn reset_after_waits_the_given_duration() {
  let mut ticker = interval(Duration::from_millis(100));
  ticker.tick().await; // Immediate.
  let start = Instant::now();
  ticker.reset_after(Duration::from_millis(300));
  ticker.tick().await;
  let elapsed = start.elapsed();
  assert!(
    elapsed >= Duration::from_millis(250),
    "the tick ignored the new delay: {elapsed:?}"
  );
}

#[wasm_bindgen_test]
async fn reset_at_waits_for_the_given_deadline() {
  let mut ticker = interval(Duration::from_millis(100));
  ticker.tick().await; // Immediate.
  let deadline = Instant::now() + Duration::from_millis(300);
  ticker.reset_at(deadline);
  let tick = ticker.tick().await;
  assert_eq!(tick, deadline);
  assert!(
    Instant::now() >= deadline,
    "the tick came before its deadline"
  );
}

#[wasm_bindgen_test]
async fn poll_tick_is_pending_before_the_period() {
  let mut ticker = interval(Duration::from_millis(200));
  ticker.tick().await; // Immediate.
  // Nothing has been waited on, so the next tick cannot be ready.
  let pending = std::future::poll_fn(|cx| {
    std::task::Poll::Ready(ticker.poll_tick(cx).is_pending())
  })
  .await;
  assert!(pending, "a fresh period was already ready");
}

#[wasm_bindgen_test]
async fn the_period_is_reported_back() {
  let ticker = interval(Duration::from_millis(250));
  assert_eq!(ticker.period(), Duration::from_millis(250));
}
