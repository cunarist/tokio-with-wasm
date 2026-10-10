use crate::support::{assert_pending, assert_ready, spawn};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn straight_execution() {
  let l = Arc::new(Mutex::new(100));
  for expected in [100, 99, 98] {
    let mut g = l.clone().try_lock_owned().unwrap();
    assert_eq!(*g, expected);
    *g -= 1;
  }
}

#[wasm_bindgen_test]
fn readiness() {
  let l = Arc::new(Mutex::new(100));
  let mut t1 = spawn(l.clone().lock_owned());
  let mut t2 = spawn(l.lock_owned());

  let g = assert_ready!(t1.poll());
  assert_pending!(t2.poll());

  drop(g);
  assert!(t2.is_woken());
  let _t2 = assert_ready!(t2.poll());
}

#[wasm_bindgen_test]
async fn aborted_future_1() {
  let m1 = Arc::new(Mutex::new(0usize));
  let m2 = m1.clone();
  tokio::time::timeout(Duration::from_millis(1), async move {
    let _g = m2.lock_owned().await;
    std::future::pending::<()>().await;
  })
  .await
  .unwrap_err();
  tokio::time::timeout(Duration::from_millis(1000), m1.lock_owned())
    .await
    .expect("Mutex is locked");
}

#[wasm_bindgen_test]
async fn aborted_future_2() {
  let m1 = Arc::new(Mutex::new(0usize));
  {
    let _lock = m1.clone().lock_owned().await;
    let m2 = m1.clone();
    tokio::time::timeout(Duration::from_millis(1), async move {
      let _g = m2.lock_owned().await;
    })
    .await
    .unwrap_err();
  }
  tokio::time::timeout(Duration::from_millis(1000), m1.lock_owned())
    .await
    .expect("Mutex is locked");
}

#[wasm_bindgen_test]
fn try_lock_owned() {
  let m = Arc::new(Mutex::new(0usize));
  {
    let _g = m.clone().try_lock_owned().unwrap();
    assert!(m.clone().try_lock_owned().is_err());
  }
  assert!(m.try_lock_owned().is_ok());
}

#[wasm_bindgen_test]
async fn debug_format() {
  let s = "debug";
  let m = Arc::new(Mutex::new(s.to_string()));
  assert_eq!(format!("{s:?}"), format!("{:?}", m.lock_owned().await));
}

#[wasm_bindgen_test]
async fn guard_moves_to_a_web_worker() {
  let m = Arc::new(Mutex::new(0));
  let mut g = m.clone().lock_owned().await;
  let task = tokio::task::spawn_blocking(move || {
    std::thread::sleep(Duration::from_millis(50));
    *g = 5;
  });
  assert!(m.clone().try_lock_owned().is_err());
  assert_eq!(*m.lock_owned().await, 5);
  task.await.unwrap();
}
