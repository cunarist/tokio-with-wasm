use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::task::{Context, Poll, Wake, Waker};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::{TryRecvError, TrySendError};
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

struct Flag(AtomicBool);

impl Wake for Flag {
  fn wake(self: Arc<Self>) {
    self.0.store(true, SeqCst);
  }
}

struct Task<F> {
  fut: Pin<Box<F>>,
  flag: Arc<Flag>,
}

impl<F: Future> Task<F> {
  fn new(fut: F) -> Self {
    let flag = Arc::new(Flag(AtomicBool::new(false)));
    Self {
      fut: Box::pin(fut),
      flag,
    }
  }

  fn poll(&mut self) -> Poll<F::Output> {
    self.flag.0.store(false, SeqCst);
    let waker = Waker::from(self.flag.clone());
    self.fut.as_mut().poll(&mut Context::from_waker(&waker))
  }

  fn pending(&mut self) {
    assert!(self.poll().is_pending());
  }

  fn ready(&mut self) -> F::Output {
    match self.poll() {
      Poll::Ready(value) => value,
      Poll::Pending => panic!("future is pending"),
    }
  }

  fn is_woken(&self) -> bool {
    self.flag.0.load(SeqCst)
  }
}

struct NoImpls;

// Bounded channels lock a `Mutex` on `recv`, which traps on the main thread
// when a worker holds it.
#[wasm_bindgen_test]
async fn send_from_a_web_worker() {
  let (tx, mut rx) = mpsc::unbounded_channel();
  tokio::task::spawn_blocking(move || {
    for i in 0..3 {
      tx.send(i).unwrap();
    }
  });
  for i in 0..3 {
    assert_eq!(rx.recv().await, Some(i));
  }
  assert_eq!(rx.recv().await, None);
}

#[wasm_bindgen_test]
async fn blocking_send_from_a_web_worker() {
  let (tx, mut rx) = mpsc::channel::<u8>(1);
  let worker = tokio::task::spawn_blocking(move || {
    tx.blocking_send(1).unwrap();
    tx.blocking_send(2).unwrap();
  });
  assert_eq!(rx.recv().await, Some(1));
  assert_eq!(rx.recv().await, Some(2));
  worker.await.unwrap();
  assert_eq!(rx.recv().await, None);
}

#[wasm_bindgen_test]
async fn blocking_recv_in_a_web_worker() {
  let (tx, mut rx) = mpsc::unbounded_channel::<u8>();
  let worker = tokio::task::spawn_blocking(move || {
    assert_eq!(rx.blocking_recv(), Some(10));
    assert_eq!(rx.blocking_recv(), None);
  });
  tx.send(10).unwrap();
  drop(tx);
  worker.await.unwrap();
}

#[wasm_bindgen_test]
async fn send_recv_with_buffer() {
  let (tx, mut rx) = mpsc::channel::<i32>(16);
  tx.reserve().await.unwrap().send(1);
  tx.try_send(2).unwrap();
  drop(tx);
  assert_eq!(rx.recv().await, Some(1));
  assert_eq!(rx.recv().await, Some(2));
  assert_eq!(rx.recv().await, None);
}

#[wasm_bindgen_test]
async fn reserve_disarm() {
  let (tx, mut rx) = mpsc::channel::<i32>(2);
  let (tx1, tx2, tx3, tx4) = (tx.clone(), tx.clone(), tx.clone(), tx);

  let permit1 = tx1.reserve().await.unwrap();
  let permit2 = tx2.reserve().await.unwrap();

  let mut r3 = Task::new(tx3.reserve());
  r3.pending();
  let mut r4 = Task::new(tx4.reserve());
  r4.pending();

  permit1.send(1);
  assert!(!r3.is_woken());
  rx.recv().await.unwrap();
  assert!(r3.is_woken());
  assert!(!r4.is_woken());

  drop(permit2);
  assert!(r4.is_woken());

  Task::new(tx1.reserve()).pending();
}

#[wasm_bindgen_test]
async fn async_send_recv_with_buffer() {
  let (tx, mut rx) = mpsc::channel(16);
  tokio::spawn(async move {
    tx.send(1).await.unwrap();
    tx.send(2).await.unwrap();
  });
  assert_eq!(rx.recv().await, Some(1));
  assert_eq!(rx.recv().await, Some(2));
  assert_eq!(rx.recv().await, None);
}

#[wasm_bindgen_test]
async fn async_send_recv_many_with_buffer() {
  let (tx, mut rx) = mpsc::channel(2);
  let mut buffer = Vec::<i32>::with_capacity(3);

  assert_eq!(rx.recv_many(&mut buffer, 0).await, 0);

  let handle = tokio::spawn(async move {
    for i in [1, 2, 7, 0] {
      tx.send(i).await.unwrap();
    }
  });

  let mut count = 0;
  while count < 4 {
    count += rx.recv_many(&mut buffer, 3).await;
    assert_eq!(buffer.len(), count);
  }

  assert_eq!(buffer, [1, 2, 7, 0]);
  assert_eq!(rx.recv_many(&mut buffer, 3).await, 0);
  handle.await.unwrap();
}

