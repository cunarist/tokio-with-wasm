//! Path provider for WebAssembly environment.
//! This module allows setting a custom path provider function
//! that determines the path to the JavaScript glue code that each
//! web worker loads in WebAssembly multi-threading.

use std::cell::RefCell;

use js_sys::JsString;
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;

thread_local! {
    pub(crate) static PATH_PROVIDER: RefCell<fn() -> Result<String, JsValue>> = RefCell::new(get_script_path);
}

#[wasm_bindgen]
extern "C" {
  /// This reference is compiled into the glue code itself,
  /// so the URL it reads is the glue code's own.
  #[wasm_bindgen(thread_local_v2, js_namespace = ["import", "meta"], js_name = url)]
  static IMPORT_META_URL: JsString;
}

/// The path provider function is used to determine the path to the
/// JavaScript glue code that bootstraps the wasm module in each worker.
/// By default the path provider reads the glue code's own
/// `import.meta.url`.
///
/// Set it before the first call to `spawn_blocking`. It applies to the
/// thread that calls this function.
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

/// Determines the path to the JavaScript glue code by reading its
/// `import.meta.url`.
///
/// The read is compiled into the glue code, so the URL is the glue code's
/// own. Nothing is evaluated as a string and no stack trace is parsed along
/// the way, so this works under a content security policy that doesn't
/// allow `unsafe-eval` and doesn't depend on the shape of stack traces.
///
/// `import.meta` only exists in ES modules, which is what `wasm-bindgen`
/// emits for the `web` target. Pass the path in with [`set_path_provider`]
/// if your glue code is bundled in a way that rewrites `import.meta.url`.
pub fn get_script_path() -> Result<String, JsValue> {
  IMPORT_META_URL
    .with(|url| url.as_string())
    .ok_or_else(|| detection_failure("`import.meta.url` carried no string"))
}

/// Explains that the path could not be detected, and how to move on.
fn detection_failure(reason: &str) -> JsValue {
  JsValue::from_str(&format!(
    "Could not detect the path of the JavaScript glue code, \
     because {reason}. Provide the path with \
     `tokio_with_wasm::only_web::set_path_provider`."
  ))
}
