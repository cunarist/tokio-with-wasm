#![allow(dead_code)]
#![allow(unused_imports)]

mod local_channel;
mod once_channel;
mod polling;
mod select_future;
mod thread_check;

pub use local_channel::*;
pub use once_channel::*;
pub use polling::*;
pub use select_future::*;
pub use thread_check::*;

use js_sys::Function;
use wasm_bindgen::prelude::{JsValue, wasm_bindgen};

#[wasm_bindgen]
extern "C" {
  #[wasm_bindgen(js_namespace = console, js_name = error)]
  pub fn error(s: &str);
  #[wasm_bindgen(js_namespace = Date, js_name = now)]
  pub fn now() -> f64;
  #[wasm_bindgen(js_namespace = globalThis, js_name = setTimeout)]
  pub fn set_timeout(callback: &Function, milliseconds: f64);
  #[wasm_bindgen(js_namespace = globalThis, js_name = setInterval)]
  pub fn set_interval(callback: &Function, milliseconds: f64) -> i32;
  #[wasm_bindgen(js_namespace = globalThis, js_name = clearInterval)]
  pub fn clear_interval(id: i32);
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
      error(&format!(
        "Error `{code}` in `tokio_with_wasm`:\n{js_value:?}"
      ));
    }
  }
}

#[cfg(test)]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[cfg(test)]
mod tests {
  use std::sync::Arc;
  use std::sync::atomic::{AtomicUsize, Ordering};
  use std::task::{Wake, Waker};

  #[derive(Default)]
  pub struct WakeCount(AtomicUsize);

  impl WakeCount {
    pub fn get(&self) -> usize {
      self.0.load(Ordering::SeqCst)
    }
  }

  impl Wake for WakeCount {
    fn wake(self: Arc<Self>) {
      self.0.fetch_add(1, Ordering::SeqCst);
    }
  }

  pub fn counting_waker() -> (Waker, Arc<WakeCount>) {
    let count = Arc::new(WakeCount::default());
    (count.clone().into(), count)
  }
}
