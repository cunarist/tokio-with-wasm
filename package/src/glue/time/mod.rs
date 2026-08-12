//! Utilities for tracking time.
//!
//! This module provides a number of types for executing code after a set period
//! of time.
//!
//! [`Instant`] is backed by JavaScript's `performance.now()`, because the
//! standard library cannot read a clock on `wasm32-unknown-unknown`. It is a
//! separate type from `std::time::Instant`, so `Instant::from_std` and
//! `Instant::into_std` have no counterpart here.

pub mod error;
mod instant;
mod interval;

use crate::{LogError, set_timeout};
use js_sys::Promise;
use std::future::{Future, IntoFuture};
use std::pin::Pin;
use std::task::{Context, Poll};
use wasm_bindgen_futures::JsFuture;

// Re-exported to match `tokio::time`, where `Duration` is available too.
pub use std::time::Duration;

pub use error::Elapsed;
pub use instant::Instant;
pub use interval::{Interval, MissedTickBehavior, interval, interval_at};

/// Web timers cut off at this many milliseconds: JavaScript stores the
/// delay of `setTimeout` in a 32-bit integer, and a longer delay fires
/// immediately instead of far in the future. Longer waits chain timers.
const MAX_TIMER_MILLIS: f64 = 2_147_483_647.0;

/// Resolves after `duration` through one JavaScript timer.
async fn time_future(duration: Duration) {
  // Rounded up, so that the timer never fires before the deadline
  // and forces an extra timer round for well-known durations.
  let milliseconds = (duration.as_secs_f64() * 1000.0)
    .ceil()
    .min(MAX_TIMER_MILLIS);
  let promise = Promise::new(&mut |resolve, _reject| {
    set_timeout(&resolve, milliseconds);
  });
  JsFuture::from(promise).await.log_error("TIME_FUTURE");
}

/// Waits until `duration` has elapsed.
///
/// No work is performed while awaiting on the sleep future to complete.
/// `Sleep` operates at millisecond granularity, which is what JavaScript
/// timers offer, and should not be used for tasks that require
/// higher-resolution timing.
pub fn sleep(duration: Duration) -> Sleep {
  let deadline = match Instant::now().checked_add(duration) {
    Some(deadline) => deadline,
    None => Instant::far_future(),
  };
  sleep_until(deadline)
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
  timer: Option<Pin<Box<dyn Future<Output = ()>>>>,
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
    // The running timer aims at the old deadline, so it is discarded.
    // The next poll starts a new one.
    this.timer = None;
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
      if this.timer.is_none() {
        let remaining = this.deadline.saturating_duration_since(now);
        this.timer = Some(Box::pin(time_future(remaining)));
      }
      let Some(timer) = this.timer.as_mut() else {
        // The timer was just stored above.
        return Poll::Pending;
      };
      match timer.as_mut().poll(cx) {
        // The timer fired, but web timers only count whole milliseconds
        // and cap out at 32 bits, so the deadline check above decides
        // whether to complete or to arm the next timer.
        Poll::Ready(()) => this.timer = None,
        Poll::Pending => return Poll::Pending,
      }
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
    // Safety: `value` is never moved out of the pinned struct,
    // and `delay` is `Unpin`.
    let (value, delay) = unsafe {
      let this = self.get_unchecked_mut();
      (
        Pin::new_unchecked(&mut this.value),
        Pin::new(&mut this.delay),
      )
    };
    // Poll the future first. If it's ready, return the output
    // even when the deadline has passed, like in `tokio`.
    match value.poll(cx) {
      Poll::Ready(output) => Poll::Ready(Ok(output)),
      Poll::Pending => match delay.poll(cx) {
        Poll::Ready(()) => Poll::Ready(Err(Elapsed::new())),
        Poll::Pending => Poll::Pending,
      },
    }
  }
}
