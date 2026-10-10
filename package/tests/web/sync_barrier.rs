use crate::support::{assert_pending, assert_ready, spawn};
use tokio::sync::Barrier;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

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
    let mut w = spawn(b.wait());
    assert!(assert_ready!(w.poll()).is_leader());
  }
}

#[wasm_bindgen_test]
fn single() {
  let b = Barrier::new(1);
  for _ in 0..3 {
    let mut w = spawn(b.wait());
    assert!(assert_ready!(w.poll()).is_leader());
  }
}

#[wasm_bindgen_test]
fn tango() {
  let b = Barrier::new(2);

  let mut w1 = spawn(b.wait());
  assert_pending!(w1.poll());

  let mut w2 = spawn(b.wait());
  let wr2 = assert_ready!(w2.poll());
  let wr1 = assert_ready!(w1.poll());

  assert!(wr1.is_leader() != wr2.is_leader());
}

#[wasm_bindgen_test]
fn lots() {
  let b = Barrier::new(100);

  for _ in 0..10 {
    let mut wait = Vec::new();
    for _ in 0..99 {
      let mut w = spawn(b.wait());
      assert_pending!(w.poll());
      wait.push(w);
    }
    for w in &mut wait {
      assert_pending!(w.poll());
    }

    let mut w = spawn(b.wait());
    let mut found_leader = assert_ready!(w.poll()).is_leader();
    for mut w in wait {
      if assert_ready!(w.poll()).is_leader() {
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