#[wasm_bindgen_test]
async fn start_send_past_cap() {
  let (tx1, mut rx) = mpsc::channel(1);
  let tx2 = tx1.clone();
  tx1.try_send(()).unwrap();

  let mut r1 = Task::new(tx1.reserve());
  r1.pending();

  {
    let mut r2 = Task::new(tx2.reserve());
    r2.pending();

    drop(r1);
    assert!(rx.recv().await.is_some());

    assert!(r2.is_woken());
  }

  drop(tx1);
  drop(tx2);
  assert!(rx.recv().await.is_none());
}

#[wasm_bindgen_test]
async fn send_recv_unbounded() {
  let (tx, mut rx) = mpsc::unbounded_channel::<i32>();
  tx.send(1).unwrap();
  tx.send(2).unwrap();
  assert_eq!(rx.recv().await, Some(1));
  assert_eq!(rx.recv().await, Some(2));
  drop(tx);
  assert!(rx.recv().await.is_none());
}

#[wasm_bindgen_test]
async fn send_recv_many_unbounded() {
  let (tx, mut rx) = mpsc::unbounded_channel::<i32>();
  let mut buffer = Vec::new();

  rx.recv_many(&mut buffer, 0).await;
  assert_eq!(buffer.len(), 0);

  for i in [7, 13, 100, 1002] {
    tx.send(i).unwrap();
  }
  rx.recv_many(&mut buffer, 0).await;
  assert_eq!(buffer.len(), 0);

  let mut count = 0;
  while count < 4 {
    count += rx.recv_many(&mut buffer, 1).await;
  }
  assert_eq!(buffer, [7, 13, 100, 1002]);
  let capacity = buffer.capacity();
  assert!(capacity > 0);

  buffer.clear();
  for i in [5, 6, 7, 2] {
    tx.send(i).unwrap();
  }
  assert_eq!(rx.recv_many(&mut buffer, 32).await, 4);
  assert_eq!(buffer.capacity(), capacity);
  assert_eq!(buffer, [5, 6, 7, 2]);

  drop(tx);
  assert_eq!(rx.recv_many(&mut buffer, 4).await, 0);
  assert!(rx.recv().await.is_none());
}

#[wasm_bindgen_test]
async fn send_recv_many_bounded_capacity() {
  let mut buffer: Vec<String> = Vec::with_capacity(9);
  let limit = buffer.capacity();
  let (tx, mut rx) = mpsc::channel(100);

  let mut expected: Vec<String> = (0..limit).map(|x| x.to_string()).collect();
  for x in expected.clone() {
    tx.send(x).await.unwrap();
  }
  tx.send("one more".to_string()).await.unwrap();

  assert_eq!(rx.recv_many(&mut buffer, limit).await, limit);
  assert_eq!(buffer, expected);
  assert_eq!(buffer.capacity(), limit);

  assert_eq!(rx.recv_many(&mut buffer, limit).await, 1);
  assert!(buffer.capacity() > limit);
  expected.push("one more".to_string());
  assert_eq!(buffer, expected);

  tokio::spawn(async move {
    tx.send("final".to_string()).await.unwrap();
  });

  assert_eq!(rx.recv_many(&mut buffer, limit).await, 1);
  expected.push("final".to_string());
  assert_eq!(buffer, expected);
  assert_eq!(rx.recv_many(&mut buffer, limit).await, 0);
  assert_eq!(buffer, expected);
}

#[wasm_bindgen_test]
async fn send_recv_many_unbounded_capacity() {
  let mut buffer: Vec<String> = Vec::with_capacity(9);
  let limit = buffer.capacity();
  let (tx, mut rx) = mpsc::unbounded_channel();

  let mut expected: Vec<String> = (0..limit).map(|x| x.to_string()).collect();
  for x in expected.clone() {
    tx.send(x).unwrap();
  }
  tx.send("one more".to_string()).unwrap();

  assert_eq!(rx.recv_many(&mut buffer, limit).await, limit);
  assert_eq!(buffer, expected);
  assert_eq!(buffer.capacity(), limit);

  assert_eq!(rx.recv_many(&mut buffer, limit).await, 1);
  assert!(buffer.capacity() > limit);
  expected.push("one more".to_string());
  assert_eq!(buffer, expected);

  tokio::spawn(async move {
    tx.send("final".to_string()).unwrap();
  });

  assert_eq!(rx.recv_many(&mut buffer, limit).await, 1);
  expected.push("final".to_string());
  assert_eq!(buffer, expected);
  assert_eq!(rx.recv_many(&mut buffer, limit).await, 0);
  assert_eq!(buffer, expected);
}

#[wasm_bindgen_test]
async fn recv_many_with_non_empty_buffer_bounded_rx_closed_and_idle() {
  let (_tx, mut rx) = mpsc::channel::<i32>(1);
  let mut buffer = vec![1];
  rx.close();
  assert_eq!(rx.recv_many(&mut buffer, 1).await, 0);
  assert_eq!(buffer, [1]);
}

#[wasm_bindgen_test]
async fn recv_many_with_non_empty_buffer_unbounded_rx_closed_and_idle() {
  let (_tx, mut rx) = mpsc::unbounded_channel::<i32>();
  let mut buffer = vec![1];
  rx.close();
  assert_eq!(rx.recv_many(&mut buffer, 1).await, 0);
  assert_eq!(buffer, [1]);
}

