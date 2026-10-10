use crate::support::{
  assert_pending, assert_ready, assert_ready_err, assert_ready_ok, spawn,
};
use std::future::Future;
use std::pin::Pin;
use tokio::sync::oneshot::{self, error::TryRecvError};
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[allow(unused)]
trait AssertSend: Send {}
impl AssertSend for oneshot::Sender<i32> {}
impl AssertSend for oneshot::Receiver<i32> {}

#[wasm_bindgen_test]
fn send_recv() {
  let (tx, rx) = oneshot::channel();
  let mut rx = spawn(rx);
  assert_pending!(rx.poll());
  tx.send(1).unwrap();
  assert!(rx.is_woken());
  assert_eq!(assert_ready_ok!(rx.poll()), 1);
}

#[wasm_bindgen_test]
async fn async_send_recv() {
  let (tx, rx) = oneshot::channel();
  tx.send(1).unwrap();
  assert_eq!(rx.await, Ok(1));
}

#[wasm_bindgen_test]
fn close_tx() {
  let (tx, rx) = oneshot::channel::<i32>();
  let mut rx = spawn(rx);
  assert_pending!(rx.poll());
  drop(tx);
  assert!(rx.is_woken());
  assert_ready_err!(rx.poll());
}

#[wasm_bindgen_test]
fn close_rx() {
  let (tx, _) = oneshot::channel();
  assert!(tx.send(1).is_err());

  let (mut tx, rx) = oneshot::channel();
  let mut task = spawn(());
  assert_pending!(task.enter(|cx, _| tx.poll_closed(cx)));
  drop(rx);
  assert!(task.is_woken());
  assert!(tx.is_closed());
  assert_ready!(task.enter(|cx, _| tx.poll_closed(cx)));
  assert!(tx.send(1).is_err());
}

#[wasm_bindgen_test]
async fn async_rx_closed() {
  let (mut tx, rx) = oneshot::channel::<()>();
  tokio::spawn(async move {
    drop(rx);
  });
  tx.closed().await;
}

#[wasm_bindgen_test]
fn explicit_close_poll() {
  let (tx, mut rx) = oneshot::channel();
  tx.send(1).unwrap();
  rx.close();
  assert_eq!(assert_ready_ok!(spawn(&mut rx).poll()), 1);

  // With and without a value sent after closing.
  for send in [true, false] {
    let (mut tx, mut rx) = oneshot::channel::<i32>();
    let mut task = spawn(());
    assert_pending!(task.enter(|cx, _| tx.poll_closed(cx)));
    rx.close();
    assert!(task.is_woken());
    assert!(tx.is_closed());
    assert_ready!(task.enter(|cx, _| tx.poll_closed(cx)));
    if send {
      assert!(tx.send(1).is_err());
    }
    assert_ready_err!(spawn(&mut rx).poll());
  }
}

#[wasm_bindgen_test]
fn explicit_close_try_recv() {
  let (tx, mut rx) = oneshot::channel();
  tx.send(1).unwrap();
  rx.close();
  assert_eq!(rx.try_recv(), Ok(1));

  let (mut tx, mut rx) = oneshot::channel::<i32>();
  let mut task = spawn(());
  assert_pending!(task.enter(|cx, _| tx.poll_closed(cx)));
  rx.close();
  assert!(task.is_woken());
  assert!(tx.is_closed());
  assert_ready!(task.enter(|cx, _| tx.poll_closed(cx)));
  assert!(rx.try_recv().is_err());
}

#[wasm_bindgen_test]
fn close_after_recv() {
  let (tx, mut rx) = oneshot::channel::<i32>();
  tx.send(17).unwrap();
  assert_eq!(rx.try_recv(), Ok(17));
  rx.close();
}

#[wasm_bindgen_test]
fn try_recv_after_completion() {
  let (tx, mut rx) = oneshot::channel::<i32>();
  tx.send(17).unwrap();
  assert_eq!(rx.try_recv(), Ok(17));
  assert_eq!(rx.try_recv(), Err(TryRecvError::Closed));
  rx.close();
}

#[wasm_bindgen_test]
fn try_recv_after_completion_await() {
  let (tx, mut rx) = oneshot::channel::<i32>();
  tx.send(17).unwrap();
  assert_eq!(assert_ready_ok!(spawn(&mut rx).poll()), 17);
  assert_eq!(rx.try_recv(), Err(TryRecvError::Closed));
  rx.close();
}

#[wasm_bindgen_test]
fn drops_tasks() {
  let (mut tx, mut rx) = oneshot::channel::<i32>();
  let mut tx_task = spawn(());
  let mut rx_task = spawn(());
  assert_pending!(tx_task.enter(|cx, _| tx.poll_closed(cx)));
  assert_pending!(rx_task.enter(|cx, _| Pin::new(&mut rx).poll(cx)));
  drop(tx);
  drop(rx);
  assert_eq!(tx_task.waker_ref_count(), 1);
  assert_eq!(rx_task.waker_ref_count(), 1);
}

#[wasm_bindgen_test]
fn receiver_changes_task() {
  let (tx, mut rx) = oneshot::channel();
  let mut task1 = spawn(());
  let mut task2 = spawn(());

  assert_pending!(task1.enter(|cx, _| Pin::new(&mut rx).poll(cx)));
  assert_eq!((task1.waker_ref_count(), task2.waker_ref_count()), (2, 1));

  assert_pending!(task2.enter(|cx, _| Pin::new(&mut rx).poll(cx)));
  assert_eq!((task1.waker_ref_count(), task2.waker_ref_count()), (1, 2));

  tx.send(1).unwrap();
  assert!(!task1.is_woken());
  assert!(task2.is_woken());
  let poll = task2.enter(|cx, _| Pin::new(&mut rx).poll(cx));
  assert_eq!(assert_ready_ok!(poll), 1);
}

