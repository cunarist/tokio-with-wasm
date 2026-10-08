//! Browser tests for `fs` on an engine that cannot write to files,
//! such as Safari before version 26.
//! Run with `wasm-pack test --headless --chrome package`.
//!
//! This is a file of its own, because it takes the writing API away
//! from the whole page.

// The glue code only exists on the web target,
// so this file is empty everywhere else.
#![cfg(all(
  target_family = "wasm",
  target_vendor = "unknown",
  target_os = "unknown"
))]

use js_sys::Function;
use std::io::ErrorKind;
use tokio_with_wasm::fs;
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn writing_without_create_writable_is_unsupported() {
  let remove = "delete FileSystemFileHandle.prototype.createWritable";
  let _ = Function::new_no_args(remove).call0(&js_sys::global());
  let written = fs::write("unsupported.txt", b"lost").await;
  assert!(written.is_err_and(|error| error.kind() == ErrorKind::Unsupported));
}
