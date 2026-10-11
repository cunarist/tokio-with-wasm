use crate::support::assert_elapsed;
use std::cell::RefCell;
use std::future::Future;
use std::pin::pin;
use std::rc::Rc;
use std::task::{Context, Waker};
use std::time::Duration;
use tokio::time;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn delayed_sleep_level_0() {
  for i in [1, 10, 60] {
    let now = js_sys::Date::now();
    time::sleep(Duration::from_millis(i)).await;
    assert_elapsed(now, i);
  }
}

#[wasm_bindgen_test]
async fn short_sleeps() {
  for _ in 0..1000 {
    time::sleep(Duration::from_millis(0)).await;
  }
}

#[wasm_bindgen_test]
async fn sleeps_complete_in_deadline_order() {
  let order = Rc::new(RefCell::new(Vec::new()));
  let handles: Vec<_> = [300, 100, 200]
    .map(|ms| {
      let order = order.clone();
      tokio::spawn(async move {
        time::sleep(Duration::from_millis(ms)).await;
        order.borrow_mut().push(ms);
      })
    })
    .into();
  for handle in handles {
    handle.await.unwrap();
  }
  assert_eq!(*order.borrow(), [100, 200, 300]);
}

#[wasm_bindgen_test]
async fn sub_ms_delayed_sleep() {
  for _ in 0..5 {
    let now = js_sys::Date::now();
    time::sleep(Duration::from_millis(1) + Duration::from_nanos(1)).await;
    assert_elapsed(now, 1);
  }
}

#[wasm_bindgen_test]
async fn delayed_sleep_wrapping_level_0() {
  time::sleep(Duration::from_millis(5)).await;
  let now = js_sys::Date::now();
  time::sleep(Duration::from_millis(60)).await;
  assert_elapsed(now, 60);
}

#[wasm_bindgen_test]
async fn sleeps_run_concurrently() {
  let now = js_sys::Date::now();
  let handles: Vec<_> = (0..10)
    .map(|_| tokio::spawn(time::sleep(Duration::from_millis(100))))
    .collect();
  for handle in handles {
    handle.await.unwrap();
  }
  assert_elapsed(now, 100);
  assert!(js_sys::Date::now() - now < 1000.0);
}

#[wasm_bindgen_test]
async fn long_sleep_does_not_fire_early() {
  for duration in [Duration::from_secs(30 * 24 * 3600), Duration::MAX] {
    let result =
      time::timeout(Duration::from_millis(50), time::sleep(duration));
    assert!(result.await.is_err());
  }
}

#[wasm_bindgen_test]
fn dropped_sleeps_free_their_memory() {
  let memory_size = || {
    let memory =
      wasm_bindgen::memory().unchecked_into::<js_sys::WebAssembly::Memory>();
    memory
      .buffer()
      .unchecked_into::<js_sys::SharedArrayBuffer>()
      .byte_length()
  };
  let mut sizes = Vec::new();
  for _ in 0..2 {
    for _ in 0..100_000 {
      let mut sleep = pin!(time::sleep(Duration::from_secs(3600)));
      let mut cx = Context::from_waker(Waker::noop());
      assert!(sleep.as_mut().poll(&mut cx).is_pending());
    }
    sizes.push(memory_size());
  }
  assert!(sizes[1] - sizes[0] < 1 << 20, "{sizes:?}");
}
