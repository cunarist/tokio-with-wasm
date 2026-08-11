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
/// Set it before the first call to `spawn_blocking`, because the path is
/// read when the first web worker is created. It applies to the thread that
/// calls this function.
///
/// # Example
/// ```rust,no_run
/// use tokio_with_wasm::only_web::set_path_provider;
///
/// set_path_provider(|| Ok(String::from("/custom/path/to/glue.js")));
/// ```
#[inline(always)]
pub fn set_path_provider(provider: fn() -> Result<String, JsValue>) {
  PATH_PROVIDER.with(|p| {
    *p.borrow_mut() = provider;
  });
}

/// Determines the path to the currently executing script by throwing an
/// error and parsing the stack trace.
///
/// This needs `eval`, so it fails under a content security policy that
/// doesn't allow `unsafe-eval`. It also depends on the shape of the stack
/// trace, which browsers are free to change. Pass the path in with
/// [`set_path_provider`] if either applies to your application.
pub fn get_script_path() -> Result<String, JsValue> {
  let evaluated = eval(
    r"
      (() => {
        try {
          throw new Error();
        } catch (error) {
          const parts = (error.stack ?? '').match(/(?:\(|@)(\S+):\d+:\d+/);
          return parts ? parts[1] : null;
        }
      })()
    ",
  )
  .map_err(|error| {
    // A content security policy without `unsafe-eval` lands here.
    detection_failure(&format!("`eval` failed with {error:?}"))
  })?;
  evaluated
    .as_string()
    .ok_or_else(|| detection_failure("no script path was found in the stack"))
}

/// Explains that the path could not be detected, and how to move on.
/// Without this, callers would only see a `TypeError` from deep inside a
/// stack trace regular expression.
fn detection_failure(reason: &str) -> JsValue {
  JsValue::from_str(&format!(
    "Could not detect the path of the JavaScript glue code, \
     because {reason}. Provide the path with \
     `tokio_with_wasm::only_web::set_path_provider`."
  ))
}
