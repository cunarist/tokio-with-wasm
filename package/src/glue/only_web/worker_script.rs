//! The bootstrap script that every blocking web worker runs.
//!
//! By default the script is built in memory and handed to the worker as a
//! `blob:` URL. Applications that run under a content security policy which
//! forbids `blob:` workers can serve the same script as a file of their own
//! and point the pool at it with [`set_worker_script_provider`].

use js_sys::Array;
use std::cell::RefCell;
use wasm_bindgen::JsValue;
use web_sys::{Blob, BlobPropertyBag, Url};

thread_local! {
    pub(crate) static WORKER_SCRIPT_PROVIDER: RefCell<fn() -> Result<String, JsValue>> = RefCell::new(get_worker_script);
    /// The object URL that the bootstrap script was wrapped in.
    static BUILT_SCRIPT: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// The worker script provider function is used to determine the URL of the
/// script that each blocking web worker runs.
///
/// The default provider builds that script in memory and passes it as a
/// `blob:` URL, which a content security policy such as a browser
/// extension's `script-src 'self'` rejects. To stay within such a policy,
/// serve `tokio_worker.js` from this crate's repository as a file of your
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
/// set_worker_script_provider(|| Ok(String::from("/tokio_worker.js")));
/// ```
#[inline(always)]
pub fn set_worker_script_provider(provider: fn() -> Result<String, JsValue>) {
  WORKER_SCRIPT_PROVIDER.with(|p| {
    *p.borrow_mut() = provider;
  });
}

/// Returns the bootstrap script as a `blob:` URL. This is the default
/// worker script provider.
///
/// Every worker runs the same script, so the URL is built once and reused.
/// Creating one URL per worker would leak an object URL for the whole
/// lifetime of the page, and revoking it right away would race with the
/// worker's script fetch.
///
/// # Errors
///
/// Returns any error that may happen while the `blob:` URL is created.
pub fn get_worker_script() -> Result<String, JsValue> {
  BUILT_SCRIPT.with(|built| {
    let mut built = built.borrow_mut();
    if let Some(built_url) = built.as_ref() {
      return Ok(built_url.clone());
    }
    let url = create_object_url(worker_bootstrap_script())?;
    *built = Some(url.clone());
    Ok(url)
  })
}

/// Returns the JavaScript source that a blocking web worker runs.
///
/// The source doesn't depend on the application: the glue path and the wasm
/// module reach the worker in a message. Serve it as a file and hand its
/// URL to [`set_worker_script_provider`] under a content security policy
/// that forbids `blob:` workers.
pub fn worker_bootstrap_script() -> &'static str {
  include_str!("tokio_worker.js")
}

/// Wraps a script in an object URL that a web worker can be created from.
fn create_object_url(script: &str) -> Result<String, JsValue> {
  let blob_property_bag = BlobPropertyBag::new();
  blob_property_bag.set_type("text/javascript");
  let blob = Blob::new_with_blob_sequence_and_options(
    &Array::from_iter([JsValue::from(script)]).into(),
    &blob_property_bag,
  )?;
  Url::create_object_url_with_blob(&blob)
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
