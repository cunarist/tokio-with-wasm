use crate::assert_elapsed;
use std::thread;
use std::time::Duration;
use tokio::{task, time};
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn burst() {
  let mut i = time::interval(Duration::from_millis(100));
  i.tick().await;
  time::sleep(Duration::from_millis(250)).await;
  let missed = time::timeout(Duration::from_millis(40), async {
    i.tick().await;
    i.tick().await;
  });
  assert!(missed.await.is_ok());
}

#[wasm_bindgen_test]
async fn reset_doesnt_panic_max_duration() {
  let mut interval = time::interval(Duration::MAX);
  interval.reset();
}

#[wasm_bindgen_test]
async fn reset() {
  let mut i = time::interval(Duration::from_millis(50));
  i.tick().await;
  time::sleep(Duration::from_millis(80)).await;
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

#[wasm_bindgen_test]
async fn several_intervals() {
  let mut fast = time::interval(Duration::from_millis(20));
  let mut slow = time::interval(Duration::from_millis(100));
  let mut ticks = 0;
  loop {
    tokio::select! {
      _ = fast.tick() => ticks += 1,
      _ = slow.tick() => break,
    }
  }
  assert!(ticks >= 2, "only {ticks} fast ticks");
}

#[wasm_bindgen_test]
async fn ticks_during_spawn_blocking() {
  let mut i = time::interval(Duration::from_millis(20));
  let mut blocking =
    task::spawn_blocking(|| thread::sleep(Duration::from_millis(200)));
  let mut ticks = 0;
  loop {
    tokio::select! {
      _ = i.tick() => ticks += 1,
      _ = &mut blocking => break,
    }
  }
  assert!(ticks >= 3, "only {ticks} ticks");
}
