//! Translation of JavaScript exceptions into `std::io::Error`.

use js_sys::{Promise, Reflect};
use std::io;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_futures::JsFuture;

/// What was being asked for when a call failed.
///
/// The file system API reports "you asked for the wrong kind" with one
/// exception name for both directions, while the standard library has a
/// separate error for each, so the caller has to say which it wanted.
#[derive(Clone, Copy)]
pub enum Wanted {
  File,
  Directory,
  /// Neither, for calls that do not name a kind.
  Either,
}

/// Reads a string property off a JavaScript exception.
fn property(value: &JsValue, key: &str) -> Option<String> {
  Reflect::get(value, &JsValue::from_str(key))
    .ok()
    .and_then(|found| found.as_string())
    .filter(|found| !found.is_empty())
}

/// Turns a rejected promise into the error
/// that the matching `tokio` call would have returned.
///
/// Everything the file system API rejects with is a `DOMException`,
/// and its `name` says what went wrong.
pub fn to_io_error(value: JsValue, wanted: Wanted) -> io::Error {
  let name = property(&value, "name").unwrap_or_default();
  let kind = match name.as_str() {
    "NotFoundError" => io::ErrorKind::NotFound,
    "NotAllowedError" | "SecurityError" => io::ErrorKind::PermissionDenied,
    // The entry is there but is the other kind.
    "TypeMismatchError" => match wanted {
      Wanted::File => io::ErrorKind::IsADirectory,
      Wanted::Directory => io::ErrorKind::NotADirectory,
      Wanted::Either => io::ErrorKind::InvalidInput,
    },
    // Removing a directory that still holds something.
    "InvalidModificationError" => io::ErrorKind::DirectoryNotEmpty,
    // Another handle is holding the file open.
    "NoModificationAllowedError" => io::ErrorKind::ResourceBusy,
    "QuotaExceededError" => io::ErrorKind::QuotaExceeded,
    _ => io::ErrorKind::Other,
  };
  let message =
    property(&value, "message").unwrap_or_else(|| format!("{value:?}"));
  if name.is_empty() {
    io::Error::new(kind, message)
  } else {
    io::Error::new(kind, format!("{name}: {message}"))
  }
}

/// Awaits a promise, reporting a rejection as an `io::Error`.
pub async fn await_js(promise: Promise, wanted: Wanted) -> io::Result<JsValue> {
  JsFuture::from(promise)
    .await
    .map_err(|value| to_io_error(value, wanted))
}

/// Awaits a promise that a method returned through a `Result`.
pub async fn await_call(
  promise: Result<Promise, JsValue>,
  wanted: Wanted,
) -> io::Result<JsValue> {
  let promise = promise.map_err(|value| to_io_error(value, wanted))?;
  await_js(promise, wanted).await
}

/// Casts a value that the file system API just handed over.
///
/// Every such cast holds by specification,
/// so a failure means the browser is not following it.
pub fn cast<T: JsCast>(value: JsValue) -> io::Result<T> {
  value.dyn_into::<T>().map_err(|value| {
    io::Error::other(format!(
      "the file system API returned an unexpected value: {value:?}"
    ))
  })
}
