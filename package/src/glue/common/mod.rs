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
