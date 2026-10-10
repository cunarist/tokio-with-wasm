use crate::support::{assert_pending, assert_ready, spawn};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;
use tokio::task;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn yield_now_outside_of_runtime() {
  let mut task = spawn(task::yield_now());

  assert_pending!(task.poll());
  tokio::time::sleep(Duration::from_millis(10)).await;
  assert!(task.is_woken());
  assert_ready!(task.poll());
}

#[wasm_bindgen_test]
async fn yield_now_runs_other_tasks() {
  let ran = Rc::new(Cell::new(false));
  let ran2 = ran.clone();
  tokio::spawn(async move { ran2.set(true) });
  task::yield_now().await;
  assert!(ran.get());
}
