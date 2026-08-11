//! Utilities for tracking time.
//!
//! This module provides a number of types for executing code after a set period
//! of time.
//!
//! `Instant` has no counterpart here, because the standard library cannot
//! read a clock on `wasm32-unknown-unknown`.

use crate::{
  LocalReceiver, LogError, clear_interval, local_channel, set_interval,
  set_timeout,
};
use js_sys::Promise;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use wasm_bindgen::prelude::{Closure, JsCast};
use wasm_bindgen_futures::JsFuture;

// Re-exported to match `tokio::time`, where `Duration` is available too.
pub use std::time::Duration;

async fn time_future(duration: Duration) {
  let milliseconds = duration.as_millis() as f64;
  let promise = Promise::new(&mut |resolve, _reject| {
    set_timeout(&resolve, milliseconds);
  });
  JsFuture::from(promise).await.log_error("TIME_FUTURE");
}

/// Waits until `duration` has elapsed.
pub fn sleep(duration: Duration) -> Sleep {
  let time_future = time_future(duration);
  Sleep {
    time_future: Box::pin(time_future),
  }
}

/// Future returned by `sleep`.
pub struct Sleep {
  time_future: Pin<Box<dyn Future<Output = ()>>>,
}

impl Future for Sleep {
  type Output = ();
  fn poll(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Self::Output> {
    self.time_future.as_mut().poll(cx)
  }
}

/// Poll a future with a timeout.
/// If the future is ready, return the output.
/// If the future is pending, poll the sleep future.
pub fn timeout<F>(duration: Duration, future: F) -> Timeout<F>
where
  F: Future,
{
  let time_future = time_future(duration);
  Timeout {
    future: Box::pin(future),
    time_future: Box::pin(time_future),
  }
}

/// Future returned by `timeout`.
pub struct Timeout<F: Future> {
  future: Pin<Box<F>>,
  time_future: Pin<Box<dyn Future<Output = ()>>>,
}

impl<F: Future> Future for Timeout<F> {
  type Output = Result<F::Output, Elapsed>;
  fn poll(
    mut self: Pin<&mut Self>,
    cx: &mut Context<'_>,
  ) -> Poll<Self::Output> {
    // Poll the future first.
    // If it's ready, return the output.
    // If it's pending, poll the sleep future.
    match self.future.as_mut().poll(cx) {
      Poll::Ready(output) => Poll::Ready(Ok(output)),
      Poll::Pending => match self.time_future.as_mut().poll(cx) {
        Poll::Ready(()) => Poll::Ready(Err(Elapsed(()))),
        Poll::Pending => Poll::Pending,
      },
    }
  }
}

/// Errors returned by `Timeout`.
///
/// This error is returned when a timeout expires before the function was able
/// to finish.
#[derive(Debug, PartialEq, Eq)]
pub struct Elapsed(());

impl Display for Elapsed {
  fn fmt(&self, fmt: &mut Formatter<'_>) -> std::fmt::Result {
    "deadline has elapsed".fmt(fmt)
  }
}

impl Error for Elapsed {}

impl From<Elapsed> for io::Error {
  fn from(_err: Elapsed) -> io::Error {
    io::ErrorKind::TimedOut.into()
  }
}

/// Creates a new interval that ticks every `period` duration.
///
/// The first tick completes immediately, as it does in `tokio`.
pub fn interval(period: Duration) -> Interval {
  Interval {
    ticker: start_ticking(period, true),
    period,
  }
}

/// Registers a JavaScript interval that feeds a channel.
/// The first tick is queued right away when `immediate` is set.
fn start_ticking(period: Duration, immediate: bool) -> Ticker {
  let (sender, receiver) = local_channel::<()>();
  if immediate {
    sender.send(());
  }
  let closure = Closure::wrap(Box::new(move || {
    sender.send(());
  }) as Box<dyn Fn()>);
  let interval_id =
    set_interval(closure.as_ref().unchecked_ref(), period.as_millis() as f64);
  Ticker {
    receiver,
    closure,
    interval_id,
  }
}

/// A registered JavaScript interval and the channel it feeds.
struct Ticker {
  receiver: LocalReceiver<()>,
  /// Held so that JavaScript can keep calling it.
  /// Handing it to the JavaScript garbage collector instead would leak it,
  /// because a `Closure` created by Rust is never collected.
  #[allow(dead_code)]
  closure: Closure<dyn Fn()>,
  interval_id: i32,
}

impl Drop for Ticker {
  fn drop(&mut self) {
    // The interval is cleared before the closure it calls is freed.
    clear_interval(self.interval_id);
  }
}

/// A structure that represents an interval that ticks at a specified period.
/// It provides methods to wait for the next tick, reset the interval,
/// and ensure the interval is cleaned up when it is dropped.
pub struct Interval {
  period: Duration,
  ticker: Ticker,
}

impl Interval {
  /// Waits until the next tick.
  pub async fn tick(&mut self) {
    self.ticker.receiver.next().await;
  }

  /// Resets the interval, making the next tick occur
  /// after the original period.
  /// This clears the existing interval and establishes a new one,
  /// dropping any tick that has already been missed.
  pub fn reset(&mut self) {
    self.ticker = start_ticking(self.period, false);
  }
}
