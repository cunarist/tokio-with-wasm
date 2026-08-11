//! The bootstrap script that every blocking web worker runs.
//!
//! By default the script is built in memory and handed to the worker as a
//! `blob:` URL. Applications that run under a content security policy which
//! forbids `blob:` workers can ship the script as a file of their own and
//! point the pool at it with [`set_worker_script_provider`].

use crate::BLOCKING_KEY;
use crate::only_web::PATH_PROVIDER;
use js_sys::Array;
use std::cell::RefCell;
use wasm_bindgen::JsValue;
use web_sys::{Blob, BlobPropertyBag, Url};

thread_local! {
    pub(crate) static WORKER_SCRIPT_PROVIDER: RefCell<fn() -> Result<String, JsValue>> = RefCell::new(get_worker_script);
    /// The glue path a bootstrap script was built from,
    /// together with the object URL that was built from it.
    static BUILT_SCRIPT: RefCell<Option<(String, String)>> = const { RefCell::new(None) };
}

/// The worker script provider function is used to determine the URL of the
/// script that each blocking web worker runs.
///
/// The default provider builds that script in memory and passes it as a
/// `blob:` URL, which a content security policy such as a browser
/// extension's `script-src 'self'` rejects. Point this at a script of your
/// own to stay within such a policy. [`worker_bootstrap_script`] returns the
/// source that the script has to contain.
///
/// Set it before the first call to `spawn_blocking`, because the URL is read
/// when the first web worker is created. It applies to the thread that calls
/// this function.
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

/// Builds the bootstrap script from the glue path and returns it as a
/// `blob:` URL. This is the default worker script provider.
///
/// Every worker on this thread runs the same script, so the URL is built
/// once and reused. Creating one URL per worker would leak an object URL
/// for the whole lifetime of the page, and revoking it right away would
/// race with the worker's script fetch.
///
/// # Errors
///
/// Returns the error of the path provider, or any error that may happen
/// while the `blob:` URL is created.
pub fn get_worker_script() -> Result<String, JsValue> {
  let glue_path = PATH_PROVIDER.with(|provider| provider.borrow()())?;
  BUILT_SCRIPT.with(|built| {
    let mut built = built.borrow_mut();
    if let Some((built_path, built_url)) = built.as_ref() {
      if *built_path == glue_path {
        return Ok(built_url.clone());
      }
    }
    let url = create_object_url(&worker_bootstrap_script(&glue_path))?;
    *built = Some((glue_path, url.clone()));
    Ok(url)
  })
}

/// Returns the JavaScript source that a blocking web worker has to run.
///
/// `glue_path` is where the worker imports the `wasm-bindgen` glue code
/// from, resolved against the location of the worker script itself. Write
/// the returned source to a file next to your glue code to serve it under a
/// content security policy that forbids `blob:` workers, and hand its URL to
/// [`set_worker_script_provider`].
///
/// # Example
/// ```rust,no_run
/// use tokio_with_wasm::only_web::worker_bootstrap_script;
///
/// // The contents of a `tokio_worker.js` sitting next to `my_app.js`.
/// let source = worker_bootstrap_script("./my_app.js");
/// ```
pub fn worker_bootstrap_script(glue_path: &str) -> String {
  format!(
    "
    import init, * as wasmBindings from '{glue_path}';
    globalThis.wasmBindings = wasmBindings;
    globalThis.{BLOCKING_KEY} = true;
    self.onmessage = event => {{
      let initialised = init(event.data).catch(err => {{
        // Propagate to main `onerror`:
        setTimeout(() => {{
          throw err;
        }});
        // Rethrow to keep promise rejected
        // and prevent execution of further commands:
        throw err;
      }});

      self.onmessage = async event => {{
        // This will queue further commands up
        // until the module is fully initialised:
        await initialised;
        try {{
          wasmBindings.task_worker_entry_point(event.data);
        }} catch (err) {{
          // A panicking task traps here. Throwing inside an async
          // handler would only reject its promise, which the parent
          // thread never sees, so the error is rethrown from a timeout
          // to reach the `Worker`'s `onerror`:
          setTimeout(() => {{
            throw err;
          }});
          throw err;
        }}
      }};
    }};
    "
  )
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

  /// A hand-written worker script has to import the glue code and mark
  /// itself as a blocking thread, or `spawn_blocking` never returns.
  #[wasm_bindgen_test]
  fn the_bootstrap_script_carries_the_worker_contract() {
    let script = worker_bootstrap_script("./my_app.js");
    assert!(script.contains("from './my_app.js'"));
    assert!(script.contains(&format!("globalThis.{BLOCKING_KEY} = true")));
    assert!(script.contains("wasmBindings.task_worker_entry_point"));
  }
}