#[wasm_bindgen_test]
fn sender_changes_task() {
  let (mut tx, rx) = oneshot::channel::<i32>();
  let mut task1 = spawn(());
  let mut task2 = spawn(());

  assert_pending!(task1.enter(|cx, _| tx.poll_closed(cx)));
  assert_eq!((task1.waker_ref_count(), task2.waker_ref_count()), (2, 1));

  assert_pending!(task2.enter(|cx, _| tx.poll_closed(cx)));
  assert_eq!((task1.waker_ref_count(), task2.waker_ref_count()), (1, 2));

  drop(rx);
  assert!(!task1.is_woken());
  assert!(task2.is_woken());
  assert_ready!(task2.enter(|cx, _| tx.poll_closed(cx)));
}

#[wasm_bindgen_test]
fn receiver_is_terminated_send() {
  let (tx, mut rx) = oneshot::channel::<i32>();
  assert!(!rx.is_terminated());
  tx.send(17).unwrap();
  assert!(!rx.is_terminated());
  assert_eq!(assert_ready_ok!(spawn(&mut rx).poll()), 17);
  assert!(rx.is_terminated());
}

#[wasm_bindgen_test]
fn receiver_is_terminated_try_recv() {
  let (tx, mut rx) = oneshot::channel::<i32>();
  assert!(!rx.is_terminated());
  tx.send(17).unwrap();
  assert!(!rx.is_terminated());
  assert_eq!(rx.try_recv(), Ok(17));
  assert!(rx.is_terminated());
}

#[wasm_bindgen_test]
fn receiver_is_terminated_drop() {
  let (tx, mut rx) = oneshot::channel::<i32>();
  assert!(!rx.is_terminated());
  drop(tx);
  assert!(!rx.is_terminated());
  assert_ready_err!(spawn(&mut rx).poll());
  assert!(rx.is_terminated());
}

#[wasm_bindgen_test]
fn receiver_is_terminated_rx_close() {
  let (_tx, mut rx) = oneshot::channel::<i32>();
  assert!(!rx.is_terminated());
  rx.close();
  assert!(!rx.is_terminated());
  assert_ready_err!(spawn(&mut rx).poll());
  assert!(rx.is_terminated());
}

#[wasm_bindgen_test]
fn receiver_is_empty_send() {
  let (tx, mut rx) = oneshot::channel::<i32>();
  assert!(rx.is_empty());
  tx.send(17).unwrap();
  assert!(!rx.is_empty());
  assert_eq!(assert_ready_ok!(spawn(&mut rx).poll()), 17);
  assert!(rx.is_empty());
}

#[wasm_bindgen_test]
fn receiver_is_empty_try_recv() {
  let (tx, mut rx) = oneshot::channel::<i32>();
  assert!(rx.is_empty());
  tx.send(17).unwrap();
  assert!(!rx.is_empty());
  assert_eq!(rx.try_recv(), Ok(17));
  assert!(rx.is_empty());
}

#[wasm_bindgen_test]
fn receiver_is_empty_drop() {
  let (tx, mut rx) = oneshot::channel::<i32>();
  assert!(rx.is_empty());
  drop(tx);
  assert!(rx.is_empty());
  assert_ready_err!(spawn(&mut rx).poll());
  assert!(rx.is_empty());
}

#[wasm_bindgen_test]
fn receiver_is_empty_rx_close() {
  let (_tx, mut rx) = oneshot::channel::<i32>();
  assert!(rx.is_empty());
  rx.close();
  assert!(rx.is_empty());
}

#[wasm_bindgen_test]
async fn send_from_a_web_worker() {
  let (tx, rx) = oneshot::channel();
  tokio::task::spawn_blocking(move || tx.send("hello").unwrap());
  assert_eq!(rx.await, Ok("hello"));
}

#[wasm_bindgen_test]
async fn sender_dropped_in_a_web_worker() {
  let (tx, rx) = oneshot::channel::<()>();
  tokio::task::spawn_blocking(move || drop(tx));
  assert!(rx.await.is_err());
}

#[wasm_bindgen_test]
async fn receive_in_a_web_worker() {
  let (tx, rx) = oneshot::channel();
  let worker = tokio::task::spawn_blocking(move || rx.blocking_recv());
  tx.send(5).unwrap();
  assert_eq!(worker.await.unwrap(), Ok(5));
}

#[wasm_bindgen_test]
async fn receiver_dropped_in_a_web_worker() {
  let (mut tx, rx) = oneshot::channel::<()>();
  tokio::task::spawn_blocking(move || drop(rx));
  tx.closed().await;
  assert!(tx.send(()).is_err());
}

#[wasm_bindgen_test]
async fn round_trip_through_a_web_worker() {
  let (tx, rx) = oneshot::channel::<i32>();
  let (back_tx, back_rx) = oneshot::channel();
  tokio::task::spawn_blocking(move || {
    back_tx.send(rx.blocking_recv().unwrap() + 1).unwrap();
  });
  tx.send(1).unwrap();
  assert_eq!(back_rx.await, Ok(2));
}
