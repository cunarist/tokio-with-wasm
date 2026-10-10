use crate::support::{assert_pending, assert_ready, spawn};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn straight_execution() {
  let l = Mutex::new(100);
  for expected in [100, 99, 98] {
    let mut g = l.try_lock().unwrap();
    assert_eq!(*g, expected);
    *g -= 1;
  }
}

#[wasm_bindgen_test]
fn readiness() {
  let l = Mutex::new(100);
  let mut t1 = spawn(l.lock());
  let mut t2 = spawn(l.lock());

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
    let _g = m2.lock().await;
    std::future::pending::<()>().await;
  })
  .await
  .unwrap_err();
  let timeout = tokio::time::timeout(Duration::from_millis(1000), m1.lock());
  drop(timeout.await.expect("Mutex is locked"));
}

#[wasm_bindgen_test]
async fn aborted_future_2() {
  let m1 = Arc::new(Mutex::new(0usize));
  {
    let _lock = m1.lock().await;
    let m2 = m1.clone();
    tokio::time::timeout(Duration::from_millis(1), async move {
      let _g = m2.lock().await;
    })
    .await
    .unwrap_err();
  }
  let timeout = tokio::time::timeout(Duration::from_millis(1000), m1.lock());
  drop(timeout.await.expect("Mutex is locked"));
}

#[wasm_bindgen_test]
fn try_lock() {
  let m = Mutex::new(0usize);
  {
    assert!(m.try_lock().is_ok());
    let _g = m.try_lock().unwrap();
    assert!(m.try_lock().is_err());
  }
  assert!(m.try_lock().is_ok());
}

#[wasm_bindgen_test]
async fn debug_format() {
  let s = "debug";
  let m = Mutex::new(s.to_string());
  assert_eq!(format!("{s:?}"), format!("{:?}", m.lock().await));
}

#[wasm_bindgen_test]
async fn mutex_debug() {
  let m = Mutex::new("data".to_string());
  assert_eq!(format!("{m:?}"), r#"Mutex { data: "data" }"#);
  let _guard = m.lock().await;
  assert_eq!(format!("{m:?}"), r#"Mutex { data: <locked> }"#);
}

#[wasm_bindgen_test]
async fn lock_held_by_a_web_worker() {
  let m = Arc::new(Mutex::new(0));
  let m2 = m.clone();
  let (tx, rx) = tokio::sync::oneshot::channel();
  let task = tokio::task::spawn_blocking(move || {
    let mut g = m2.try_lock().unwrap();
    tx.send(()).unwrap();
    *g = 7;
  });
  rx.await.unwrap();
  task.await.unwrap();
  assert_eq!(*m.lock().await, 7);
}

#[wasm_bindgen_test]
async fn worker_unlock_wakes_main_thread() {
  let m = Arc::new(Mutex::new(0));
  let mut g = m.clone().try_lock_owned().unwrap();
  tokio::task::spawn_blocking(move || {
    std::thread::sleep(Duration::from_millis(50));
    *g = 5;
  });
  assert_eq!(*m.lock().await, 5);
}
