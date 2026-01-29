//! Path provider for WebAssembly environment.
//! This module allows setting a custom path provider function
//! that determines the path to the worker script used in WebAssembly
//! multi-threading.

use std::cell::RefCell;

use js_sys::eval;
use wasm_bindgen::JsValue;

thread_local! {
    pub(crate) static PATH_PROVIDER: RefCell<fn() -> Result<String, JsValue>> = RefCell::new(get_script_path);
}

/// The path provider function is used to determine the path to the
/// JavaScript glue code that bootstraps the wasm module in each worker.
/// By default the path provider uses a stack trace to determine the path
/// to the current script.
///
/// # Example
/// ```rust,no_run
/// use tokio_with_wasm::only_web::set_path_provider;
/// use wasm_bindgen::JsValue;
///
/// set_path_provider(|| {
///     Ok(String::from("/custom/path/to/worker.js"))
/// });
/// ```
#[inline(always)]
pub fn set_path_provider(provider: fn() -> Result<String, JsValue>) {
  PATH_PROVIDER.with(|p| {
    *p.borrow_mut() = provider;
  });
}

/// Determines the path to the currently executing script by throwing an
/// error and parsing the stack trace.
pub fn get_script_path() -> Result<String, JsValue> {
  let string = eval(
    r"
      (() => {
        try {
          throw new Error();
        } catch (e) {
          let parts = e.stack.match(/(?:\(|@)(\S+):\d+:\d+/);
          return parts[1];
        }
      })()
    ",
  )?
  .as_string()
  .ok_or(JsValue::from(
    "Could not convert JS string path to native string",
  ))?;
  Ok(string)
}
