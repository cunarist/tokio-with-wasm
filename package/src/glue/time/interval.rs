//! Intervals built on repeatedly reset [`Sleep`] futures.

use super::{Duration, Instant, Sleep, sleep_until};
use std::future::{Future, poll_fn};
use std::pin::Pin;
use std::task::{Context, Poll};

/// Creates new [`Interval`] that yields with interval of `period`. The first
/// tick completes immediately, as it does in `tokio`.
///
/// An interval will tick indefinitely. At any time, the [`Interval`] value can
/// be dropped. This cancels the interval.
///
/// # Panics
///
/// This function panics if `period` is zero.
pub fn interval(period: Duration) -> Interval {
  assert!(period > Duration::ZERO, "`period` must be non-zero.");
  interval_at(Instant::now(), period)
}

/// Creates new [`Interval`] that yields with interval of `period` and whose
/// first tick completes at `start`.
///
/// # Panics
///
/// This function panics if `period` is zero.
pub fn interval_at(start: Instant, period: Duration) -> Interval {
  assert!(period > Duration::ZERO, "`period` must be non-zero.");
  Interval {
    delay: sleep_until(start),
    period,
    missed_tick_behavior: MissedTickBehavior::default(),
  }
}

/// Defines the behavior of an [`Interval`] when it misses a tick.
///
/// Generally, a tick is missed if too much time is spent without calling
/// [`Interval::tick()`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MissedTickBehavior {
  /// Ticks as fast as possible until caught up.
  ///
  /// When this strategy is used, [`Interval`] schedules ticks "normally" (the
  /// same as it would have if the ticks hadn't been delayed), which results
  /// in it firing ticks as fast as possible until it is caught up in time to
  /// where it should be. Unlike [`Delay`] and [`Skip`], the ticks yielded
  /// when `Burst` is used (the [`Instant`]s that [`tick`] yields) aren't
  /// different than they would have been if a tick had not been missed.
  ///
  /// This is the default behavior when [`Interval`] is created with
  /// [`interval`] and [`interval_at`].
  ///
  /// [`Delay`]: MissedTickBehavior::Delay
  /// [`Skip`]: MissedTickBehavior::Skip
  /// [`tick`]: Interval::tick
  #[default]
  Burst,

  /// Tick at multiples of `period` from when [`tick`] was called, rather
  /// than from `start`.
  ///
  /// [`tick`]: Interval::tick
  Delay,

  /// Skips missed ticks and tick on the next multiple of `period` from
  /// `start`.
  Skip,
}

impl MissedTickBehavior {
  /// If a tick is missed, this method is called to determine when the next
  /// tick should happen.
  fn next_timeout(
    &self,
    timeout: Instant,
    now: Instant,
    period: Duration,
  ) -> Instant {
    match self {
      Self::Burst => timeout + period,
      Self::Delay => now + period,
      Self::Skip => {
        now + period
          - Duration::from_nanos(
            ((now - timeout).as_nanos() % period.as_nanos())
              .try_into()
              // The remainder is less than the period, which fit in a
              // `Duration`, so this only saturates on absurd periods
              // over 584 years.
              .unwrap_or(u64::MAX),
          )
      }
    }
  }
}

/// Interval returned by [`interval`] and [`interval_at`].
///
/// This type allows you to wait on a sequence of instants with a certain
/// duration between each instant. Unlike calling [`sleep`](super::sleep) in
/// a loop, this lets you count the time spent between the calls to
/// [`tick`](Self::tick) as well.
#[derive(Debug)]
pub struct Interval {
  /// Future that completes at the next tick's deadline.
  delay: Sleep,
  /// The duration between values yielded by [`Interval`].
  period: Duration,
  /// The strategy [`Interval`] should use when a tick is missed.
  missed_tick_behavior: MissedTickBehavior,
}

impl Interval {
  /// Completes when the next instant in the interval has been reached.
  ///
  /// # Cancel safety
  ///
  /// This method is cancellation safe. If `tick` is used as the branch in a
  /// `tokio::select!` and another branch completes first, then no tick has
  /// been consumed.
  pub async fn tick(&mut self) -> Instant {
    poll_fn(|cx| self.poll_tick(cx)).await
  }

