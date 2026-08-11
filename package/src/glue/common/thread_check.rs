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

#[cfg(test)]
mod tests {
  use super::*;
  use wasm_bindgen_test::wasm_bindgen_test;

  // The worker side of this check is covered by the integration test
  // that calls `spawn` inside `spawn_blocking`.
  #[wasm_bindgen_test]
  fn the_test_harness_counts_as_the_main_thread() {
    assert!(is_main_thread());
  }
}
