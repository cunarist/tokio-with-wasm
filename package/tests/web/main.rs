//! Browser tests, in one binary to start the browser once.
//! Run with `wasm-pack test --headless --chrome package`, with
//! `--cfg tokio_unstable` in the flags to include `task::Builder`.

// The glue code only exists on the web target,
// so this crate is empty everywhere else.
#![cfg(all(
  target_family = "wasm",
  target_vendor = "unknown",
  target_os = "unknown"
))]
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod blocking;
mod fs;
mod join_map;
mod join_set;
mod macros;
mod task;
mod time;

use js_sys::Function;
use std::future::Future;
use std::task::{Context, Waker};
use wasm_bindgen::{JsCast, JsValue};

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

/// Polls a future once and drops it unfinished.
fn cancel(future: impl Future) {
  let mut context = Context::from_waker(Waker::noop());
  assert!(Box::pin(future).as_mut().poll(&mut context).is_pending());
}

/// Runs `run` under `js`, which patches a global and returns
/// a function that undoes the patch and reports what it saw.
async fn spy(js: &str, run: impl Future) -> JsValue {
  let undo = Function::new_no_args(js).call0(&JsValue::NULL).unwrap();
  run.await;
  undo
    .unchecked_into::<Function>()
    .call0(&JsValue::NULL)
    .unwrap()
}
