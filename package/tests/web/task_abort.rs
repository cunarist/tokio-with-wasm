use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn test_abort_without_panic_3157() {
  let handle = tokio::spawn(tokio::time::sleep(Duration::from_secs(100)));
  tokio::time::sleep(Duration::from_millis(10)).await;
  handle.abort();
  assert!(handle.await.unwrap_err().is_cancelled());
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

  let result = task.await;
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