  /// Polls for the next instant in the interval to be reached.
  ///
  /// This method can return the following values:
  ///
  ///  * `Poll::Pending` if the next instant has not yet been reached.
  ///  * `Poll::Ready(instant)` if the next instant has been reached.
  ///
  /// When this method returns `Poll::Pending`, the current task is scheduled
  /// to receive a wakeup when the instant has elapsed. Note that on multiple
  /// calls to `poll_tick`, only the [`Waker`](std::task::Waker) from the
  /// [`Context`] passed to the most recent call is scheduled to receive a
  /// wakeup.
  pub fn poll_tick(&mut self, cx: &mut Context<'_>) -> Poll<Instant> {
    if Pin::new(&mut self.delay).poll(cx).is_pending() {
      return Poll::Pending;
    }

    let timeout = self.delay.deadline();
    let now = Instant::now();

    // If a tick was missed by more than a small margin, let the
    // missed-tick behavior pick the next deadline. The margin absorbs
    // ordinary timer lateness, which JavaScript timers always have.
    let next = if now > timeout + Duration::from_millis(5) {
      self
        .missed_tick_behavior
        .next_timeout(timeout, now, self.period)
    } else {
      match timeout.checked_add(self.period) {
        Some(next) => next,
        None => Instant::far_future(),
      }
    };
    Pin::new(&mut self.delay).reset(next);

    Poll::Ready(timeout)
  }

  /// Resets the interval to complete one period after the current time.
  ///
  /// This is equivalent to calling `reset_at(Instant::now() + period)`.
  pub fn reset(&mut self) {
    self.reset_at(Instant::now() + self.period);
  }

  /// Resets the interval immediately.
  ///
  /// This is equivalent to calling `reset_at(Instant::now())`.
  pub fn reset_immediately(&mut self) {
    self.reset_at(Instant::now());
  }

  /// Resets the interval after the specified [`Duration`].
  ///
  /// This is equivalent to calling `reset_at(Instant::now() + after)`.
  pub fn reset_after(&mut self, after: Duration) {
    self.reset_at(Instant::now() + after);
  }

  /// Resets the interval to a [`Instant`] deadline.
  ///
  /// Sets the next tick to expire at the given instant. If the instant is in
  /// the past, then the [`MissedTickBehavior`] strategy will be used to
  /// catch up.
  pub fn reset_at(&mut self, deadline: Instant) {
    Pin::new(&mut self.delay).reset(deadline);
  }

  /// Returns the [`MissedTickBehavior`] strategy currently being used.
  pub fn missed_tick_behavior(&self) -> MissedTickBehavior {
    self.missed_tick_behavior
  }

  /// Sets the [`MissedTickBehavior`] strategy that should be used.
  pub fn set_missed_tick_behavior(&mut self, behavior: MissedTickBehavior) {
    self.missed_tick_behavior = behavior;
  }

  /// Returns the period of the interval.
  pub fn period(&self) -> Duration {
    self.period
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use wasm_bindgen_test::wasm_bindgen_test;

  #[wasm_bindgen_test]
  fn burst_keeps_the_original_schedule() {
    let behavior = MissedTickBehavior::Burst;
    let start = Instant::now();
    let period = Duration::from_millis(100);
    // Three periods were missed; the next deadline stays on the grid,
    // right after the missed one.
    let now = start + Duration::from_millis(350);
    assert_eq!(behavior.next_timeout(start, now, period), start + period);
  }

  #[wasm_bindgen_test]
  fn delay_restarts_the_schedule_from_now() {
    let behavior = MissedTickBehavior::Delay;
    let start = Instant::now();
    let period = Duration::from_millis(100);
    let now = start + Duration::from_millis(350);
    assert_eq!(behavior.next_timeout(start, now, period), now + period);
  }

  #[wasm_bindgen_test]
  fn skip_stays_on_the_grid() {
    let behavior = MissedTickBehavior::Skip;
    let start = Instant::now();
    let period = Duration::from_millis(100);
    // 350ms late: the 100, 200, and 300ms ticks were missed,
    // so the next tick lands on the 400ms grid point.
    let now = start + Duration::from_millis(350);
    assert_eq!(
      behavior.next_timeout(start, now, period),
      start + Duration::from_millis(400),
    );
  }

  #[wasm_bindgen_test]
  fn the_default_behavior_is_burst() {
    assert_eq!(MissedTickBehavior::default(), MissedTickBehavior::Burst);
    let ticker = interval(Duration::from_millis(100));
    assert_eq!(ticker.missed_tick_behavior(), MissedTickBehavior::Burst);
    assert_eq!(ticker.period(), Duration::from_millis(100));
  }
}
