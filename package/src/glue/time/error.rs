//! Time error types.

use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io;

/// Errors returned by [`Timeout`](crate::time::Timeout).
///
/// This error is returned when a timeout expires before the function was able
/// to finish.
#[derive(Debug, PartialEq, Eq)]
pub struct Elapsed(());

impl Elapsed {
  pub(crate) fn new() -> Self {
    Elapsed(())
  }
}

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
