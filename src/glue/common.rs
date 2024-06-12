#![allow(unused_macros, unused_imports, dead_code)]

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    pub fn log(s: &str);
    #[wasm_bindgen(js_namespace = Date, js_name = now)]
    pub fn now() -> f64;
    #[wasm_bindgen(js_name = setTimeout)]
    pub fn set_timeout(callback: &js_sys::Function, milliseconds: f64);
    #[wasm_bindgen(js_name = queueMicrotask)]
    pub fn queue_microtask(callback: &js_sys::Function);
}

macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}
pub(crate) use console_log;

pub type Result<T> = std::result::Result<T, JsValue>;