#[wasm_bindgen_test]
async fn async_send_recv_unbounded() {
  let (tx, mut rx) = mpsc::unbounded_channel();
  tokio::spawn(async move {
    tx.send(1).unwrap();
    tx.send(2).unwrap();
  });
  assert_eq!(rx.recv().await, Some(1));
  assert_eq!(rx.recv().await, Some(2));
  assert_eq!(rx.recv().await, None);
}

#[wasm_bindgen_test]
async fn no_t_bounds_buffer() {
  let (tx, mut rx) = mpsc::channel(100);
  let _ = format!("{tx:?} {rx:?}");
  assert!(tx.clone().try_send(NoImpls).is_ok());
  assert!(rx.recv().await.is_some());
}

#[wasm_bindgen_test]
async fn no_t_bounds_unbounded() {
  let (tx, mut rx) = mpsc::unbounded_channel();
  let _ = format!("{tx:?} {rx:?}");
  assert!(tx.clone().send(NoImpls).is_ok());
  assert!(rx.recv().await.is_some());
}

#[wasm_bindgen_test]
async fn send_recv_buffer_limited() {
  let (tx, mut rx) = mpsc::channel::<i32>(1);

  tx.reserve().await.unwrap().send(1);

  let mut p2 = Task::new(tx.reserve());
  p2.pending();

  assert!(rx.recv().await.is_some());
  assert!(p2.is_woken());
  assert!(tx.try_send(1337).is_err());

  p2.ready().unwrap().send(2);
  assert!(rx.recv().await.is_some());
}

#[wasm_bindgen_test]
async fn recv_close_gets_none_idle() {
  let (tx, mut rx) = mpsc::channel::<i32>(10);
  rx.close();
  assert!(rx.recv().await.is_none());
  assert!(tx.send(1).await.is_err());
}

#[wasm_bindgen_test]
async fn recv_close_gets_none_reserved() {
  let (tx1, mut rx) = mpsc::channel::<i32>(1);
  let tx2 = tx1.clone();

  let permit1 = tx1.reserve().await.unwrap();
  let mut permit2 = Task::new(tx2.reserve());
  permit2.pending();

  rx.close();
  assert!(permit2.is_woken());
  assert!(permit2.ready().is_err());

  {
    let mut recv = Task::new(rx.recv());
    recv.pending();
    permit1.send(123);
    assert!(recv.is_woken());
    assert_eq!(recv.ready(), Some(123));
  }

  assert!(rx.recv().await.is_none());
}

#[wasm_bindgen_test]
fn failed_reserve_many_wakes_closed_receiver() {
  let (tx, mut rx) = mpsc::channel::<()>(2);
  let permit = tx.try_reserve().unwrap();

  let mut reserve = Task::new(tx.reserve_many(2));
  reserve.pending();

  rx.close();
  let mut recv = Task::new(rx.recv());
  recv.pending();

  drop(permit);
  assert!(!recv.is_woken());

  assert!(reserve.ready().is_err());
  assert!(recv.is_woken());
  recv.ready();
}

#[wasm_bindgen_test]
fn cancelled_reserve_many_wakes_closed_receiver() {
  let (tx, mut rx) = mpsc::channel::<()>(2);
  let permit = tx.try_reserve().unwrap();

  let mut reserve = Task::new(tx.reserve_many(2));
  reserve.pending();

  rx.close();
  let mut recv = Task::new(rx.recv());
  recv.pending();

  drop(permit);
  assert!(!recv.is_woken());

  drop(reserve);
  assert!(recv.is_woken());
  recv.ready();
}

#[wasm_bindgen_test]
fn failed_reserve_wakes_closed_receiver() {
  let (tx, mut rx) = mpsc::channel::<()>(1);
  let permit = tx.try_reserve().unwrap();

  let mut reserve = Task::new(tx.reserve());
  reserve.pending();

  drop(permit);
  assert!(reserve.is_woken());

  rx.close();
  let mut recv = Task::new(rx.recv());
  recv.pending();

  assert!(reserve.ready().is_err());
  assert!(recv.is_woken());
  recv.ready();
}

#[wasm_bindgen_test]
fn cancelled_reserve_wakes_closed_receiver() {
  let (tx, mut rx) = mpsc::channel::<()>(1);
  let permit = tx.try_reserve().unwrap();

  let mut reserve = Task::new(tx.reserve());
  reserve.pending();

  drop(permit);
  assert!(reserve.is_woken());

  rx.close();
  let mut recv = Task::new(rx.recv());
  recv.pending();

  drop(reserve);
  assert!(recv.is_woken());
  recv.ready();
}

#[wasm_bindgen_test]
async fn tx_close_gets_none() {
  let (_, mut rx) = mpsc::channel::<i32>(10);
  assert!(rx.recv().await.is_none());
}

#[wasm_bindgen_test]
async fn try_send_fail() {
  let (tx, mut rx) = mpsc::channel(1);
  tx.try_send("hello").unwrap();
  assert!(matches!(tx.try_send("fail"), Err(TrySendError::Full(_))));
  assert_eq!(rx.recv().await, Some("hello"));

  tx.try_send("goodbye").unwrap();
  drop(tx);
  assert_eq!(rx.recv().await, Some("goodbye"));
  assert!(rx.recv().await.is_none());
}

