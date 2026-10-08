//! Cooperative scheduling helpers, mirroring `tokio::task::coop`.
//!
//! Real `tokio` gives each task a poll budget that its resources consume.
//! The JavaScript event loop has no such budget, so this module keeps one
//! counter per thread, shared by its tasks: every 128th [`consume_budget`]
//! call yields to the event loop. That approximates how a budget-exhausted
//! `tokio` task gets rescheduled.

use crate::yield_now;
use std::cell::Cell;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// How many [`consume_budget`] calls make one yield,
/// the same number that `tokio` budgets for each task poll.
const BUDGET: u32 = 128;

thread_local! {
  static REMAINING_BUDGET: Cell<u32> = const { Cell::new(BUDGET) };
  /// Set while an [`Unconstrained`] future is being polled,
  /// which turns `consume_budget` into a no-op.
  static IS_UNCONSTRAINED: Cell<bool> = const { Cell::new(false) };
}

/// Consumes a unit of budget and returns the execution back to the
/// JavaScript event loop if the thread's coop budget was exhausted.
///
/// This lets long computations that never otherwise `.await`
/// stay responsive, the same way it prevents starvation in `tokio`:
///
/// ```no_run
/// use tokio_with_wasm::alias as tokio;
///
/// async fn sum_iterator(
///   input: &mut impl std::iter::Iterator<Item = i64>,
/// ) -> i64 {
///   let mut sum: i64 = 0;
///   while let Some(i) = input.next() {
///     sum += i;
///     tokio::task::coop::consume_budget().await
///   }
///   sum
/// }
/// ```
pub async fn consume_budget() {
  if IS_UNCONSTRAINED.with(|cell| cell.get()) {
    return;
  }
  let depleted = REMAINING_BUDGET.with(|cell| {
    let remaining = cell.get() - 1;
    cell.set(if remaining == 0 { BUDGET } else { remaining });
    remaining == 0
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
