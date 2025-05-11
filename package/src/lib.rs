//! A library that adapts the popular async runtime `tokio` for web browsers.
//!
//! It provides a similar set of features specifically for web applications
//! by leveraging the JavaScript web API.
//!
//! This library includes JavaScript glue code
//! to mimic the behavior of real `tokio`,
//! making it possible to run asynchronous Rust code in the browser.
//! Since `tokio_with_wasm` adapts to the JavaScript event loop
//! and does not include its own runtime,
//! some advanced features of `tokio` might not be fully supported.

#[cfg(not(all(
  target_arch = "wasm32",
  target_vendor = "unknown",
  target_os = "unknown"
)))]
pub use tokio as alias;

#[cfg(all(
  target_arch = "wasm32",
  target_vendor = "unknown",
  target_os = "unknown"
))]
pub use crate as alias;

#[cfg(all(
  target_arch = "wasm32",
  target_vendor = "unknown",
  target_os = "unknown"
))]
mod glue;

#[allow(unused_imports)]
#[cfg(all(
  target_arch = "wasm32",
  target_vendor = "unknown",
  target_os = "unknown"
))]
pub use glue::*;
