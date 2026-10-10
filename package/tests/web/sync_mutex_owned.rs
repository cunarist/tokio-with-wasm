use std::future::Future;
use std::pin::pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[derive(Default)]
struct Wakes(AtomicUsize);

impl Wake for Wakes {
  fn wake(self: Arc<Self>) {
    self.0.fetch_add(1, Ordering::SeqCst);
  }
}

#[wasm_bindgen_test]
fn straight_execution() {
  let l = Arc::new(Mutex::new(100));
  for expected in [100, 99, 98] {
    let mut g = l.clone().try_lock_owned().unwrap();
    assert_eq!(*g, expected);
    *g -= 1;
  }
}

#[wasm_bindgen_test]
fn readiness() {
  let l = Arc::new(Mutex::new(100));
  let wakes = Arc::new(Wakes::default());
  let waker = Waker::from(wakes.clone());
  let mut cx = Context::from_waker(&waker);

  let mut t1 = pin!(l.clone().lock_owned());
  let mut t2 = pin!(l.lock_owned());
  let Poll::Ready(g) = t1.as_mut().poll(&mut cx) else {
    panic!("first lock is pending");
  };
  assert!(t2.as_mut().poll(&mut cx).is_pending());

  drop(g);
  assert_eq!(wakes.0.load(Ordering::SeqCst), 1);
  assert!(t2.as_mut().poll(&mut cx).is_ready());
}

#[wasm_bindgen_test]
async fn aborted_future_1() {
  let m1 = Arc::new(Mutex::new(0usize));
  let m2 = m1.clone();
  tokio::time::timeout(Duration::from_millis(1), async move {
    let _g = m2.lock_owned().await;
    std::future::pending::<()>().await;
  })
  .await
  .unwrap_err();
  tokio::time::timeout(Duration::from_millis(1000), m1.lock_owned())
    .await
    .expect("Mutex is locked");
}

#[wasm_bindgen_test]
async fn aborted_future_2() {
  let m1 = Arc::new(Mutex::new(0usize));
  {
    let _lock = m1.clone().lock_owned().await;
    let m2 = m1.clone();
    tokio::time::timeout(Duration::from_millis(1), async move {
      let _g = m2.lock_owned().await;
    })
    .await
    .unwrap_err();
  }
  tokio::time::timeout(Duration::from_millis(1000), m1.lock_owned())
    .await
    .expect("Mutex is locked");
}

#[wasm_bindgen_test]
fn try_lock_owned() {
  let m = Arc::new(Mutex::new(0usize));
  {
    let _g = m.clone().try_lock_owned().unwrap();
    assert!(m.clone().try_lock_owned().is_err());
  }
  assert!(m.try_lock_owned().is_ok());
}

#[wasm_bindgen_test]
async fn debug_format() {
  let s = "debug";
  let m = Arc::new(Mutex::new(s.to_string()));
  assert_eq!(format!("{s:?}"), format!("{:?}", m.lock_owned().await));
}

#[wasm_bindgen_test]
async fn guard_moves_to_a_web_worker() {
  let m = Arc::new(Mutex::new(0));
  let mut g = m.clone().lock_owned().await;
  let task = tokio::task::spawn_blocking(move || {
    std::thread::sleep(Duration::from_millis(50));
    *g = 5;
  });
  assert!(m.clone().try_lock_owned().is_err());
  assert_eq!(*m.lock_owned().await, 5);
  task.await.unwrap();
}
