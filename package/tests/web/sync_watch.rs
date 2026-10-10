use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use tokio::sync::watch;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[derive(Default)]
struct Flag(AtomicBool);

impl Wake for Flag {
  fn wake(self: Arc<Self>) {
    self.0.store(true, Ordering::SeqCst);
  }
}

struct Task<F> {
  fut: Pin<Box<F>>,
  flag: Arc<Flag>,
}

fn spawn<F: Future>(fut: F) -> Task<F> {
  Task {
    fut: Box::pin(fut),
    flag: Arc::default(),
  }
}

impl<F: Future> Task<F> {
  fn poll(&mut self) -> Poll<F::Output> {
    self.flag.0.store(false, Ordering::SeqCst);
    let waker = Waker::from(self.flag.clone());
    self.fut.as_mut().poll(&mut Context::from_waker(&waker))
  }

  fn is_woken(&self) -> bool {
    self.flag.0.load(Ordering::SeqCst)
  }
}

macro_rules! assert_pending {
  ($e:expr) => {
    assert!($e.is_pending())
  };
}

macro_rules! assert_ready_ok {
  ($e:expr) => {
    match $e {
      Poll::Ready(Ok(v)) => v,
      other => panic!("expected ready ok, got {:?}", other.map(|r| r.is_ok())),
    }
  };
}

macro_rules! assert_ready_err {
  ($e:expr) => {
    match $e {
      Poll::Ready(Err(e)) => e,
      other => panic!("expected ready err, got {:?}", other.map(|r| r.is_ok())),
    }
  };
}

#[wasm_bindgen_test]
fn single_rx_recv() {
  let (tx, mut rx) = watch::channel("one");

  {
    let mut t = spawn(rx.changed());
    assert_pending!(t.poll());
  }
  assert_eq!(*rx.borrow(), "one");

  {
    let mut t = spawn(rx.changed());
    assert_pending!(t.poll());
    tx.send("two").unwrap();
    assert!(t.is_woken());
    assert_ready_ok!(t.poll());
  }
  assert_eq!(*rx.borrow(), "two");

  {
    let mut t = spawn(rx.changed());
    assert_pending!(t.poll());
    drop(tx);
    assert!(t.is_woken());
    assert_ready_err!(t.poll());
  }
  assert_eq!(*rx.borrow(), "two");
}

#[wasm_bindgen_test]
fn rx_version_underflow() {
  let (_tx, mut rx) = watch::channel("one");
  rx.mark_changed();
  rx.mark_changed();
}

#[wasm_bindgen_test]
fn rx_mark_changed() {
  let (tx, mut rx) = watch::channel("one");

  let mut rx2 = rx.clone();
  let mut rx3 = rx.clone();
  let mut rx4 = rx.clone();
  {
    rx.mark_changed();
    assert!(rx.has_changed().unwrap());

    let mut t = spawn(rx.changed());
    assert_ready_ok!(t.poll());
  }

  {
    assert!(!rx2.has_changed().unwrap());

    let mut t = spawn(rx2.changed());
    assert_pending!(t.poll());
  }

  {
    rx3.mark_changed();
    assert_eq!(*rx3.borrow(), "one");
    assert!(rx3.has_changed().unwrap());
    assert_eq!(*rx3.borrow_and_update(), "one");
    assert!(!rx3.has_changed().unwrap());

    let mut t = spawn(rx3.changed());
    assert_pending!(t.poll());
  }

  {
    tx.send("two").unwrap();
    assert!(rx4.has_changed().unwrap());
    assert_eq!(*rx4.borrow_and_update(), "two");

    rx4.mark_changed();
    assert!(rx4.has_changed().unwrap());
    assert_eq!(*rx4.borrow_and_update(), "two")
  }

  assert_eq!(*rx.borrow(), "two");
}

#[wasm_bindgen_test]
fn rx_mark_unchanged() {
  let (tx, mut rx) = watch::channel("one");
  let mut rx2 = rx.clone();

  {
    assert!(!rx.has_changed().unwrap());

    rx.mark_changed();
    assert!(rx.has_changed().unwrap());

    rx.mark_unchanged();
    assert!(!rx.has_changed().unwrap());

    let mut t = spawn(rx.changed());
    assert_pending!(t.poll());
  }

  {
    assert!(!rx2.has_changed().unwrap());

    tx.send("two").unwrap();
    assert!(rx2.has_changed().unwrap());

    rx2.mark_unchanged();
    assert!(!rx2.has_changed().unwrap());
    assert_eq!(*rx2.borrow_and_update(), "two");
  }

  assert_eq!(*rx.borrow(), "two");
}

