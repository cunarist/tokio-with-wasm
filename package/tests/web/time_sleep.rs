use crate::assert_elapsed;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;
use tokio::time;
use tokio_with_wasm::alias as tokio;
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
async fn issue_5183() {
  let big = Duration::from_secs(u64::MAX / 10);
  tokio::select! {
    biased;
    _ = time::sleep(big) => {}
    _ = time::sleep(Duration::from_nanos(1)) => {}
  }
}

#[wasm_bindgen_test]
async fn sleeps_complete_in_deadline_order() {
  let order = Rc::new(RefCell::new(Vec::new()));
  let handles: Vec<_> = [30, 10, 20]
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
  assert_eq!(*order.borrow(), [10, 20, 30]);
}