#[wasm_bindgen_test]
fn try_send_fail_with_try_recv() {
  let (tx, mut rx) = mpsc::channel(1);
  tx.try_send("hello").unwrap();
  assert!(matches!(tx.try_send("fail"), Err(TrySendError::Full(_))));
  assert_eq!(rx.try_recv(), Ok("hello"));

  tx.try_send("goodbye").unwrap();
  drop(tx);
  assert_eq!(rx.try_recv(), Ok("goodbye"));
  assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
}

#[wasm_bindgen_test]
async fn reserve_many_above_cap() {
  const MAX_PERMITS: usize = tokio::sync::Semaphore::MAX_PERMITS;
  let (tx, _rx) = mpsc::channel::<()>(1);
  assert!(tx.reserve_many(2).await.is_err());
  assert!(tx.reserve_many(MAX_PERMITS + 1).await.is_err());
  assert!(tx.reserve_many(usize::MAX).await.is_err());
}

#[wasm_bindgen_test]
fn try_reserve_many_zero() {
  let (tx, rx) = mpsc::channel::<()>(1);

  assert!(tx.try_reserve_many(0).unwrap().next().is_none());

  tx.try_send(()).unwrap();
  assert!(tx.try_reserve_many(0).unwrap().next().is_none());

  drop(rx);
  assert_eq!(
    tx.try_reserve_many(0).unwrap_err(),
    TrySendError::Closed(())
  );
}

#[wasm_bindgen_test]
async fn reserve_many_zero() {
  let (tx, rx) = mpsc::channel::<()>(1);

  assert!(tx.reserve_many(0).await.unwrap().next().is_none());

  tx.send(()).await.unwrap();
  assert!(tx.reserve_many(0).await.unwrap().next().is_none());

  drop(rx);
  assert!(tx.reserve_many(0).await.is_err());
}

#[wasm_bindgen_test]
async fn try_reserve_many_edge_cases() {
  const MAX_PERMITS: usize = tokio::sync::Semaphore::MAX_PERMITS;
  let (tx, rx) = mpsc::channel::<()>(1);

  assert!(tx.try_reserve_many(0).unwrap().next().is_none());
  assert!(matches!(
    tx.try_reserve_many(MAX_PERMITS + 1),
    Err(TrySendError::Full(_))
  ));
  assert!(matches!(
    tx.try_reserve_many(usize::MAX),
    Err(TrySendError::Full(_))
  ));

  drop(rx);
  assert!(tx.reserve_many(0).await.is_err());
}

#[wasm_bindgen_test]
async fn try_reserve_fails() {
  let (tx, mut rx) = mpsc::channel(1);
  let permit = tx.try_reserve().unwrap();
  assert!(matches!(tx.try_reserve(), Err(TrySendError::Full(()))));

  permit.send("foo");
  assert_eq!(rx.recv().await, Some("foo"));

  drop(tx.try_reserve().unwrap());
  let _permit = tx.try_reserve().unwrap();
}

#[wasm_bindgen_test]
async fn reserve_many_and_send() {
  let (tx, mut rx) = mpsc::channel(100);
  for i in 0..100 {
    for permit in tx.reserve_many(i).await.unwrap() {
      permit.send("foo");
      assert_eq!(rx.recv().await, Some("foo"));
    }
    assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
  }
}

#[wasm_bindgen_test]
async fn try_reserve_many_and_send() {
  let (tx, mut rx) = mpsc::channel(100);
  for i in 0..100 {
    for permit in tx.try_reserve_many(i).unwrap() {
      permit.send("foo");
      assert_eq!(rx.recv().await, Some("foo"));
    }
    assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
  }
}

#[wasm_bindgen_test]
async fn reserve_many_on_closed_channel() {
  let (tx, rx) = mpsc::channel::<()>(100);
  drop(rx);
  assert!(tx.reserve_many(10).await.is_err());
}

#[wasm_bindgen_test]
fn try_reserve_many_on_closed_channel() {
  let (tx, rx) = mpsc::channel::<usize>(100);
  drop(rx);
  assert!(matches!(
    tx.try_reserve_many(10),
    Err(TrySendError::Closed(()))
  ));
}

#[wasm_bindgen_test]
async fn try_reserve_many_full() {
  for n in 1..100 {
    for k in 0..n {
      let (tx, mut rx) = mpsc::channel::<usize>(n);
      let permits = tx.try_reserve_many(n).unwrap();

      assert_eq!(permits.len(), n);
      assert_eq!(tx.capacity(), 0);
      assert!(matches!(tx.try_reserve_many(1), Err(TrySendError::Full(_))));

      for permit in permits.take(k) {
        permit.send(0);
      }
      assert_eq!(tx.capacity(), n - k);

      tx.try_reserve_many(1).unwrap();
      assert!(matches!(
        tx.try_reserve_many(n - k + 1),
        Err(TrySendError::Full(_))
      ));

      for _ in 0..k {
        assert_eq!(rx.recv().await, Some(0));
      }
      assert_eq!(tx.capacity(), n);
    }
  }
}

