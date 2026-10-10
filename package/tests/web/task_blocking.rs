use std::cell::Cell;
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
  let finished = async {
    while !handle.is_finished() {
      tokio::time::sleep(Duration::from_millis(10)).await;
    }
  };
  tokio::time::timeout(Duration::from_secs(10), finished)
    .await
    .unwrap();
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
  thread_local!(static USED: Cell<bool> = const { Cell::new(false) });
  // Workers of earlier tests can go idle in between, so retry until a task
  // lands on a worker that this test has used before.
  for _ in 0..100 {
    if task::spawn_blocking(|| USED.replace(true)).await.unwrap() {
      return;
    }
    tokio::time::sleep(Duration::from_millis(10)).await;
  }
  panic!("no worker was reused");
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

// A worker may not spawn, so the task fails instead of returning.
#[wasm_bindgen_test]
async fn spawn_is_rejected_in_a_web_worker() {
  let spawned = task::spawn_blocking(|| drop(tokio::spawn(async {})));
  let joined = tokio::time::timeout(Duration::from_millis(500), spawned).await;
  assert!(!matches!(joined, Ok(Ok(()))));
  let spawned = task::spawn_blocking(|| drop(task::spawn_blocking(|| {})));
  let joined = tokio::time::timeout(Duration::from_millis(500), spawned).await;
  assert!(!matches!(joined, Ok(Ok(()))));
}

#[wasm_bindgen_test]
async fn culled_workers_free_their_memory() {
  let memory_size = || {
    let memory =
      wasm_bindgen::memory().unchecked_into::<js_sys::WebAssembly::Memory>();
    memory
      .buffer()
      .unchecked_into::<js_sys::SharedArrayBuffer>()
      .byte_length()
  };
  let mut sizes = Vec::new();
  for _ in 0..3 {
    let barrier = Arc::new(Barrier::new(8));
    let handles: Vec<_> = (0..8)
      .map(|_| {
        let barrier = barrier.clone();
        task::spawn_blocking(move || drop(barrier.wait()))
      })
      .collect();
    for handle in handles {
      handle.await.unwrap();
    }
    // Idle workers are culled after 10 seconds.
    tokio::time::sleep(Duration::from_secs(11)).await;
    sizes.push(memory_size());
  }
  // Each leaked worker would hold a 2 MB stack.
  assert!(sizes[2] - sizes[0] < 8 << 20, "{sizes:?}");
}