#[wasm_bindgen_test]
fn multi_rx() {
  let (tx, mut rx1) = watch::channel("one");
  let mut rx2 = rx1.clone();

  {
    let mut t1 = spawn(rx1.changed());
    let mut t2 = spawn(rx2.changed());

    assert_pending!(t1.poll());
    assert_pending!(t2.poll());
  }
  assert_eq!(*rx1.borrow(), "one");
  assert_eq!(*rx2.borrow(), "one");

  let mut t2 = spawn(rx2.changed());

  {
    let mut t1 = spawn(rx1.changed());

    assert_pending!(t1.poll());
    assert_pending!(t2.poll());

    tx.send("two").unwrap();

    assert!(t1.is_woken());
    assert!(t2.is_woken());

    assert_ready_ok!(t1.poll());
  }
  assert_eq!(*rx1.borrow(), "two");

  {
    let mut t1 = spawn(rx1.changed());

    assert_pending!(t1.poll());

    tx.send("three").unwrap();

    assert!(t1.is_woken());
    assert!(t2.is_woken());

    assert_ready_ok!(t1.poll());
    assert_ready_ok!(t2.poll());
  }
  assert_eq!(*rx1.borrow(), "three");

  drop(t2);

  assert_eq!(*rx2.borrow(), "three");

  {
    let mut t1 = spawn(rx1.changed());
    let mut t2 = spawn(rx2.changed());

    assert_pending!(t1.poll());
    assert_pending!(t2.poll());

    tx.send("four").unwrap();

    assert_ready_ok!(t1.poll());
    assert_ready_ok!(t2.poll());
  }
  assert_eq!(*rx1.borrow(), "four");
  assert_eq!(*rx2.borrow(), "four");
}

#[wasm_bindgen_test]
fn rx_observes_final_value() {
  let (tx, mut rx) = watch::channel("one");
  drop(tx);

  {
    let mut t1 = spawn(rx.changed());
    assert_ready_err!(t1.poll());
  }
  assert_eq!(*rx.borrow(), "one");

  let (tx, mut rx) = watch::channel("one");
  tx.send("two").unwrap();

  {
    let mut t1 = spawn(rx.changed());
    assert_ready_ok!(t1.poll());
  }
  assert_eq!(*rx.borrow(), "two");

  {
    let mut t1 = spawn(rx.changed());
    assert_pending!(t1.poll());

    tx.send("three").unwrap();
    drop(tx);

    assert!(t1.is_woken());
    assert_ready_ok!(t1.poll());
  }
  assert_eq!(*rx.borrow(), "three");

  {
    let mut t1 = spawn(rx.changed());
    assert_ready_err!(t1.poll());
  }
  assert_eq!(*rx.borrow(), "three");
}

#[wasm_bindgen_test]
fn poll_close() {
  let (tx, rx) = watch::channel("one");

  {
    let mut t = spawn(tx.closed());
    assert_pending!(t.poll());

    drop(rx);

    assert!(t.is_woken());
    assert!(t.poll().is_ready());
  }

  assert!(tx.send("two").is_err());
}

#[wasm_bindgen_test]
fn borrow_and_update() {
  let (tx, mut rx) = watch::channel("one");

  assert!(!rx.has_changed().unwrap());

  tx.send("two").unwrap();
  assert!(rx.has_changed().unwrap());
  assert_ready_ok!(spawn(rx.changed()).poll());
  assert_pending!(spawn(rx.changed()).poll());
  assert!(!rx.has_changed().unwrap());

  tx.send("three").unwrap();
  assert!(rx.has_changed().unwrap());
  assert_eq!(*rx.borrow_and_update(), "three");
  assert_pending!(spawn(rx.changed()).poll());
  assert!(!rx.has_changed().unwrap());

  drop(tx);
  assert_eq!(*rx.borrow_and_update(), "three");
  assert_ready_err!(spawn(rx.changed()).poll());
  assert!(rx.has_changed().is_err());
}

