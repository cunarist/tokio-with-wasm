//! Utilities for tracking time.
//!
//! This module provides a number of types for executing code after a set period
//! of time.

pub mod error;
mod instant;
mod interval;

use crate::Timer;
use error::Elapsed;
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::task::{Context, Poll, ready};

// Re-exported to match `tokio::time`, where `Duration` is available too.
pub use std::time::Duration;

pub use instant::Instant;
pub use interval::{Interval, MissedTickBehavior, interval, interval_at};

/// Web timers cut off at this many milliseconds: JavaScript stores the
/// delay of `setTimeout` in a 32-bit integer, and a longer delay fires
/// immediately. Longer waits chain timers, which tests do with a small cap.
const MAX_TIMER_MILLIS: f64 = if cfg!(test) { 50.0 } else { 2_147_483_647.0 };

/// Waits until `duration` has elapsed.
///
/// No work is performed while awaiting on the sleep future to complete.
/// `Sleep` operates at millisecond granularity, which is what JavaScript
/// timers offer, and should not be used for tasks that require
/// higher-resolution timing.
pub fn sleep(duration: Duration) -> Sleep {
  sleep_until(
    Instant::now()
      .checked_add(duration)
      .unwrap_or_else(Instant::far_future),
  )
}

/// Waits until `deadline` is reached.
///
/// No work is performed while awaiting on the sleep future to complete.
pub fn sleep_until(deadline: Instant) -> Sleep {
  Sleep {
    deadline,
    timer: None,
  }
}

/// Future returned by [`sleep`] and [`sleep_until`].
pub struct Sleep {
  deadline: Instant,
  /// The pending JavaScript timer, created lazily on the first poll.
  /// [`None`] before the first poll and after a reset.
  timer: Option<Timer>,
}

impl Sleep {
  /// Returns the instant at which the future will complete.
  pub fn deadline(&self) -> Instant {
    self.deadline
  }

  /// Returns `true` if `Sleep` has elapsed.
  ///
  /// A `Sleep` instance is elapsed when the requested duration has elapsed.
  pub fn is_elapsed(&self) -> bool {
    Instant::now() >= self.deadline
  }

  /// Resets the `Sleep` instance to a new deadline.
  ///
  /// Calling this function allows changing the instant at which the `Sleep`
  /// future completes without having to create new associated state.
  ///
  /// This function can be called both before and after the future has
  /// completed.
  ///
  /// To call this method, you will usually combine the call with
  /// [`Pin::as_mut`], which lets you call the method without consuming the
  /// `Sleep` itself.
  pub fn reset(self: Pin<&mut Self>, deadline: Instant) {
    // `Sleep` is `Unpin`, so the pinned reference can be unwrapped.
    let this = self.get_mut();
    this.deadline = deadline;
    // Firing the old timer early makes the waiting task poll and set a new one.
    let timer = this.timer.take();
    if let Some(waker) = timer.and_then(|timer| timer.waker.take()) {
      waker.wake();
    }
  }
}

impl Future for Sleep {
  type Output = ();
  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    let this = self.get_mut();
    loop {
      let now = Instant::now();
      if now >= this.deadline {
        this.timer = None;
        return Poll::Ready(());
      }
      // Rounded up, so that the timer never fires before the deadline.
      let millis = ((this.deadline - now).as_secs_f64() * 1000.0).ceil();
      let timer = this
        .timer
        .get_or_insert_with(|| Timer::new(millis.min(MAX_TIMER_MILLIS)));
      ready!(Pin::new(timer).poll(cx));
      // The timer fired, but web timers only count whole milliseconds
      // and cap out at 32 bits, so the deadline check above decides
      // whether to complete or to arm the next timer.
      this.timer = None;
    }
  }
}

impl std::fmt::Debug for Sleep {
  fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    fmt
      .debug_struct("Sleep")
      .field("deadline", &self.deadline)
      .finish()
  }
}

/// Requires a `Future` to complete before the specified duration has elapsed.
///
/// If the future completes before the duration has elapsed, then the
/// completed value is returned. Otherwise, an error is returned and the
/// future is canceled.
///
/// Note that the timeout is checked before polling the future, so if the
/// future does not yield during execution then it is possible for the
/// future to complete and exceed the timeout _without_ returning an error.
pub fn timeout<F>(duration: Duration, future: F) -> Timeout<F::IntoFuture>
where
  F: IntoFuture,
{
  Timeout {
    value: future.into_future(),
    delay: sleep(duration),
  }
}

/// Requires a `Future` to complete before the specified instant in time.
///
/// If the future completes before the instant is reached, then the
/// completed value is returned. Otherwise, an error is returned.
pub fn timeout_at<F>(deadline: Instant, future: F) -> Timeout<F::IntoFuture>
where
  F: IntoFuture,
{
  Timeout {
    value: future.into_future(),
    delay: sleep_until(deadline),
  }
}

/// Future returned by [`timeout`] and [`timeout_at`].
#[derive(Debug)]
pub struct Timeout<F> {
  value: F,
  delay: Sleep,
}

impl<F> Timeout<F> {
  /// Gets a reference to the underlying value in this timeout.
  pub fn get_ref(&self) -> &F {
    &self.value
  }

  /// Gets a mutable reference to the underlying value in this timeout.
  pub fn get_mut(&mut self) -> &mut F {
    &mut self.value
  }

  /// Consumes this timeout, returning the underlying value.
  pub fn into_inner(self) -> F {
    self.value
  }
}

impl<F: Future> Future for Timeout<F> {
  type Output = Result<F::Output, Elapsed>;
  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    // Safety: `value` is never moved out of the pinned struct.
    let this = unsafe { self.get_unchecked_mut() };
    // The future goes first, so that its output wins over the deadline,
    // like in `tokio`.
    if let Poll::Ready(output) =
      unsafe { Pin::new_unchecked(&mut this.value) }.poll(cx)
    {
      return Poll::Ready(Ok(output));
    }
    Pin::new(&mut this.delay)
      .poll(cx)
      .map(|()| Err(Elapsed::new()))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use wasm_bindgen_test::wasm_bindgen_test;

  /// The test build caps a single JavaScript timer at 50ms, so this
  /// sleep can only complete on time by chaining six timers, the same
  /// way a 25-day sleep must on the real 32-bit cap. Completing at the
  /// first timer would wake up at 50ms and fail the assertion.
  #[wasm_bindgen_test]
  async fn sleeps_chain_timers_beyond_the_single_timer_cap() {
    let start = Instant::now();
    sleep(Duration::from_millis(300)).await;
    let elapsed = start.elapsed();
    assert!(
      elapsed >= Duration::from_millis(250),
      "the sleep ended at the timer cap: {elapsed:?}"
    );
    assert!(
      elapsed < Duration::from_secs(5),
      "the sleep chained too long: {elapsed:?}"
    );
  }

  /// A timeout that outlives the timer cap must not elapse early.
  #[wasm_bindgen_test]
  async fn timeouts_survive_the_single_timer_cap() {
    let output = timeout(Duration::from_millis(300), async {
      sleep(Duration::from_millis(150)).await;
      42
    })
    .await;
    assert_eq!(output.ok(), Some(42));
  }
}
