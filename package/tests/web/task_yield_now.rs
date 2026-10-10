use std::cell::Cell;
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::task::{Context, Wake, Waker};
use std::time::Duration;
use tokio::task;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

struct Flag(AtomicBool);

impl Wake for Flag {
  fn wake(self: Arc<Self>) {
    self.0.store(true, Ordering::SeqCst);
  }
}

#[wasm_bindgen_test]
async fn yield_now_outside_of_runtime() {
  let flag = Arc::new(Flag(AtomicBool::new(false)));
  let waker = Waker::from(flag.clone());
  let mut cx = Context::from_waker(&waker);
  let mut yielded = pin!(task::yield_now());

  assert!(yielded.as_mut().poll(&mut cx).is_pending());
  tokio::time::sleep(Duration::from_millis(10)).await;
  assert!(flag.0.load(Ordering::SeqCst));
  assert!(yielded.as_mut().poll(&mut cx).is_ready());
}

#[wasm_bindgen_test]
async fn yield_now_runs_other_tasks() {
  let ran = Rc::new(Cell::new(false));
  let ran2 = ran.clone();
  tokio::spawn(async move { ran2.set(true) });
  task::yield_now().await;
  assert!(ran.get());
}
