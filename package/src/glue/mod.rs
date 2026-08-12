//! JavaScript glue module that mimics `tokio`.

mod common;

#[cfg(feature = "fs")]
pub mod fs;

// `tokio::io` reads and writes through the `AsyncRead` and `AsyncWrite`
// traits rather than through the operating system, so the real
// implementation already compiles for the web without any glue.
#[cfg(feature = "io-util")]
pub use tokio::io;

#[cfg(feature = "macros")]
pub use tokio::{join, pin, select, try_join};

#[cfg(feature = "sync")]
pub use tokio::sync;

#[cfg(feature = "time")]
pub mod time;

#[cfg(feature = "rt")]
pub mod task;
#[cfg(feature = "rt")]
pub use task::spawn;
// The macro's expansion machinery lives in real `tokio` and does not
// depend on the `tokio` runtime, so it works on the web as is.
#[cfg(feature = "rt")]
pub(crate) use task::*;
#[cfg(feature = "rt")]
pub use tokio::task_local;

#[cfg(all(
  any(feature = "rt", feature = "rt-multi-thread"),
  feature = "macros"
))]
pub use tokio_with_wasm_proc::{main, test};

#[doc(hidden)]
#[cfg(all(
  any(feature = "rt", feature = "rt-multi-thread"),
  feature = "macros"
))]
// This export is needed for the `main` macro.
pub use wasm_bindgen_futures::spawn_local;

/// Turns the return value of a `#[tokio::main]` or `#[tokio::test]`
/// function into an outcome, panicking on `Err` the way a native binary
/// exits with an error. This export is needed for those macros.
#[doc(hidden)]
#[cfg(all(
  any(feature = "rt", feature = "rt-multi-thread"),
  feature = "macros"
))]
pub trait MacroOutcome {
  fn handle(self);
}

#[cfg(all(
  any(feature = "rt", feature = "rt-multi-thread"),
  feature = "macros"
))]
impl MacroOutcome for () {
  fn handle(self) {}
}

#[cfg(all(
  any(feature = "rt", feature = "rt-multi-thread"),
  feature = "macros"
))]
impl<T, E: std::fmt::Debug> MacroOutcome for Result<T, E> {
  fn handle(self) {
    if let Err(error) = self {
      panic!("the function returned an error: {error:?}");
    }
  }
}

#[allow(unused_imports)]
pub(crate) use common::*;

// Module only available when compiling to WebAssembly.
#[cfg(all(
  target_family = "wasm",
  target_vendor = "unknown",
  target_os = "unknown"
))]
pub mod only_web;
