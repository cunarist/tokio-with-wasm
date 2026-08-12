//! JavaScript glue module that mimics `tokio`.

mod common;

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
#[cfg(feature = "rt")]
pub(crate) use task::*;

#[cfg(all(
  any(feature = "rt", feature = "rt-multi-thread"),
  feature = "macros"
))]
pub use tokio_with_wasm_proc::main;

#[doc(hidden)]
#[cfg(all(
  any(feature = "rt", feature = "rt-multi-thread"),
  feature = "macros"
))]
// This export is needed for the `main` macro.
pub use wasm_bindgen_futures::spawn_local;

#[allow(unused_imports)]
pub(crate) use common::*;

// Module only available when compiling to WebAssembly.
#[cfg(all(
  target_family = "wasm",
  target_vendor = "unknown",
  target_os = "unknown"
))]
pub mod only_web;
