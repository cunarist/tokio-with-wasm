//! Utilities for tracking time.
//!
//! This module provides a number of types for executing code after a set period
//! of time.

use std::error;
use std::fmt;
use std::future::{Future, IntoFuture};
use std::io;
use std::pin::Pin;
use std::result::Result as StdResult;
use std::task::{Context, Poll};
use std::time::Duration;

use crate::glue::common::*;

/// Waits until `duration` has elapsed.
///
/// Because this is a naive implemenation
/// based on `setTimeout()` of JavaScript,
/// web browsers might increase the interval arbitrarily
/// to save system resources.
pub async fn sleep(duration: Duration) {
    use wasm_bindgen::prelude::*;
    let milliseconds = duration.as_millis() as f64;
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        set_timeout(&resolve, milliseconds);
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

/// Poll a future with a timeout.
/// If the future is ready, return the output.
/// If the future is pending, poll the sleep future.
pub fn timeout<F>(duration: Duration, future: F) -> Timeout<F>
where
    F: Future,
{
    let sleep = async move {
        sleep(duration).await;
        ()
    };
    Timeout {
        future: Box::pin(future.into_future()),
        sleep: Box::pin(sleep),
    }
}

pub struct Timeout<F: Future> {
    future: Pin<Box<F>>,
    sleep: Pin<Box<dyn Future<Output = ()>>>,
}

impl<F: Future> Future for Timeout<F> {
    type Output = StdResult<F::Output, Elapsed>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Poll the future first.
        // If it's ready, return the output.
        // If it's pending, poll the sleep future.
        match self.future.as_mut().poll(cx) {
            Poll::Ready(output) => Poll::Ready(Ok(output)),
            Poll::Pending => match self.sleep.as_mut().poll(cx) {
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