#[wasm_bindgen_test]
async fn drop_permit_releases_permit() {
  let (tx1, _rx) = mpsc::channel::<i32>(1);
  let tx2 = tx1.clone();

  let permit = tx1.reserve().await.unwrap();
  let mut reserve2 = Task::new(tx2.reserve());
  reserve2.pending();

  drop(permit);
  assert!(reserve2.is_woken());
  assert!(reserve2.ready().is_ok());
}

#[wasm_bindgen_test]
async fn drop_permit_iterator_releases_permits() {
  for n in 1..100 {
    let (tx1, _rx) = mpsc::channel::<i32>(n);
    let tx2 = tx1.clone();

    let permits = tx1.reserve_many(n).await.unwrap();
    let mut reserve2 = Task::new(tx2.reserve_many(n));
    reserve2.pending();

    drop(permits);
    assert!(reserve2.is_woken());

    drop(reserve2.ready().unwrap());
    assert_eq!(tx1.capacity(), n);
  }
}

#[wasm_bindgen_test]
fn dropping_last_permit_wakes_closed_receiver() {
  let (tx, mut rx) = mpsc::channel::<()>(100);
  let permit = tx.try_reserve().unwrap();
  rx.close();

  let mut recv = Task::new(rx.recv());
  recv.pending();
  drop(permit);
  assert!(recv.is_woken());
  recv.ready();
}

#[wasm_bindgen_test]
fn dropping_last_owned_permit_wakes_closed_receiver() {
  let (tx, mut rx) = mpsc::channel::<()>(100);
  let permit = tx.try_reserve_owned().unwrap();
  rx.close();

  let mut recv = Task::new(rx.recv());
  recv.pending();
  drop(permit);
  assert!(recv.is_woken());
  recv.ready();
}

#[wasm_bindgen_test]
fn dropping_last_permit_iterator_wakes_closed_receiver() {
  let (tx, mut rx) = mpsc::channel::<()>(100);
  let permits = tx.try_reserve_many(1).unwrap();
  rx.close();

  let mut recv = Task::new(rx.recv());
  recv.pending();
  drop(permits);
  assert!(recv.is_woken());
  recv.ready();
}

#[wasm_bindgen_test]
fn sending_last_permit_wakes_closed_receiver() {
  let (tx, mut rx) = mpsc::channel::<()>(100);
  let permit = tx.try_reserve().unwrap();
  rx.close();

  let mut recv = Task::new(rx.recv());
  recv.pending();
  permit.send(());
  assert!(recv.is_woken());
  recv.ready();
}

#[wasm_bindgen_test]
fn sending_last_owned_permit_wakes_closed_receiver() {
  let (tx, mut rx) = mpsc::channel::<()>(100);
  let permit = tx.try_reserve_owned().unwrap();
  rx.close();

  let mut recv = Task::new(rx.recv());
  recv.pending();
  permit.send(());
  assert!(recv.is_woken());
  recv.ready();
}

#[wasm_bindgen_test]
fn releasing_last_owned_permit_wakes_closed_receiver() {
  let (tx, mut rx) = mpsc::channel::<()>(100);
  let permit = tx.try_reserve_owned().unwrap();
  rx.close();

  let mut recv = Task::new(rx.recv());
  recv.pending();
  let inert_sender = permit.release();
  assert!(recv.is_woken());
  recv.ready();
  drop(inert_sender);
}

#[wasm_bindgen_test]
async fn dropping_rx_closes_channel() {
  let (tx, rx) = mpsc::channel(100);
  let msg = Arc::new(());
  tx.try_send(msg.clone()).unwrap();

  drop(rx);
  assert!(tx.reserve().await.is_err());
  assert!(tx.reserve_many(10).await.is_err());
  assert_eq!(Arc::strong_count(&msg), 1);
}

#[wasm_bindgen_test]
fn dropping_rx_closes_channel_for_try() {
  let (tx, rx) = mpsc::channel(100);
  let msg = Arc::new(());
  tx.try_send(msg.clone()).unwrap();

  drop(rx);
  assert!(matches!(
    tx.try_send(msg.clone()),
    Err(TrySendError::Closed(_))
  ));
  assert!(matches!(tx.try_reserve(), Err(TrySendError::Closed(_))));
  assert!(matches!(
    tx.try_reserve_owned(),
    Err(TrySendError::Closed(_))
  ));
  assert_eq!(Arc::strong_count(&msg), 1);
}

#[wasm_bindgen_test]
fn unconsumed_messages_are_dropped() {
  let msg = Arc::new(());
  let (tx, rx) = mpsc::channel(100);
  tx.try_send(msg.clone()).unwrap();
  assert_eq!(Arc::strong_count(&msg), 2);

  drop((tx, rx));
  assert_eq!(Arc::strong_count(&msg), 1);
}

#[wasm_bindgen_test]
async fn ready_close_cancel_bounded() {
  let (tx, mut rx) = mpsc::channel::<()>(100);
  let _tx2 = tx.clone();
  let permit = tx.reserve().await.unwrap();
  rx.close();

  let mut recv = Task::new(rx.recv());
  recv.pending();
  drop(permit);
  assert!(recv.is_woken());
  assert!(recv.ready().is_none());
}

