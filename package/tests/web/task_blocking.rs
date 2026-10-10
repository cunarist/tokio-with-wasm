use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;
use tokio::task;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::wasm_bindgen_test;
use web_sys::DedicatedWorkerGlobalScope;

#[wasm_bindgen_test]
async fn basic_blocking() {
  for _ in 0..100 {
    let out = tokio::spawn(async {
      task::spawn_blocking(|| {
        thread::sleep(Duration::from_millis(5));
        "hello"
      })
      .await
      .unwrap()
    })
    .await
    .unwrap();
    assert_eq!(out, "hello");
  }
}

#[wasm_bindgen_test]
async fn runs_in_a_web_worker() {
  let in_worker = task::spawn_blocking(|| {
    js_sys::global().is_instance_of::<DedicatedWorkerGlobalScope>()
  });
  assert!(in_worker.await.unwrap());
}

#[wasm_bindgen_test]
async fn unawaited_blocking_task_finishes() {
  let handle = task::spawn_blocking(|| thread::sleep(Duration::from_millis(1)));
  tokio::time::sleep(Duration::from_millis(500)).await;
  assert!(handle.is_finished());
  handle.await.unwrap();
}

#[wasm_bindgen_test]
async fn many_blocking_tasks_run_at_once() {
  let barrier = Arc::new(Barrier::new(32));
  let handles: Vec<_> = (0..32)
    .map(|i| {
      let barrier = barrier.clone();
      task::spawn_blocking(move || {
        barrier.wait();
        i
      })
    })
    .collect();
  let mut sum = 0;
  for handle in handles {
    sum += tokio::time::timeout(Duration::from_secs(10), handle)
      .await
      .unwrap()
      .unwrap();
  }
  assert_eq!(sum, (0..32).sum::<i32>());
}

#[wasm_bindgen_test]
async fn idle_worker_is_reused() {
  let first = task::spawn_blocking(|| thread::current().id())
    .await
    .unwrap();
  // Give the worker time to report back as idle.
  tokio::time::sleep(Duration::from_millis(50)).await;
  let second = task::spawn_blocking(|| thread::current().id())
    .await
    .unwrap();
  assert_eq!(first, second);
}

#[wasm_bindgen_test]
async fn returns_large_value() {
  let out = task::spawn_blocking(|| vec![7u8; 16 << 20]).await.unwrap();
  assert_eq!(out.len(), 16 << 20);
  assert!(out.iter().all(|&b| b == 7));
}

#[wasm_bindgen_test]
async fn dropped_handle_still_runs() {
  let (tx, rx) = tokio::sync::oneshot::channel();
  drop(task::spawn_blocking(move || {
    thread::sleep(Duration::from_millis(10));
    tx.send(()).unwrap();
  }));
  rx.await.unwrap();
}
