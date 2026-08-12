//! A monotonic clock backed by JavaScript's `performance.now()`.

use std::cell::Cell;
use std::ops::{Add, AddAssign, Sub, SubAssign};
use std::time::Duration;
use wasm_bindgen::{JsCast, JsValue};

thread_local! {
  /// The `performance` object of this thread's JavaScript context,
  /// together with its `timeOrigin`. Every worker has its own origin,
  /// so the origin is added back in to make instants from different
  /// threads comparable.
  static PERFORMANCE: Option<(f64, web_sys::Performance)> =
    js_sys::Reflect::get(&js_sys::global(), &JsValue::from_str("performance"))
      .ok()
      .and_then(|value| value.dyn_into::<web_sys::Performance>().ok())
      .map(|performance| (performance.time_origin(), performance));
}

/// Milliseconds since the JavaScript epoch, from a monotonic clock.
fn epoch_millis() -> f64 {
  PERFORMANCE.with(|performance| match performance {
    Some((time_origin, performance)) => time_origin + performance.now(),
    // A JavaScript runtime without the `performance` web API;
    // the wall clock is the only clock left. It can jump backwards,
    // which the clamp in `monotonic` absorbs.
    None => js_sys::Date::now(),
  })
}

thread_local! {
  /// The largest reading handed out on this thread, so that
  /// [`Instant::now`] never goes backwards even on the wall-clock
  /// fallback.
  static LAST_NOW: Cell<Duration> = const { Cell::new(Duration::ZERO) };
}

/// Clamps a clock reading to the largest one seen so far.
fn monotonic(since_epoch: Duration) -> Duration {
  LAST_NOW.with(|last| {
    let clamped = since_epoch.max(last.get());
    last.set(clamped);
    clamped
  })
}

/// A measurement of a monotonically nondecreasing clock.
/// Opaque and useful only with [`Duration`].
///
/// Instants are always guaranteed to be no less than any previously
/// measured instant when created, and are often useful for tasks such as
/// measuring benchmarks or timing how long an operation takes.
///
/// # Notes
///
/// Unlike `tokio::time::Instant`, this type is not a wrapper around
/// `std::time::Instant`, which cannot read a clock on
/// `wasm32-unknown-unknown`. It reads JavaScript's `performance.now()`
/// instead, so `from_std` and `into_std` have no counterpart here.
// No `Default`, because `tokio::time::Instant` has none either;
// deriving it here would let non-portable code slip through.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant {
  /// Time elapsed since the JavaScript time origin epoch.
  since_epoch: Duration,
}

impl Instant {
  /// Returns an instant corresponding to "now".
  pub fn now() -> Instant {
    let millis = epoch_millis();
    let since_epoch = if millis.is_finite() && millis > 0.0 {
      Duration::try_from_secs_f64(millis / 1000.0).unwrap_or(Duration::ZERO)
    } else {
      Duration::ZERO
    };
    Instant {
      since_epoch: monotonic(since_epoch),
    }
  }

  /// A deadline for waits without a real deadline, far enough away
  /// that it never fires within a page's lifetime.
  pub(crate) fn far_future() -> Instant {
    // Roughly 30 years from now, like in `tokio`.
    Instant {
      since_epoch: Instant::now()
        .since_epoch
        .saturating_add(Duration::from_secs(86400 * 365 * 30)),
    }
  }

  /// Returns the amount of time elapsed from another instant to this one, or
  /// zero duration if that instant is later than this one.
  pub fn duration_since(&self, earlier: Instant) -> Duration {
    self.saturating_duration_since(earlier)
  }

  /// Returns the amount of time elapsed from another instant to this one, or
  /// `None` if that instant is later than this one.
  pub fn checked_duration_since(&self, earlier: Instant) -> Option<Duration> {
    self.since_epoch.checked_sub(earlier.since_epoch)
  }

