use std::cell::Cell;
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;
use std::task::{Context, Waker};
use tokio::task;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn yield_now_outside_of_runtime() {
  let mut yielded = pin!(task::yield_now());
  let mut cx = Context::from_waker(Waker::noop());
  assert!(yielded.as_mut().poll(&mut cx).is_pending());
  yielded.await;
}

#[wasm_bindgen_test]
async fn yield_now_runs_other_tasks() {
  let ran = Rc::new(Cell::new(false));
  let ran2 = ran.clone();
  tokio::spawn(async move { ran2.set(true) });
  task::yield_now().await;
  assert!(ran.get());
}