#[wasm_bindgen_test]
async fn permit_available_not_acquired_close() {
  let (tx1, mut rx) = mpsc::channel::<()>(1);
  let tx2 = tx1.clone();

  let permit1 = tx1.reserve().await.unwrap();
  let mut permit2 = Task::new(tx2.reserve());
  permit2.pending();

  rx.close();
  drop(permit1);
  assert!(permit2.is_woken());

  drop(permit2);
  assert!(rx.recv().await.is_none());
}

#[wasm_bindgen_test]
fn try_recv_bounded() {
  let (tx, mut rx) = mpsc::channel(5);

  for _ in 0..5 {
    tx.try_send("hello").unwrap();
  }
  assert!(tx.try_send("hello").is_err());
  for _ in 0..5 {
    assert_eq!(rx.try_recv(), Ok("hello"));
  }
  assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));

  for _ in 0..4 {
    tx.try_send("hello").unwrap();
  }
  assert_eq!(rx.try_recv(), Ok("hello"));
  tx.try_send("hello").unwrap();
  tx.try_send("hello").unwrap();
  assert!(tx.try_send("hello").is_err());
  for _ in 0..5 {
    assert_eq!(rx.try_recv(), Ok("hello"));
  }
  assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));

  for _ in 0..3 {
    tx.try_send("hello").unwrap();
  }
  drop(tx);
  for _ in 0..3 {
    assert_eq!(rx.try_recv(), Ok("hello"));
  }
  assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
}

#[wasm_bindgen_test]
fn try_recv_unbounded() {
  for num in 0..100 {
    let (tx, mut rx) = mpsc::unbounded_channel();
    for i in 0..num {
      tx.send(i).unwrap();
    }
    for i in 0..num {
      assert_eq!(rx.try_recv(), Ok(i));
    }
    assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
    drop(tx);
    assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
  }
}

#[wasm_bindgen_test]
fn try_recv_after_receiver_close() {
  let (_tx, mut rx) = mpsc::channel::<()>(5);
  assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
  rx.close();
  assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
}

#[wasm_bindgen_test]
fn try_recv_after_receiver_close_with_permit() {
  let (tx, mut rx) = mpsc::channel::<()>(5);
  let permit = tx.try_reserve().unwrap();

  assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
  rx.close();
  assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
  drop(permit);
  assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
}

#[wasm_bindgen_test]
fn try_recv_close_while_empty_bounded() {
  let (tx, mut rx) = mpsc::channel::<()>(5);
  assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
  drop(tx);
  assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
}

#[wasm_bindgen_test]
fn try_recv_close_while_empty_unbounded() {
  let (tx, mut rx) = mpsc::unbounded_channel::<()>();
  assert_eq!(rx.try_recv(), Err(TryRecvError::Empty));
  drop(tx);
  assert_eq!(rx.try_recv(), Err(TryRecvError::Disconnected));
}

#[wasm_bindgen_test]
async fn test_tx_capacity() {
  let (tx, _rx) = mpsc::channel::<()>(10);
  assert_eq!(tx.capacity(), 10);
  assert_eq!(tx.max_capacity(), 10);

  let _permit = tx.reserve().await.unwrap();
  assert_eq!(tx.capacity(), 9);
  assert_eq!(tx.max_capacity(), 10);

  tx.send(()).await.unwrap();
  assert_eq!(tx.capacity(), 8);
  assert_eq!(tx.max_capacity(), 10);
}

#[wasm_bindgen_test]
fn rx_is_closed_when_calling_close_with_sender() {
  let (_tx, mut rx) = mpsc::channel::<()>(10);
  rx.close();
  assert!(rx.is_closed());
}

#[wasm_bindgen_test]
async fn rx_is_closed_when_dropping_all_senders() {
  let (tx, rx) = mpsc::channel::<()>(10);
  let another_tx = tx.clone();
  let task = tokio::spawn(async move { drop(another_tx) });

  drop(tx);
  task.await.unwrap();
  assert!(rx.is_closed());
}

#[wasm_bindgen_test]
fn rx_is_not_closed_when_there_are_senders() {
  let (_tx, rx) = mpsc::channel::<()>(10);
  assert!(!rx.is_closed());
}

#[wasm_bindgen_test]
async fn rx_is_not_closed_when_there_are_senders_and_buffer_filled() {
  let (tx, rx) = mpsc::channel(10);
  for i in 0..10 {
    tx.send(i).await.unwrap();
  }
  assert!(!rx.is_closed());
}

#[wasm_bindgen_test]
async fn rx_is_closed_when_there_are_no_senders_and_there_are_messages() {
  let (tx, rx) = mpsc::channel(10);
  for i in 0..10 {
    tx.send(i).await.unwrap();
  }
  drop(tx);
  assert!(rx.is_closed());
}

#[wasm_bindgen_test]
async fn rx_is_closed_when_there_are_messages_and_close_is_called() {
  let (tx, mut rx) = mpsc::channel(10);
  for i in 0..10 {
    tx.send(i).await.unwrap();
  }
  rx.close();
  assert!(rx.is_closed());
}

#[wasm_bindgen_test]
async fn rx_is_not_closed_when_there_are_permits_but_not_senders() {
  let (tx, rx) = mpsc::channel::<()>(10);
  let _permit = tx.reserve_owned().await.unwrap();
  assert!(!rx.is_closed());
}

