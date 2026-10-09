//! The bootstrap script that every blocking web worker runs.

use js_sys::Array;
use std::cell::{Cell, OnceCell};
use wasm_bindgen::JsValue;
use web_sys::{Blob, BlobPropertyBag, Url};

thread_local! {
    pub(crate) static WORKER_SCRIPT_PROVIDER: Cell<fn() -> Result<String, JsValue>> = Cell::new(get_worker_script);
    static BUILT_SCRIPT: OnceCell<String> = const { OnceCell::new() };
}

/// The worker script provider function is used to determine the URL of the
/// script that each blocking web worker runs.
///
/// The default provider builds that script in memory and passes it as a
/// `blob:` URL, which a content security policy such as a browser
/// extension's `script-src 'self'` rejects. To stay within such a policy,
/// serve `blocking_worker.js` from this crate's repository as a file of your
/// own and point this at it. The script is the same for every application,
/// because the wasm module and its glue path arrive in a message.
///
/// Set it before the first call to `spawn_blocking`. It applies to the
/// thread that calls this function.
///
/// # Example
/// ```rust,no_run
/// use tokio_with_wasm::only_web::set_worker_script_provider;
///
/// set_worker_script_provider(|| Ok(String::from("/blocking_worker.js")));
/// ```
pub fn set_worker_script_provider(provider: fn() -> Result<String, JsValue>) {
  WORKER_SCRIPT_PROVIDER.set(provider);
}

/// Returns the bootstrap script as a `blob:` URL, built once so that
/// workers don't leak one object URL each. This is the default provider.
pub fn get_worker_script() -> Result<String, JsValue> {
  BUILT_SCRIPT.with(|built| {
    if let Some(url) = built.get() {
      return Ok(url.clone());
    }
    let options = BlobPropertyBag::new();
    options.set_type("text/javascript");
    let script = Array::of1(&worker_bootstrap_script().into());
    let blob = Blob::new_with_blob_sequence_and_options(&script, &options)?;
    let url = Url::create_object_url_with_blob(&blob)?;
    Ok(built.get_or_init(|| url).clone())
  })
}

/// Returns the JavaScript source that a blocking web worker runs.
/// See [`set_worker_script_provider`] for serving it as a file.
pub fn worker_bootstrap_script() -> &'static str {
  include_str!("blocking_worker.js")
}

#[cfg(test)]
mod tests {
  use super::worker_bootstrap_script;
  use crate::BLOCKING_KEY;
  use wasm_bindgen_test::wasm_bindgen_test;

  /// The script file is hand-copied by users, so it has to keep matching
  /// the crate: the blocking-thread key, the entry point, and the glue
  /// path arriving in the first message. If this test fails, the script
  /// and the crate have drifted apart.
  #[wasm_bindgen_test]
  fn the_bootstrap_script_carries_the_worker_contract() {
    let script = worker_bootstrap_script();
    assert!(script.contains("import(event.data.glue_path)"));
    assert!(script.contains(&format!("globalThis.{BLOCKING_KEY} = true")));
    assert!(script.contains("wasmBindings.task_worker_entry_point"));
  }
}
