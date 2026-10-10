use crate::assert_elapsed;
use std::time::Duration;
use tokio::time;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn reset_doesnt_panic_max_duration() {
  let mut interval = time::interval(Duration::MAX);
  interval.reset();
}

#[wasm_bindgen_test]
async fn reset() {
  let mut i = time::interval(Duration::from_millis(50));
  i.tick().await;
  time::sleep(Duration::from_millis(30)).await;
  i.reset();
  let now = js_sys::Date::now();
  i.tick().await;
  assert_elapsed(now, 50);
}

#[wasm_bindgen_test]
async fn ticks_every_period() {
  let mut i = time::interval(Duration::from_millis(10));
  let now = js_sys::Date::now();
  for _ in 0..3 {
    i.tick().await;
  }
  assert_elapsed(now, 20);
}
