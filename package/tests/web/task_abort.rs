use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio::time::timeout;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn test_abort_without_panic_3157() {
  let handle = tokio::spawn(tokio::time::sleep(Duration::from_secs(100)));
  tokio::time::sleep(Duration::from_millis(10)).await;
  handle.abort();
  let result = timeout(Duration::from_secs(5), handle).await.unwrap();
  assert!(result.unwrap_err().is_cancelled());
}

#[wasm_bindgen_test]
async fn test_abort_without_panic_3662() {
  struct DropCheck(Arc<AtomicBool>);

  impl Drop for DropCheck {
    fn drop(&mut self) {
      self.0.store(true, Ordering::SeqCst);
    }
  }

  let drop_flag = Arc::new(AtomicBool::new(false));
  let drop_check = DropCheck(drop_flag.clone());
  let j = tokio::spawn(async move {
    let _drop_check = drop_check;
    std::future::pending::<()>().await;
  });

  // Abort from a web worker.
  let drop_flag2 = drop_flag.clone();
  let task = tokio::task::spawn_blocking(move || {
    assert!(!drop_flag2.load(Ordering::SeqCst));
    j.abort();
    j
  })
  .await
  .unwrap();

  let result = timeout(Duration::from_secs(5), task).await.unwrap();
  assert!(drop_flag.load(Ordering::SeqCst));
  assert!(result.unwrap_err().is_cancelled());

  tokio::spawn(tokio::task::yield_now()).await.unwrap();
}

#[wasm_bindgen_test]
async fn test_abort_wakes_task_3964() {
  let notify_dropped = Arc::new(());
  let weak_notify_dropped = Arc::downgrade(&notify_dropped);
  let handle = tokio::spawn(async move {
    let _notify_dropped = notify_dropped;
    tokio::time::sleep(Duration::from_secs(100)).await
  });
  tokio::time::sleep(Duration::from_millis(10)).await;

  handle.abort();
  drop(handle);
  tokio::time::sleep(Duration::from_millis(10)).await;
  assert!(weak_notify_dropped.upgrade().is_none());
}

#[wasm_bindgen_test]
async fn remote_abort_local_3929() {
  struct DropCheck(std::thread::ThreadId, std::marker::PhantomData<*const ()>);

  impl Drop for DropCheck {
    fn drop(&mut self) {
      assert_eq!(std::thread::current().id(), self.0);
    }
  }

  let check = DropCheck(std::thread::current().id(), std::marker::PhantomData);
  let handle = tokio::spawn(async move {
    std::future::pending::<()>().await;
    drop(check);
  });

  let abort = handle.abort_handle();
  tokio::task::spawn_blocking(move || abort.abort())
    .await
    .unwrap();
  let result = timeout(Duration::from_secs(5), handle).await.unwrap();
  assert!(result.unwrap_err().is_cancelled());
}

#[wasm_bindgen_test]
async fn abort_handle_cancels_task() {
  let handle = tokio::spawn(std::future::pending::<()>());
  let abort = handle.abort_handle();
  abort.clone().abort();
  abort.abort();
  let result = timeout(Duration::from_secs(5), handle).await.unwrap();
  assert!(result.unwrap_err().is_cancelled());
}

#[wasm_bindgen_test]
async fn abort_after_finish_keeps_output() {
  let handle = tokio::spawn(async { 7 });
  tokio::time::sleep(Duration::from_millis(10)).await;
  assert!(handle.is_finished());
  handle.abort();
  assert_eq!(handle.await.unwrap(), 7);
}

#[wasm_bindgen_test]
async fn dropping_handle_does_not_abort() {
  let (tx, rx) = tokio::sync::oneshot::channel();
  drop(tokio::spawn(async move {
    tokio::time::sleep(Duration::from_millis(10)).await;
    tx.send(()).unwrap();
  }));
  rx.await.unwrap();
}

#[wasm_bindgen_test]
async fn abort_blocking_after_start_has_no_effect() {
  let (started_tx, started_rx) = tokio::sync::oneshot::channel();
  let handle = tokio::task::spawn_blocking(move || {
    started_tx.send(()).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    5
  });
  started_rx.await.unwrap();
  handle.abort_handle().abort();
  assert_eq!(handle.await.unwrap(), 5);
}
