//! Browser tests for `JoinSet`.
//! Run with `wasm-pack test --headless --chrome package`.

// The glue code only exists on the web target,
// so this file is empty everywhere else.
#![cfg(all(
  target_arch = "wasm32",
  target_vendor = "unknown",
  target_os = "unknown"
))]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use tokio_with_wasm::alias as tokio;
use tokio_with_wasm::task::{JoinError, JoinSet};
use tokio_with_wasm::time::{Duration, sleep, timeout};
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn join_next_returns_every_output() -> Result<(), JoinError> {
  let mut set = JoinSet::new();
  for i in 0..10 {
    set.spawn(async move { i });
  }
  assert_eq!(set.len(), 10);

  let mut seen = [false; 10];
  while let Some(result) = set.join_next().await {
    seen[result?] = true;
  }
  assert!(seen.iter().all(|b| *b));
  assert!(set.is_empty());
  Ok(())
}

#[wasm_bindgen_test]
async fn join_next_yields_in_completion_order() -> Result<(), JoinError> {
  let mut set = JoinSet::new();
  set.spawn(async {
    sleep(Duration::from_millis(350)).await;
    1
  });
  set.spawn(async {
    sleep(Duration::from_millis(50)).await;
    2
  });
  set.spawn(async {
    sleep(Duration::from_millis(200)).await;
    3
  });

  let mut order = Vec::new();
  while let Some(result) = set.join_next().await {
    order.push(result?);
  }
  assert_eq!(order, vec![2, 3, 1]);
  Ok(())
}

#[wasm_bindgen_test]
async fn join_all_collects_everything() {
  let mut set = JoinSet::new();
  for i in 0..5 {
    set.spawn(async move { i });
  }
  let mut output = set.join_all().await;
  output.sort();
  assert_eq!(output, vec![0, 1, 2, 3, 4]);
}

#[wasm_bindgen_test]
async fn join_next_on_an_empty_set_is_none() {
  let mut set: JoinSet<()> = JoinSet::new();
  assert!(set.join_next().await.is_none());
}

#[wasm_bindgen_test]
async fn try_join_next_sees_only_finished_tasks() -> Result<(), JoinError> {
  let mut set = JoinSet::new();
  set.spawn(async {
    sleep(Duration::from_millis(100)).await;
    5
  });
  // Nothing has finished yet.
  assert!(set.try_join_next().is_none());
  sleep(Duration::from_millis(200)).await;
  assert_eq!(set.try_join_next().transpose()?, Some(5));
  assert!(set.try_join_next().is_none());
  Ok(())
}

/// Regression test: `try_join_next` used to poll pending tasks with a
/// no-op waker, which replaced the real waker registered by a concurrent
/// `join_next`. The completion then woke nobody and `join_next` hung.
#[wasm_bindgen_test]
async fn batched_completions_arrive_in_completion_order() -> Result<(), JoinError> {
  let mut set = JoinSet::new();
  set.spawn(async {
    sleep(Duration::from_millis(300)).await;
    "slow"
  });
  set.spawn(async {
    sleep(Duration::from_millis(100)).await;
    "fast"
  });
  set.spawn(async {
    sleep(Duration::from_millis(200)).await;
    "middle"
  });
  // Let every task finish before the first `join_next` poll,
  // so the order must come from completion times, not from polling.
  sleep(Duration::from_millis(400)).await;

  let mut order = Vec::new();
  while let Some(result) = set.join_next().await {
    order.push(result?);
  }
  assert_eq!(order, vec!["fast", "middle", "slow"]);
  Ok(())
}

#[wasm_bindgen_test]
async fn try_join_next_does_not_silence_join_next() -> Result<(), JoinError> {
  let set = Rc::new(RefCell::new(JoinSet::new()));
  set.borrow_mut().spawn(async {
    sleep(Duration::from_millis(150)).await;
    5
  });

  // While the test below is awaiting `join_next`,
  // this task pokes the set with `try_join_next`.
  let poker = set.clone();
  tokio::spawn(async move {
    sleep(Duration::from_millis(50)).await;
    assert!(poker.borrow_mut().try_join_next().is_none());
  });

  let start = js_sys::Date::now();
  let waited = timeout(
    Duration::from_secs(5),
    std::future::poll_fn(|cx| set.borrow_mut().poll_join_next(cx)),
  )
  .await;
  let Ok(result) = waited else {
    panic!("`join_next` missed the completion");
  };
  assert_eq!(result.transpose()?, Some(5));
  // With the clobbered waker, nothing re-polls `join_next` until the
  // timeout above fires at five seconds, so completion must come from
  // the task's own wake at 150ms to prove the waker survived.
  let elapsed = js_sys::Date::now() - start;
  assert!(
    elapsed < 2500.0,
    "the completion was not delivered: {elapsed}ms"
  );
  Ok(())
}

#[wasm_bindgen_test]
async fn shutdown_aborts_and_drains() {
  let mut set = JoinSet::new();
  for _ in 0..3 {
    set.spawn(async {
      sleep(Duration::from_secs(10)).await;
    });
  }
  set.shutdown().await;
  assert!(set.is_empty());
}

#[wasm_bindgen_test]
async fn abort_all_cancels_pending_tasks() {
  let mut set = JoinSet::new();
  for _ in 0..3 {
    set.spawn(async {
      sleep(Duration::from_secs(10)).await;
    });
  }
  set.abort_all();
  let mut cancelled = 0;
  while let Some(result) = set.join_next().await {
    assert!(result.is_err_and(|error| error.is_cancelled()));
    cancelled += 1;
  }
  assert_eq!(cancelled, 3);
}

#[wasm_bindgen_test]
async fn dropping_the_set_aborts_its_tasks() {
  let flag = Rc::new(Cell::new(false));
  let cloned = flag.clone();
  let set = {
    let mut set = JoinSet::new();
    set.spawn(async move {
      sleep(Duration::from_millis(100)).await;
      cloned.set(true);
    });
    set
  };
  drop(set);
  sleep(Duration::from_millis(300)).await;
  assert!(!flag.get(), "the task outlived the dropped `JoinSet`");
}

#[wasm_bindgen_test]
async fn detach_all_keeps_tasks_running() {
  let flag = Rc::new(Cell::new(false));
  let cloned = flag.clone();
  let mut set = JoinSet::new();
  set.spawn(async move {
    sleep(Duration::from_millis(100)).await;
    cloned.set(true);
  });
  set.detach_all();
  drop(set);
  sleep(Duration::from_millis(300)).await;
  assert!(flag.get(), "the detached task was aborted");
}
