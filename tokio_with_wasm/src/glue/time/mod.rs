//! Utilities for tracking time.
//!
//! This module provides a number of types for executing code after a set period
//! of time.

use crate::glue::common::{
    clear_interval, error, internal_channel, set_interval, set_timeout,
    InternalReceiver, LogError,
};
use js_sys::Promise;
use std::error;
use std::fmt;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use wasm_bindgen::prelude::{Closure, JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

async fn time_future(duration: Duration) {
    let milliseconds = duration.as_millis() as f64;
    let promise = Promise::new(&mut |resolve, _reject| {
        set_timeout(&resolve, milliseconds);
    });
    JsFuture::from(promise).await.log_error("TIME_FUTURE");
}

/// Waits until `duration` has elapsed.
///
/// Because this is a naive implemenation
/// based on `setTimeout()` of JavaScript,
/// web browsers might increase the interval arbitrarily
/// to save system resources.
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
    type Output = std::result::Result<F::Output, Elapsed>;
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

impl fmt::Display for Elapsed {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        "deadline has elapsed".fmt(fmt)
    }
}

impl error::Error for Elapsed {}

impl From<Elapsed> for io::Error {
    fn from(_err: Elapsed) -> io::Error {
        io::ErrorKind::TimedOut.into()
    }
}

/// Creates a new interval that ticks every `period` duration.
pub fn interval(period: Duration) -> Interval {
    let (tx, rx) = internal_channel::<()>();
    let period_ms = period.as_millis() as f64;
    // Create a closure that sends a tick via the channel.
    let closure = Closure::wrap(Box::new(move || {
        tx.send(());
    }) as Box<dyn Fn()>);
    // Register an interval with the closure.
    let interval_id = set_interval(closure.as_ref().unchecked_ref(), period_ms);
    // Release memory management of this closure from Rust to the JS GC.
    closure.forget();
    Interval {
        period,
        rx,
        interval_id,
    }
}

/// A structure that represents an interval that ticks at a specified period.
/// It provides methods to wait for the next tick, reset the interval,
/// and ensure the interval is cleaned up when it is dropped.
pub struct Interval {
    period: Duration,
    rx: InternalReceiver<()>,
    interval_id: i32,
}

impl Interval {
    /// Waits until the next tick.
    pub async fn tick(&mut self) {
        self.rx.next().await;
    }

    /// Resets the interval, making the next tick occur
    /// after the original period.
    /// This clears the existing interval and establishes a new one.
    pub fn reset(&mut self) {
        // Clear the existing interval.
        clear_interval(self.interval_id);
        // Create a new channel to receive ticks.
        let (tx, rx) = internal_channel::<()>();
        self.rx = rx;
        let period_ms = self.period.as_millis() as f64;
        // Set up a new interval.
        let closure = Closure::wrap(Box::new(move || {
            tx.send(());
        }) as Box<dyn Fn()>);
        self.interval_id =
            set_interval(closure.as_ref().unchecked_ref(), period_ms);
        // Release memory management of this closure from Rust to the JS GC.
        closure.forget();
    }
}

impl Drop for Interval {
    fn drop(&mut self) {
        clear_interval(self.interval_id);
    }
}