#[wasm_bindgen_test]
fn rx_is_empty_when_no_messages_were_sent() {
  let (_tx, rx) = mpsc::channel::<()>(10);
  assert!(rx.is_empty());
}

#[wasm_bindgen_test]
async fn rx_is_not_empty_when_there_are_messages_in_the_buffer() {
  let (tx, rx) = mpsc::channel::<()>(10);
  tx.send(()).await.unwrap();
  assert!(!rx.is_empty());
}

#[wasm_bindgen_test]
async fn rx_is_not_empty_when_the_buffer_is_full() {
  let (tx, rx) = mpsc::channel(10);
  for i in 0..10 {
    tx.send(i).await.unwrap();
  }
  assert!(!rx.is_empty());
}

#[wasm_bindgen_test]
async fn rx_is_not_empty_when_all_but_one_messages_are_consumed() {
  let (tx, mut rx) = mpsc::channel(10);
  for i in 0..10 {
    tx.send(i).await.unwrap();
  }
  for _ in 0..9 {
    assert!(rx.recv().await.is_some());
  }
  assert!(!rx.is_empty());
}

#[wasm_bindgen_test]
async fn rx_is_empty_when_all_messages_are_consumed() {
  let (tx, mut rx) = mpsc::channel(10);
  for i in 0..10 {
    tx.send(i).await.unwrap();
  }
  while rx.try_recv().is_ok() {}
  assert!(rx.is_empty());
}

#[wasm_bindgen_test]
async fn rx_is_empty_all_senders_are_dropped_and_messages_consumed() {
  let (tx, mut rx) = mpsc::channel(10);
  for i in 0..10 {
    tx.send(i).await.unwrap();
  }
  drop(tx);
  for _ in 0..10 {
    assert!(rx.recv().await.is_some());
  }
  assert!(rx.is_empty());
}

#[wasm_bindgen_test]
fn rx_len_on_empty_channel() {
  let (_tx, rx) = mpsc::channel::<()>(100);
  assert_eq!(rx.len(), 0);
}

#[wasm_bindgen_test]
fn rx_len_on_empty_channel_without_senders() {
  let (tx, rx) = mpsc::channel::<()>(100);
  drop(tx);
  assert_eq!(rx.len(), 0);
}

#[wasm_bindgen_test]
async fn rx_len_on_filled_channel() {
  let (tx, rx) = mpsc::channel(100);
  for i in 0..100 {
    tx.send(i).await.unwrap();
  }
  assert_eq!(rx.len(), 100);
}

#[wasm_bindgen_test]
async fn rx_len_on_filled_channel_without_senders() {
  let (tx, rx) = mpsc::channel(100);
  for i in 0..100 {
    tx.send(i).await.unwrap();
  }
  drop(tx);
  assert_eq!(rx.len(), 100);
}

#[wasm_bindgen_test]
async fn rx_len_when_consuming_all_messages() {
  let (tx, mut rx) = mpsc::channel(100);
  for i in 0..100 {
    tx.send(i).await.unwrap();
    assert_eq!(rx.len(), i + 1);
  }
  drop(tx);
  for i in (0..100).rev() {
    assert!(rx.recv().await.is_some());
    assert_eq!(rx.len(), i);
  }
}

#[wasm_bindgen_test]
async fn rx_len_when_close_is_called() {
  let (tx, mut rx) = mpsc::channel(100);
  tx.send(()).await.unwrap();
  rx.close();
  assert_eq!(rx.len(), 1);
}

#[wasm_bindgen_test]
async fn rx_len_when_close_is_called_before_dropping_sender() {
  let (tx, mut rx) = mpsc::channel(100);
  tx.send(()).await.unwrap();
  rx.close();
  drop(tx);
  assert_eq!(rx.len(), 1);
}

#[wasm_bindgen_test]
async fn rx_len_when_close_is_called_after_dropping_sender() {
  let (tx, mut rx) = mpsc::channel(100);
  tx.send(()).await.unwrap();
  drop(tx);
  rx.close();
  assert_eq!(rx.len(), 1);
}

#[wasm_bindgen_test]
fn rx_unbounded_is_closed_when_calling_close_with_sender() {
  let (_tx, mut rx) = mpsc::unbounded_channel::<()>();
  rx.close();
  assert!(rx.is_closed());
}

#[wasm_bindgen_test]
async fn rx_unbounded_is_closed_when_dropping_all_senders() {
  let (tx, rx) = mpsc::unbounded_channel::<()>();
  let another_tx = tx.clone();
  let task = tokio::spawn(async move { drop(another_tx) });

  drop(tx);
  task.await.unwrap();
  assert!(rx.is_closed());
}

#[wasm_bindgen_test]
fn rx_unbounded_is_not_closed_when_there_are_senders() {
  let (_tx, rx) = mpsc::unbounded_channel::<()>();
  assert!(!rx.is_closed());
}

#[wasm_bindgen_test]
fn rx_unbounded_is_closed_when_there_are_no_senders_and_there_are_messages() {
  let (tx, rx) = mpsc::unbounded_channel();
  for i in 0..10 {
    tx.send(i).unwrap();
  }
  drop(tx);
  assert!(rx.is_closed());
}

