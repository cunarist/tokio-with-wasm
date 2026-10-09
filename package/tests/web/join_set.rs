use std::cell::{Cell, RefCell};
use std::rc::Rc;
use tokio_with_wasm::alias as tokio;
use tokio_with_wasm::task::{JoinError, JoinSet};
use tokio_with_wasm::time::{Duration, sleep, timeout};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn join_next_returns_every_output() -> Result<(), JoinError> {
  let mut set = JoinSet::new();
  for i in 0..10 {
    set.spawn(async move { i });
  }
  assert_eq!(set.len(), 10);
  assert!(!set.is_empty());

  let mut seen = [false; 10];
  while let Some(result) = set.join_next().await {
    seen[result?] = true;
  }
  assert!(seen.iter().all(|b| *b));
  assert!(set.is_empty());
  Ok(())
}

#[wasm_bindgen_test]
async fn spawn_local_behaves_like_spawn() -> Result<(), JoinError> {
  let mut set = JoinSet::new();
  set.spawn_local(async { 1 });
  assert_eq!(set.join_next().await.transpose()?, Some(1));
  assert!(set.is_empty());
  Ok(())
}

/// Unlike in `tokio`, which panics, failed tasks are left out.
#[wasm_bindgen_test]
async fn join_all_skips_failed_tasks() {
  let mut set = JoinSet::new();
  set.spawn(async { 1 });
  set.spawn(std::future::pending()).abort();
  set.spawn(async { 2 });
  let mut output = set.join_all().await;
  output.sort();
  assert_eq!(output, vec![1, 2]);
}

#[wasm_bindgen_test]
async fn join_next_on_an_empty_set_is_none() {
  let mut set: JoinSet<()> = JoinSet::new();
  assert!(set.join_next().await.is_none());
}

#[wasm_bindgen_test]
async fn batched_completions_arrive_in_completion_order()
-> Result<(), JoinError> {
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

/// Regression test: `try_join_next` used to poll pending tasks with a
/// no-op waker, which replaced the real waker registered by a concurrent
/// `join_next`. The completion then woke nobody and `join_next` hung.
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
  assert_eq!(waited.unwrap().transpose()?, Some(5));
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
  // Without the abort, the drain would wait out the sleeps.
  let drained = timeout(Duration::from_secs(2), set.shutdown()).await;
  assert!(drained.is_ok() && set.is_empty());
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
  let mut set = JoinSet::new();
  set.spawn(async move {
    sleep(Duration::from_millis(100)).await;
    cloned.set(true);
  });
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

#[wasm_bindgen_test]
async fn join_next_with_id_pairs_outputs_with_task_ids() -> Result<(), JoinError>
{
  let mut set = JoinSet::new();
  let mut expected = std::collections::HashMap::new();
  for i in 0..5 {
    let abort_handle = set.spawn(async move { i });
    expected.insert(abort_handle.id(), i);
  }

  let mut joined = 0;
  while let Some(result) = set.join_next_with_id().await {
    let (task_id, output) = result?;
    assert_eq!(expected.get(&task_id), Some(&output));
    joined += 1;
  }
  assert_eq!(joined, 5);
  Ok(())
}

#[wasm_bindgen_test]
async fn try_join_next_with_id_sees_only_finished_tasks()
-> Result<(), JoinError> {
  let mut set = JoinSet::new();
  let abort_handle = set.spawn(async {
    sleep(Duration::from_millis(100)).await;
    5
  });
  let task_id = abort_handle.id();
  // Nothing has finished yet.
  assert!(set.try_join_next_with_id().is_none());
  sleep(Duration::from_millis(200)).await;
  assert_eq!(set.try_join_next_with_id().transpose()?, Some((task_id, 5)));
  assert!(set.try_join_next_with_id().is_none());
  Ok(())
}

#[wasm_bindgen_test]
async fn join_next_with_id_reports_the_aborted_task() {
  let mut set = JoinSet::new();
  let abort_handle = set.spawn(async {
    sleep(Duration::from_secs(10)).await;
  });
  let task_id = abort_handle.id();
  abort_handle.abort();

  let error = set.join_next_with_id().await.unwrap().unwrap_err();
  assert!(error.is_cancelled());
  assert_eq!(error.id(), task_id);
}

#[wasm_bindgen_test]
async fn blocking_tasks_also_carry_ids() -> Result<(), JoinError> {
  let mut set = JoinSet::new();
  let task_id = set.spawn_blocking(|| 7).id();
  assert_eq!(set.join_next_with_id().await.unwrap()?, (task_id, 7));
  Ok(())
}
