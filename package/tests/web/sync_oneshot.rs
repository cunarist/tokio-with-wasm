use std::future::Future;
use std::pin::{pin, Pin};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use tokio::sync::oneshot::{self, error::TryRecvError};
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[allow(unused)]
trait AssertSend: Send {}
impl AssertSend for oneshot::Sender<i32> {}
impl AssertSend for oneshot::Receiver<i32> {}

struct Flag(AtomicBool);

impl Wake for Flag {
  fn wake(self: Arc<Self>) {
    self.0.store(true, Ordering::SeqCst);
  }
}

/// A stand-in for `tokio_test::task::Spawn` that records wake-ups.
struct Task(Arc<Flag>);

impl Task {
  fn new() -> Self {
    Self(Arc::new(Flag(AtomicBool::new(false))))
  }

  fn enter<R>(&self, f: impl FnOnce(&mut Context<'_>) -> R) -> R {
    self.0 .0.store(false, Ordering::SeqCst);
    let waker = Waker::from(self.0.clone());
    f(&mut Context::from_waker(&waker))
  }

  fn poll<F: Future + Unpin>(&self, fut: &mut F) -> Poll<F::Output> {
    self.enter(|cx| Pin::new(fut).poll(cx))
  }

  fn poll_closed<T>(&self, tx: &mut oneshot::Sender<T>) -> Poll<()> {
    self.enter(|cx| pin!(tx.closed()).poll(cx))
  }

  fn is_woken(&self) -> bool {
    self.0 .0.load(Ordering::SeqCst)
  }

  fn ref_count(&self) -> usize {
    Arc::strong_count(&self.0)
  }
}

#[wasm_bindgen_test]
fn send_recv() {
  let (tx, mut rx) = oneshot::channel();
  let task = Task::new();
  assert_eq!(task.poll(&mut rx), Poll::Pending);
  tx.send(1).unwrap();
  assert!(task.is_woken());
  assert_eq!(task.poll(&mut rx), Poll::Ready(Ok(1)));
}

#[wasm_bindgen_test]
async fn async_send_recv() {
  let (tx, rx) = oneshot::channel();
  tx.send(1).unwrap();
  assert_eq!(rx.await, Ok(1));
}

#[wasm_bindgen_test]
fn close_tx() {
  let (tx, mut rx) = oneshot::channel::<i32>();
  let task = Task::new();
  assert_eq!(task.poll(&mut rx), Poll::Pending);
  drop(tx);
  assert!(task.is_woken());
  assert!(matches!(task.poll(&mut rx), Poll::Ready(Err(_))));
}

#[wasm_bindgen_test]
fn close_rx() {
  let (tx, _) = oneshot::channel();
  assert!(tx.send(1).is_err());

  let (mut tx, rx) = oneshot::channel();
  let task = Task::new();
  assert_eq!(task.poll_closed(&mut tx), Poll::Pending);
  drop(rx);
  assert!(task.is_woken());
  assert!(tx.is_closed());
  assert_eq!(task.poll_closed(&mut tx), Poll::Ready(()));
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
  let task = Task::new();
  tx.send(1).unwrap();
  rx.close();
  assert_eq!(task.poll(&mut rx), Poll::Ready(Ok(1)));

  // With and without a value sent after closing.
  for send in [true, false] {
    let (mut tx, mut rx) = oneshot::channel::<i32>();
    let task = Task::new();
    assert_eq!(task.poll_closed(&mut tx), Poll::Pending);
    rx.close();
    assert!(task.is_woken());
    assert!(tx.is_closed());
    assert_eq!(task.poll_closed(&mut tx), Poll::Ready(()));
    if send {
      assert!(tx.send(1).is_err());
    }
    assert!(matches!(task.poll(&mut rx), Poll::Ready(Err(_))));
  }
}

#[wasm_bindgen_test]
fn explicit_close_try_recv() {
  let (tx, mut rx) = oneshot::channel();
  tx.send(1).unwrap();
  rx.close();
  assert_eq!(rx.try_recv(), Ok(1));

  let (mut tx, mut rx) = oneshot::channel::<i32>();
  let task = Task::new();
  assert_eq!(task.poll_closed(&mut tx), Poll::Pending);
  rx.close();
  assert!(task.is_woken());
  assert!(tx.is_closed());
  assert_eq!(task.poll_closed(&mut tx), Poll::Ready(()));
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
  let task = Task::new();
  tx.send(17).unwrap();
  assert_eq!(task.poll(&mut rx), Poll::Ready(Ok(17)));
  assert_eq!(rx.try_recv(), Err(TryRecvError::Closed));
  rx.close();
}

#[wasm_bindgen_test]
fn drops_tasks() {
  let (mut tx, mut rx) = oneshot::channel::<i32>();
  let tx_task = Task::new();
  let rx_task = Task::new();
  assert_eq!(tx_task.poll_closed(&mut tx), Poll::Pending);
  assert_eq!(rx_task.poll(&mut rx), Poll::Pending);
  drop(tx);
  drop(rx);
  assert_eq!(tx_task.ref_count(), 1);
  assert_eq!(rx_task.ref_count(), 1);
}

#[wasm_bindgen_test]
fn receiver_changes_task() {
  let (tx, mut rx) = oneshot::channel();
  let task1 = Task::new();
  let task2 = Task::new();

  assert_eq!(task1.poll(&mut rx), Poll::Pending);
  assert_eq!((task1.ref_count(), task2.ref_count()), (2, 1));

  assert_eq!(task2.poll(&mut rx), Poll::Pending);
  assert_eq!((task1.ref_count(), task2.ref_count()), (1, 2));

  tx.send(1).unwrap();
  assert!(!task1.is_woken());
  assert!(task2.is_woken());
  assert_eq!(task2.poll(&mut rx), Poll::Ready(Ok(1)));
}

#[wasm_bindgen_test]
fn sender_changes_task() {
  let (mut tx, rx) = oneshot::channel::<i32>();
  let task1 = Task::new();
  let task2 = Task::new();

  assert_eq!(task1.poll_closed(&mut tx), Poll::Pending);
  assert_eq!((task1.ref_count(), task2.ref_count()), (2, 1));

  assert_eq!(task2.poll_closed(&mut tx), Poll::Pending);
  assert_eq!((task1.ref_count(), task2.ref_count()), (1, 2));

  drop(rx);
  assert!(!task1.is_woken());
  assert!(task2.is_woken());
  assert_eq!(task2.poll_closed(&mut tx), Poll::Ready(()));
}

#[wasm_bindgen_test]
fn receiver_is_terminated_send() {
  let (tx, mut rx) = oneshot::channel::<i32>();
  assert!(!rx.is_terminated());
  tx.send(17).unwrap();
  assert!(!rx.is_terminated());
  assert_eq!(Task::new().poll(&mut rx), Poll::Ready(Ok(17)));
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
  assert!(matches!(Task::new().poll(&mut rx), Poll::Ready(Err(_))));
  assert!(rx.is_terminated());
}

#[wasm_bindgen_test]
fn receiver_is_terminated_rx_close() {
  let (_tx, mut rx) = oneshot::channel::<i32>();
  assert!(!rx.is_terminated());
  rx.close();
  assert!(!rx.is_terminated());
  assert!(matches!(Task::new().poll(&mut rx), Poll::Ready(Err(_))));
  assert!(rx.is_terminated());
}

#[wasm_bindgen_test]
fn receiver_is_empty_send() {
  let (tx, mut rx) = oneshot::channel::<i32>();
  assert!(rx.is_empty());
  tx.send(17).unwrap();
  assert!(!rx.is_empty());
  assert_eq!(Task::new().poll(&mut rx), Poll::Ready(Ok(17)));
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
  assert!(matches!(Task::new().poll(&mut rx), Poll::Ready(Err(_))));
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
