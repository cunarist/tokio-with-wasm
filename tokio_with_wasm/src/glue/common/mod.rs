#![allow(dead_code)]

mod internal_channel;
mod once_channel;
mod select_future;

pub use internal_channel::*;
pub use once_channel::*;
pub use select_future::*;

use js_sys::Function;
use wasm_bindgen::prelude::{wasm_bindgen, JsError};
use wasm_bindgen::JsValue;

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

impl LogError for JsError {
    fn log_error(&self, code: &str) {
        error(&format!(
            "Error `{}` in `tokio_with_wasm`:\n{:?}",
            code, self
        ));
    }
}

impl<T> LogError for Result<T, JsValue> {
    fn log_error(&self, code: &str) {
        if let Err(js_error) = self {
            error(&format!(
                "Error `{}` in `tokio_with_wasm`:\n{:?}",
                code, js_error
            ));
        }
    }
}
