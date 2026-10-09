//! Translation of JavaScript exceptions into `std::io::Error`.

use js_sys::{Promise, Reflect};
use std::io;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

fn property(value: &JsValue, key: &str) -> Option<String> {
  Reflect::get(value, &JsValue::from_str(key))
    .ok()
    .and_then(|found| found.as_string())
    .filter(|found| !found.is_empty())
}

/// Maps a `DOMException` to the error the matching `tokio` call returns.
pub fn to_io_error(value: JsValue) -> io::Error {
  let name = property(&value, "name").unwrap_or_default();
  let kind = match name.as_str() {
    "NotFoundError" => io::ErrorKind::NotFound,
    "NotAllowedError" | "SecurityError" => io::ErrorKind::PermissionDenied,
    // Only directory lookups raise it this way round; `file_in` flips it.
    "TypeMismatchError" => io::ErrorKind::NotADirectory,
    "InvalidModificationError" => io::ErrorKind::DirectoryNotEmpty,
    "NoModificationAllowedError" => io::ErrorKind::ResourceBusy,
    "QuotaExceededError" => io::ErrorKind::QuotaExceeded,
    _ => io::ErrorKind::Other,
  };
  let message =
    property(&value, "message").unwrap_or_else(|| format!("{value:?}"));
  let message = if name.is_empty() {
    message
  } else {
    format!("{name}: {message}")
  };
  io::Error::new(kind, message)
}

pub async fn await_js(promise: Promise) -> io::Result<JsValue> {
  JsFuture::from(promise).await.map_err(to_io_error)
}

/// Awaits a promise from a method that can also throw.
pub async fn await_call(
  promise: Result<Promise, JsValue>,
) -> io::Result<JsValue> {
  await_js(promise.map_err(to_io_error)?).await
}
