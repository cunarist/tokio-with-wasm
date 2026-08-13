// Everything here is gated by the features that use it,
// so that partial builds stay free of dead code.

#[cfg(feature = "rt")]
mod completion_queue;
#[cfg(feature = "rt")]
mod once_channel;
#[cfg(test)]
pub(crate) mod test_util;
#[cfg(feature = "rt")]
mod thread_check;

#[cfg(feature = "rt")]
pub use completion_queue::*;
#[cfg(feature = "rt")]
pub use once_channel::*;
#[cfg(feature = "rt")]
pub use thread_check::*;

#[cfg(any(feature = "rt", feature = "time"))]
use js_sys::Function;
#[cfg(feature = "rt")]
use std::sync::{Mutex, MutexGuard};
#[cfg(any(feature = "rt", feature = "time"))]
use wasm_bindgen::prelude::JsValue;
#[cfg(any(feature = "fs", feature = "rt", feature = "time"))]
use wasm_bindgen::prelude::wasm_bindgen;

/// Locks a mutex, recovering the state inside
/// even if another thread panicked while holding the lock.
/// A poisoned lock would otherwise stall its consumers forever.
#[cfg(feature = "rt")]
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
  mutex.lock().unwrap_or_else(|error| error.into_inner())
}

#[cfg(any(feature = "fs", feature = "rt", feature = "time"))]
#[wasm_bindgen]
extern "C" {
  #[wasm_bindgen(js_namespace = console, js_name = error)]
  pub fn error(s: &str);
}

#[cfg(feature = "rt")]
#[wasm_bindgen]
extern "C" {
  #[wasm_bindgen(js_namespace = Date, js_name = now)]
  pub fn now() -> f64;
}

#[cfg(any(feature = "rt", feature = "time"))]
#[wasm_bindgen]
extern "C" {
  #[wasm_bindgen(js_namespace = globalThis, js_name = setTimeout)]
  pub fn set_timeout(callback: &Function, milliseconds: f64);
}

#[cfg(any(feature = "rt", feature = "time"))]
pub trait LogError {
  fn log_error(&self, code: &str);
}

#[cfg(any(feature = "rt", feature = "time"))]
impl LogError for JsValue {
  fn log_error(&self, code: &str) {
    error(&format!("Error `{code}` in `tokio_with_wasm`:\n{self:?}"));
  }
}

#[cfg(any(feature = "rt", feature = "time"))]
impl<T> LogError for Result<T, JsValue> {
  fn log_error(&self, code: &str) {
    if let Err(js_value) = self {
      js_value.log_error(code);
    }
  }
}
