use std::future::Future;
use std::pin::{pin, Pin};
use std::task::{Context, Poll, Waker};
use tokio::sync::Barrier;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

fn poll<F: Future>(f: Pin<&mut F>) -> Poll<F::Output> {
  f.poll(&mut Context::from_waker(Waker::noop()))
}

macro_rules! assert_ready {
  ($e:expr) => {
    match $e {
      Poll::Ready(v) => v,
      Poll::Pending => panic!("expected ready"),
    }
  };
}

#[wasm_bindgen_test]
fn barrier_future_is_send() {
  fn is_send<T: Send>(_: T) {}
  let b = Barrier::new(0);
  is_send(b.wait());
}

#[wasm_bindgen_test]
fn zero_does_not_block() {
  let b = Barrier::new(0);
  for _ in 0..2 {
    let mut w = pin!(b.wait());
    assert!(assert_ready!(poll(w.as_mut())).is_leader());
  }
}

#[wasm_bindgen_test]
fn single() {
  let b = Barrier::new(1);
  for _ in 0..3 {
    let mut w = pin!(b.wait());
    assert!(assert_ready!(poll(w.as_mut())).is_leader());
  }
}

#[wasm_bindgen_test]
fn tango() {
  let b = Barrier::new(2);

  let mut w1 = pin!(b.wait());
  assert!(poll(w1.as_mut()).is_pending());

  let mut w2 = pin!(b.wait());
  let wr2 = assert_ready!(poll(w2.as_mut()));
  let wr1 = assert_ready!(poll(w1.as_mut()));

  assert!(wr1.is_leader() != wr2.is_leader());
}

#[wasm_bindgen_test]
fn lots() {
  let b = Barrier::new(100);

  for _ in 0..10 {
    let mut wait = Vec::new();
    for _ in 0..99 {
      let mut w = Box::pin(b.wait());
      assert!(poll(w.as_mut()).is_pending());
      wait.push(w);
    }
    for w in &mut wait {
      assert!(poll(w.as_mut()).is_pending());
    }

    let mut w = pin!(b.wait());
    let mut found_leader = assert_ready!(poll(w.as_mut())).is_leader();
    for mut w in wait {
      if assert_ready!(poll(w.as_mut())).is_leader() {
        assert!(!found_leader);
        found_leader = true;
      }
    }
    assert!(found_leader);
  }
}

#[wasm_bindgen_test]
async fn tasks_meet_at_the_barrier() {
  let b = std::sync::Arc::new(Barrier::new(3));
  let handles: Vec<_> = (0..3)
    .map(|_| {
      let b = b.clone();
      tokio::spawn(async move { b.wait().await.is_leader() })
    })
    .collect();

  let mut leaders = 0;
  for h in handles {
    leaders += h.await.unwrap() as u32;
  }
  assert_eq!(leaders, 1);
}

#[wasm_bindgen_test]
async fn barrier_reused_by_tasks() {
  let b = std::sync::Arc::new(Barrier::new(2));
  let other = b.clone();
  let h = tokio::spawn(async move {
    other.wait().await;
    other.wait().await;
  });
  b.wait().await;
  b.wait().await;
  h.await.unwrap();
}
