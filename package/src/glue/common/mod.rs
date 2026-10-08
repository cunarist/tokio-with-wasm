#![allow(dead_code, unused_imports)]

mod completion_queue;
mod once_channel;
#[cfg(test)]
pub(crate) mod test_util;
mod thread_check;

pub use completion_queue::*;
pub use once_channel::*;
pub use thread_check::*;

use js_sys::Function;
use std::sync::{Mutex, MutexGuard};
use wasm_bindgen::prelude::{JsValue, wasm_bindgen};

/// Locks a mutex, taking the state inside even if the lock is poisoned.
pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
  mutex.lock().unwrap_or_else(|error| error.into_inner())
}

#[wasm_bindgen]
extern "C" {
  #[wasm_bindgen(js_namespace = console, js_name = error)]
  pub fn error(s: &str);
  #[wasm_bindgen(js_namespace = Date, js_name = now)]
  pub fn now() -> f64;
  #[wasm_bindgen(js_namespace = globalThis, js_name = setTimeout)]
  pub fn set_timeout(callback: &Function, milliseconds: f64) -> JsValue;
}

pub trait LogError {
  fn log_error(&self, code: &str);
}

impl LogError for JsValue {
  fn log_error(&self, code: &str) {
    error(&format!("Error `{code}` in `tokio_with_wasm`:\n{self:?}"));
  }
}

impl<T> LogError for Result<T, JsValue> {
  fn log_error(&self, code: &str) {
    if let Err(js_value) = self {
      js_value.log_error(code);
    }
  }
}
