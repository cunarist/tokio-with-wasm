//! Browser test for `fs` on an engine that cannot write to files, such as
//! Safari before version 26, in a page of its own: it takes the writing
//! API away from the whole page.

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
