use js_sys::{Reflect, global};
use wasm_bindgen::JsValue;

/// The name of a JS object
/// that is only present in the blocking thread.
pub static BLOCKING_KEY: &str = "isBlockingTokioThread";

thread_local! {
  static IS_MAIN_THREAD: bool =
    !Reflect::has(&global(), &JsValue::from_str(BLOCKING_KEY)).unwrap_or(false);
}

pub fn is_main_thread() -> bool {
  IS_MAIN_THREAD.with(|is_main| *is_main)
}
