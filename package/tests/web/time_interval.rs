use crate::support::{assert_elapsed, assert_ready, spawn};
use js_sys::{Function, Reflect};
use std::cell::Cell;
use std::rc::Rc;
use std::thread;
use std::time::Duration;
use tokio::{task, time};
use tokio_with_wasm::alias as tokio;
use wasm_bindgen::prelude::{Closure, JsValue};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn burst() {
  let mut i = time::interval(Duration::from_millis(20));
  i.tick().await;
  time::sleep(Duration::from_millis(300)).await;
  assert_ready!(spawn(i.tick()).poll());
  assert_ready!(spawn(i.tick()).poll());
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
  let mut slow = time::interval(Duration::from_millis(200));
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

#[wasm_bindgen_test]
fn drop_clears_the_interval() {
  let global = js_sys::global();
  let key = JsValue::from("clearInterval");
  let original: Function = Reflect::get(&global, &key).unwrap().into();
  let cleared = Rc::new(Cell::new(0));
  let spy = Closure::<dyn Fn(JsValue)>::new({
    let (original, cleared) = (original.clone(), cleared.clone());
    move |id| {
      cleared.set(cleared.get() + 1);
      original.call1(&JsValue::NULL, &id).unwrap();
    }
  });
  Reflect::set(&global, &key, spy.as_ref()).unwrap();
  drop(time::interval(Duration::from_millis(10)));
  Reflect::set(&global, &key, &original).unwrap();
  assert_eq!(cleared.get(), 1);
}
