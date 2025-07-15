use js_sys::global;
use std::cell::LazyCell;
use wasm_bindgen::JsValue;

/// The name of a JS object
/// that is only present in the blocking thread.
pub static BLOCKING_KEY: &str = "isBlockingTokioThread";

thread_local! {
  pub static IS_MAIN_THREAD: LazyCell<bool> = LazyCell::new(|| {
    let global_obj = global();
    let is_blocking_thread =
      js_sys::Reflect::has(&global_obj, &JsValue::from_str(BLOCKING_KEY))
        .unwrap_or(false);
    !is_blocking_thread
  });
}

pub fn is_main_thread() -> bool {
  let mut is_main: bool = false;
  IS_MAIN_THREAD.with(|cell| {
    is_main = **cell;
  });
  is_main
}
