//! A monotonic clock backed by JavaScript's `performance.now()`.

use std::ops::{Add, AddAssign, Sub, SubAssign};
use std::time::Duration;
use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
extern "C" {
  #[wasm_bindgen(js_namespace = performance, js_name = now)]
  fn performance_now() -> f64;
  // Each worker has its own origin, so adding it makes instants comparable.
  #[wasm_bindgen(thread_local_v2, js_namespace = performance, js_name = timeOrigin)]
  static TIME_ORIGIN: f64;
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
    let millis = TIME_ORIGIN.with(|origin| origin + performance_now());
    Instant {
      since_epoch: Duration::from_secs_f64(millis / 1000.0),
    }
  }

  /// A deadline for waits without a real deadline, far enough away
  /// that it never fires within a page's lifetime.
  pub(crate) fn far_future() -> Instant {
    // Roughly 30 years from now, like in `tokio`.
    Instant::now() + Duration::from_secs(86400 * 365 * 30)
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
    Instant {
      since_epoch: self.since_epoch + other,
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
    Instant {
      since_epoch: self.since_epoch - other,
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
