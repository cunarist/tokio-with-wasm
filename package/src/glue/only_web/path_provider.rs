//! Path provider for WebAssembly environment.
//! This module allows setting a custom path provider function
//! that determines the path to the worker script used in WebAssembly
//! multi-threading.

use std::cell::RefCell;

use js_sys::{Error, Reflect};
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

/// Determines the path to the currently executing script by reading the
/// stack trace of a freshly created JavaScript `Error`.
///
/// `wasm-bindgen` puts the code that constructs the `Error` in the glue
/// code, so the innermost frame of the stack names that file. Nothing is
/// evaluated as a string along the way, which keeps this working under a
/// content security policy that doesn't allow `unsafe-eval`.
///
/// It still depends on the shape of the stack trace, which browsers are
/// free to change. Pass the path in with [`set_path_provider`] if that
/// turns out to be a problem for your application.
pub fn get_script_path() -> Result<String, JsValue> {
  let error = Error::new("");
  let stack = Reflect::get(&error, &JsValue::from_str("stack"))
    .map_err(|error| {
      detection_failure(&format!("the stack could not be read: {error:?}"))
    })?
    .as_string()
    .ok_or_else(|| detection_failure("the error carried no stack"))?;
  stack
    .lines()
    .find_map(script_url_in_frame)
    .map(String::from)
    .ok_or_else(|| detection_failure("no script path was found in the stack"))
}

/// Reads the script URL out of a single stack trace frame.
///
/// V8 writes `    at name (https://host/glue.js:1:2)` or, for a frame with
/// no named function, `    at https://host/glue.js:1:2`. SpiderMonkey and
/// JavaScriptCore write `name@https://host/glue.js:1:2`. All of them end
/// with the line and column, which is what tells a frame apart from the
/// `Error` header line. A wasm frame ends with a hexadecimal offset
/// instead, so it is skipped and the enclosing JavaScript frame wins.
fn script_url_in_frame(frame: &str) -> Option<&str> {
  let frame = frame.trim().trim_end_matches(')');
  let (frame, column) = frame.rsplit_once(':')?;
  let (frame, line) = frame.rsplit_once(':')?;
  let is_position = !column.is_empty()
    && !line.is_empty()
    && column
      .bytes()
      .chain(line.bytes())
      .all(|byte| byte.is_ascii_digit());
  if !is_position {
    return None;
  }
  let url = match frame.rsplit_once('(') {
    Some((_, url)) => url,
    None => match frame.rsplit_once('@') {
      Some((_, url)) => url,
      None => frame.strip_prefix("at ").unwrap_or(frame),
    },
  };
  let url = url.trim();
  if url.is_empty() { None } else { Some(url) }
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

#[cfg(test)]
mod tests {
  use super::script_url_in_frame;
  use wasm_bindgen_test::wasm_bindgen_test;

  #[wasm_bindgen_test]
  fn frames_of_every_engine_are_understood() {
    let v8 = "    at __wbg_new (https://host/glue.js:729:67)";
    assert_eq!(script_url_in_frame(v8), Some("https://host/glue.js"));

    let v8_anonymous = "    at https://host/glue.js:729:67";
    assert_eq!(
      script_url_in_frame(v8_anonymous),
      Some("https://host/glue.js")
    );

    let spider_monkey = "__wbg_new@https://host/glue.js:729:67";
    assert_eq!(
      script_url_in_frame(spider_monkey),
      Some("https://host/glue.js")
    );
  }

  /// The header line and the wasm frames carry no script path,
  /// so the search has to walk past them.
  #[wasm_bindgen_test]
  fn frames_without_a_script_path_are_skipped() {
    assert_eq!(script_url_in_frame("Error"), None);
    assert_eq!(script_url_in_frame("Error: "), None);
    let wasm = "    at app.wasm.tokio_with_wasm::glue::task::pool::run::h1 \
      (https://host/app_bg.wasm:wasm-function[853]:0x5d1fb9)";
    assert_eq!(script_url_in_frame(wasm), None);
  }
}
