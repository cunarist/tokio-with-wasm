use tokio::sync::{mpsc, oneshot, watch};
use tokio::task::spawn_blocking;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

fn is_error<T: std::error::Error + Send + Sync>() {}

#[wasm_bindgen_test]
fn mpsc_error_bound() {
  is_error::<mpsc::error::SendError<()>>();
  is_error::<mpsc::error::TrySendError<()>>();
}

#[wasm_bindgen_test]
fn oneshot_error_bound() {
  is_error::<oneshot::error::RecvError>();
  is_error::<oneshot::error::TryRecvError>();
}

#[wasm_bindgen_test]
fn watch_error_bound() {
  is_error::<watch::error::SendError<()>>();
}

#[wasm_bindgen_test]
async fn mpsc_send_error_from_worker() {
  let (tx, rx) = mpsc::channel(1);
  drop(rx);
  let err = spawn_blocking(move || tx.blocking_send(7).unwrap_err())
    .await
    .unwrap();
  assert_eq!(err.0, 7);
  assert_eq!(err.to_string(), "channel closed");
}

#[wasm_bindgen_test]
async fn mpsc_try_send_error_from_worker() {
  let (tx, rx) = mpsc::channel(1);
  let (full, closed) = spawn_blocking(move || {
    tx.try_send(1).unwrap();
    let full = tx.try_send(2).unwrap_err();
    drop(rx);
    (full, tx.try_send(3).unwrap_err())
  })
  .await
  .unwrap();
  assert!(matches!(full, mpsc::error::TrySendError::Full(2)));
  assert!(matches!(closed, mpsc::error::TrySendError::Closed(3)));
}

#[wasm_bindgen_test]
async fn oneshot_recv_error_from_worker() {
  let (tx, rx) = oneshot::channel::<()>();
  let handle = spawn_blocking(move || {
    drop(tx);
  });
  assert!(rx.await.is_err());
  handle.await.unwrap();

  let (tx, rx) = oneshot::channel::<()>();
  drop(tx);
  let err = spawn_blocking(move || rx.blocking_recv().unwrap_err())
    .await
    .unwrap();
  assert_eq!(err.to_string(), "channel closed");
}

#[wasm_bindgen_test]
async fn oneshot_try_recv_error_from_worker() {
  let (tx, mut rx) = oneshot::channel::<u8>();
  let (empty, closed) = spawn_blocking(move || {
    let empty = rx.try_recv().unwrap_err();
    drop(tx);
    (empty, rx.try_recv().unwrap_err())
  })
  .await
  .unwrap();
  assert_eq!(empty, oneshot::error::TryRecvError::Empty);
  assert_eq!(closed, oneshot::error::TryRecvError::Closed);
}

#[wasm_bindgen_test]
async fn watch_send_error_from_worker() {
  let (tx, rx) = watch::channel(0);
  drop(rx);
  let err = spawn_blocking(move || tx.send(5).unwrap_err())
    .await
    .unwrap();
  assert_eq!(err.0, 5);
  assert_eq!(err.to_string(), "channel closed");
}