#[wasm_bindgen_test]
fn rx_unbounded_is_closed_when_there_are_messages_and_close_is_called() {
  let (tx, mut rx) = mpsc::unbounded_channel();
  for i in 0..10 {
    tx.send(i).unwrap();
  }
  rx.close();
  assert!(rx.is_closed());
}

#[wasm_bindgen_test]
fn rx_unbounded_is_empty_when_no_messages_were_sent() {
  let (_tx, rx) = mpsc::unbounded_channel::<()>();
  assert!(rx.is_empty());
}

#[wasm_bindgen_test]
fn rx_unbounded_is_not_empty_when_there_are_messages_in_the_buffer() {
  let (tx, rx) = mpsc::unbounded_channel();
  tx.send(()).unwrap();
  assert!(!rx.is_empty());
}

#[wasm_bindgen_test]
async fn rx_unbounded_is_not_empty_when_all_but_one_messages_are_consumed() {
  let (tx, mut rx) = mpsc::unbounded_channel();
  for i in 0..10 {
    tx.send(i).unwrap();
  }
  for _ in 0..9 {
    assert!(rx.recv().await.is_some());
  }
  assert!(!rx.is_empty());
}

#[wasm_bindgen_test]
fn rx_unbounded_is_empty_when_all_messages_are_consumed() {
  let (tx, mut rx) = mpsc::unbounded_channel();
  for i in 0..10 {
    tx.send(i).unwrap();
  }
  while rx.try_recv().is_ok() {}
  assert!(rx.is_empty());
}

#[wasm_bindgen_test]
async fn rx_unbounded_is_empty_all_senders_are_dropped_and_messages_consumed() {
  let (tx, mut rx) = mpsc::unbounded_channel();
  for i in 0..10 {
    tx.send(i).unwrap();
  }
  drop(tx);
  for _ in 0..10 {
    assert!(rx.recv().await.is_some());
  }
  assert!(rx.is_empty());
}

#[wasm_bindgen_test]
fn rx_unbounded_len_on_empty_channel() {
  let (_tx, rx) = mpsc::unbounded_channel::<()>();
  assert_eq!(rx.len(), 0);
}

#[wasm_bindgen_test]
fn rx_unbounded_len_on_empty_channel_without_senders() {
  let (tx, rx) = mpsc::unbounded_channel::<()>();
  drop(tx);
  assert_eq!(rx.len(), 0);
}

#[wasm_bindgen_test]
fn rx_unbounded_len_with_multiple_messages() {
  let (tx, rx) = mpsc::unbounded_channel();
  for i in 0..100 {
    tx.send(i).unwrap();
  }
  assert_eq!(rx.len(), 100);
}

#[wasm_bindgen_test]
fn rx_unbounded_len_with_multiple_messages_and_dropped_senders() {
  let (tx, rx) = mpsc::unbounded_channel();
  for i in 0..100 {
    tx.send(i).unwrap();
  }
  drop(tx);
  assert_eq!(rx.len(), 100);
}

#[wasm_bindgen_test]
async fn rx_unbounded_len_when_consuming_all_messages() {
  let (tx, mut rx) = mpsc::unbounded_channel();
  for i in 0..100 {
    tx.send(i).unwrap();
    assert_eq!(rx.len(), i + 1);
  }
  drop(tx);
  for i in (0..100).rev() {
    assert!(rx.recv().await.is_some());
    assert_eq!(rx.len(), i);
  }
}

#[wasm_bindgen_test]
fn rx_unbounded_len_when_close_is_called() {
  let (tx, mut rx) = mpsc::unbounded_channel();
  tx.send(()).unwrap();
  rx.close();
  assert_eq!(rx.len(), 1);
}

#[wasm_bindgen_test]
fn rx_unbounded_len_when_close_is_called_before_dropping_sender() {
  let (tx, mut rx) = mpsc::unbounded_channel();
  tx.send(()).unwrap();
  rx.close();
  drop(tx);
  assert_eq!(rx.len(), 1);
}

#[wasm_bindgen_test]
fn rx_unbounded_len_when_close_is_called_after_dropping_sender() {
  let (tx, mut rx) = mpsc::unbounded_channel();
  tx.send(()).unwrap();
  drop(tx);
  rx.close();
  assert_eq!(rx.len(), 1);
}

// Regression test for https://github.com/tokio-rs/tokio/issues/6602
#[wasm_bindgen_test]
async fn is_empty_32_msgs() {
  let (sender, mut receiver) = mpsc::channel(33);
  for value in 1..257 {
    sender.send(value).await.unwrap();
    receiver.recv().await.unwrap();
    assert!(receiver.is_empty(), "{value}. len: {}", receiver.len());
  }
}

#[wasm_bindgen_test]
fn release_waker_on_rx_drop() {
  struct DummyWaker;
  impl Wake for DummyWaker {
    fn wake(self: Arc<Self>) {}
  }

  let dummy = Arc::new(DummyWaker);
  let waker = Waker::from(dummy.clone());
  let mut cx = Context::from_waker(&waker);
  assert_eq!(Arc::strong_count(&dummy), 2);

  let (_tx, mut rx) = mpsc::channel::<()>(1);
  assert!(rx.poll_recv(&mut cx).is_pending());
  assert_eq!(Arc::strong_count(&dummy), 3);

  drop(rx);
  assert_eq!(Arc::strong_count(&dummy), 2);
}
