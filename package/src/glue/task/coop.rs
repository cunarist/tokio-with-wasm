//! Cooperative scheduling helpers, mirroring `tokio::task::coop`.
//!
//! Real `tokio` gives each task a poll budget that its resources consume.
//! The JavaScript event loop has no such budget, so this module keeps a
//! per-thread counter instead: after a task burns through the counter with
//! [`consume_budget`] calls, one call yields to the event loop and the
//! counter refills. That approximates how a budget-exhausted `tokio` task
//! gets rescheduled without ever blocking the browser.

use crate::yield_now;
use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// How many [`consume_budget`] calls run between yields,
/// matching the budget that `tokio` assigns to each task poll.
const BUDGET: u32 = 128;

thread_local! {
  static REMAINING_BUDGET: Cell<u32> = const { Cell::new(BUDGET) };
  /// Set while an [`Unconstrained`] future is being polled,
  /// which turns `consume_budget` into a no-op.
  static IS_UNCONSTRAINED: Cell<bool> = const { Cell::new(false) };
}

/// Consumes a unit of budget and returns the execution back to the
/// JavaScript event loop if the task's coop budget was exhausted.
///
/// This lets long computations that never otherwise `.await`
/// stay responsive, the same way it prevents starvation in `tokio`:
///
/// ```no_run
/// use tokio_with_wasm::alias as tokio;
///
/// async fn sum_iterator(input: &mut impl std::iter::Iterator<Item=i64>) -> i64 {
///     let mut sum: i64 = 0;
///     while let Some(i) = input.next() {
///         sum += i;
///         tokio::task::consume_budget().await
///     }
///     sum
/// }
/// ```
pub async fn consume_budget() {
  if IS_UNCONSTRAINED.with(|cell| cell.get()) {
    return;
  }
  let depleted = REMAINING_BUDGET.with(|cell| {
    let remaining = cell.get();
    if remaining == 0 {
      cell.set(BUDGET);
      true
    } else {
      cell.set(remaining - 1);
      false
    }
  });
  if depleted {
    yield_now().await;
  }
}

/// Turns off the cooperative scheduling for a future.
/// The future will never be forced to yield by [`consume_budget`].
pub fn unconstrained<F>(inner: F) -> Unconstrained<F> {
  Unconstrained { inner }
}

/// Future for the [`unconstrained`] method.
pub struct Unconstrained<F> {
  inner: F,
}

impl<F: Future> Future for Unconstrained<F> {
  type Output = F::Output;
  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    // Safety: `inner` is never moved out of the pinned struct.
    let inner = unsafe { self.map_unchecked_mut(|this| &mut this.inner) };
    let previous = IS_UNCONSTRAINED.replace(true);
    let polled = inner.poll(cx);
    IS_UNCONSTRAINED.set(previous);
    polled
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use wasm_bindgen_test::wasm_bindgen_test;

  #[wasm_bindgen_test]
  async fn consume_budget_completes_many_times() {
    // More calls than the budget holds, so at least one yield happens.
    for _ in 0..(BUDGET * 2 + 1) {
      consume_budget().await;
    }
  }

  #[wasm_bindgen_test]
  async fn unconstrained_passes_the_output_through() {
    let output = unconstrained(async { 42 }).await;
    assert_eq!(output, 42);
  }

  #[wasm_bindgen_test]
  async fn unconstrained_skips_the_budget() {
    REMAINING_BUDGET.with(|cell| cell.set(BUDGET));
    unconstrained(async {
      for _ in 0..(BUDGET * 2 + 1) {
        consume_budget().await;
      }
    })
    .await;
    // An unconstrained future must not have touched the counter.
    assert_eq!(REMAINING_BUDGET.with(|cell| cell.get()), BUDGET);
  }
}