#[wasm_bindgen_test]
fn reopened_after_subscribe() {
  let (tx, rx) = watch::channel("one");
  assert!(!tx.is_closed());

  drop(rx);
  assert!(tx.is_closed());

  let rx = tx.subscribe();
  assert!(!tx.is_closed());

  drop(rx);
  assert!(tx.is_closed());
}

#[wasm_bindgen_test]
fn multiple_sender() {
  let (tx1, mut rx) = watch::channel(0);
  let tx2 = tx1.clone();

  let mut t = spawn(async {
    rx.changed().await.unwrap();
    let v1 = *rx.borrow_and_update();
    rx.changed().await.unwrap();
    let v2 = *rx.borrow_and_update();
    (v1, v2)
  });

  tx1.send(1).unwrap();
  assert_pending!(t.poll());
  tx2.send(2).unwrap();
  assert_eq!(t.poll(), Poll::Ready((1, 2)));
}

#[wasm_bindgen_test]
fn receiver_is_notified_when_last_sender_is_dropped() {
  let (tx1, mut rx) = watch::channel(0);
  let tx2 = tx1.clone();

  let mut t = spawn(rx.changed());
  assert_pending!(t.poll());

  drop(tx1);
  assert!(!t.is_woken());
  drop(tx2);

  assert!(t.is_woken());
}

#[wasm_bindgen_test]
async fn changed_succeeds_on_closed_channel_with_unseen_value() {
  let (tx, mut rx) = watch::channel("A");
  tx.send("B").unwrap();
  drop(tx);

  rx.changed().await.unwrap();
}

#[wasm_bindgen_test]
async fn changed_errors_on_closed_channel_with_seen_value() {
  let (tx, mut rx) = watch::channel("A");
  drop(tx);

  rx.changed().await.unwrap_err();
}

#[wasm_bindgen_test]
fn has_changed_errors_on_closed_channel_with_unseen_value() {
  let (tx, rx) = watch::channel("A");
  tx.send("B").unwrap();
  drop(tx);

  rx.has_changed().unwrap_err();
}

#[wasm_bindgen_test]
fn has_changed_errors_on_closed_channel_with_seen_value() {
  let (tx, rx) = watch::channel("A");
  drop(tx);

  rx.has_changed().unwrap_err();
}

#[wasm_bindgen_test]
async fn wait_for_errors_on_closed_channel_true_predicate() {
  let (tx, mut rx) = watch::channel("A");
  tx.send("B").unwrap();
  drop(tx);

  rx.wait_for(|_| true).await.unwrap();
}

#[wasm_bindgen_test]
async fn wait_for_resolves_when_predicate_matches() {
  let (tx, mut rx) = watch::channel(0);
  tokio::spawn(async move {
    for i in 1..=3 {
      tx.send(i).unwrap();
      tokio::task::yield_now().await;
    }
  });
  assert_eq!(*rx.wait_for(|v| *v == 3).await.unwrap(), 3);
}

#[wasm_bindgen_test]
async fn changed_wakes_a_spawned_task() {
  let (tx, mut rx) = watch::channel(0);
  let task = tokio::spawn(async move {
    rx.changed().await.unwrap();
    *rx.borrow()
  });
  tokio::task::yield_now().await;
  tx.send(7).unwrap();
  assert_eq!(task.await.unwrap(), 7);
}

#[wasm_bindgen_test]
async fn closed_resolves_when_receiver_dropped_in_worker() {
  let (tx, rx) = watch::channel(0);
  tokio::task::spawn_blocking(move || drop(rx));
  tx.closed().await;
}

#[wasm_bindgen_test]
async fn send_from_a_web_worker() {
  let (tx, mut rx) = watch::channel(0);
  tokio::task::spawn_blocking(move || tx.send(1).unwrap());
  rx.changed().await.unwrap();
  assert_eq!(*rx.borrow_and_update(), 1);
}

#[wasm_bindgen_test]
async fn sender_drop_in_worker_closes_receiver() {
  let (tx, mut rx) = watch::channel(0);
  tokio::task::spawn_blocking(move || drop(tx));
  rx.changed().await.unwrap_err();
}