  /// Returns the amount of time elapsed from another instant to this one, or
  /// zero duration if that instant is later than this one.
  pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
    self.since_epoch.saturating_sub(earlier.since_epoch)
  }

  /// Returns the amount of time elapsed since this instant was created,
  /// or zero duration if this instant is in the future.
  pub fn elapsed(&self) -> Duration {
    Instant::now().saturating_duration_since(*self)
  }

  /// Returns `Some(t)` where `t` is the time `self + duration` if `t` can be
  /// represented as `Instant`, `None` otherwise.
  pub fn checked_add(&self, duration: Duration) -> Option<Instant> {
    let since_epoch = self.since_epoch.checked_add(duration)?;
    Some(Instant { since_epoch })
  }

  /// Returns `Some(t)` where `t` is the time `self - duration` if `t` can be
  /// represented as `Instant`, `None` otherwise.
  pub fn checked_sub(&self, duration: Duration) -> Option<Instant> {
    let since_epoch = self.since_epoch.checked_sub(duration)?;
    Some(Instant { since_epoch })
  }
}

impl Add<Duration> for Instant {
  type Output = Instant;
  fn add(self, other: Duration) -> Instant {
    match self.checked_add(other) {
      Some(instant) => instant,
      None => panic!("overflow when adding duration to instant"),
    }
  }
}

impl AddAssign<Duration> for Instant {
  fn add_assign(&mut self, other: Duration) {
    *self = *self + other;
  }
}

impl Sub<Duration> for Instant {
  type Output = Instant;
  fn sub(self, other: Duration) -> Instant {
    match self.checked_sub(other) {
      Some(instant) => instant,
      None => panic!("overflow when subtracting duration from instant"),
    }
  }
}

impl SubAssign<Duration> for Instant {
  fn sub_assign(&mut self, other: Duration) {
    *self = *self - other;
  }
}

impl Sub<Instant> for Instant {
  type Output = Duration;
  /// Returns the amount of time elapsed from another instant to this one, or
  /// zero duration if that instant is later than this one.
  fn sub(self, rhs: Instant) -> Duration {
    self.saturating_duration_since(rhs)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use wasm_bindgen_test::wasm_bindgen_test;

  #[wasm_bindgen_test]
  fn now_is_monotonic() {
    let first = Instant::now();
    let second = Instant::now();
    assert!(second >= first);
  }

  #[wasm_bindgen_test]
  fn a_backwards_clock_reading_is_clamped() {
    let latest = Instant::now().since_epoch;
    // A reading older than the last one, as a jumping wall clock
    // produces, must not travel back in time.
    let earlier = latest.saturating_sub(Duration::from_secs(1));
    assert!(monotonic(earlier) >= latest);
  }

  #[wasm_bindgen_test]
  fn arithmetic_round_trips() {
    let base = Instant::now();
    let later = base + Duration::from_secs(5);
    assert_eq!(later - base, Duration::from_secs(5));
    assert_eq!(later - Duration::from_secs(5), base);
  }

  #[wasm_bindgen_test]
  fn duration_since_saturates_to_zero() {
    let base = Instant::now();
    let later = base + Duration::from_secs(5);
    assert_eq!(base.duration_since(later), Duration::ZERO);
    assert_eq!(base.saturating_duration_since(later), Duration::ZERO);
    assert_eq!(base.checked_duration_since(later), None);
    assert_eq!(
      later.checked_duration_since(base),
      Some(Duration::from_secs(5))
    );
  }

  #[wasm_bindgen_test]
  fn checked_arithmetic_reports_overflow() {
    let base = Instant::now();
    assert_eq!(base.checked_add(Duration::MAX), None);
    assert_eq!(
      base.checked_sub(Duration::from_secs(86400 * 365 * 100)),
      None
    );
    assert!(base.checked_add(Duration::from_secs(1)).is_some());
  }

  #[wasm_bindgen_test]
  fn elapsed_is_zero_for_future_instants() {
    let future = Instant::now() + Duration::from_secs(60);
    assert_eq!(future.elapsed(), Duration::ZERO);
  }

  #[wasm_bindgen_test]
  fn far_future_is_far() {
    let far = Instant::far_future();
    assert!(far - Instant::now() > Duration::from_secs(86400 * 365));
  }
}
