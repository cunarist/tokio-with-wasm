use std::cell::{Cell, RefCell};
use std::rc::Rc;
use tokio_with_wasm::alias as tokio;
use tokio_with_wasm::task::coop::{consume_budget, unconstrained};
use tokio_with_wasm::task::{
  JoinError, LocalKey, spawn, spawn_blocking, spawn_local, yield_now,
};
use tokio_with_wasm::time::{Duration, sleep, timeout};
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn spawn_runs_without_being_awaited() {
  let flag = Rc::new(Cell::new(false));
  let cloned = flag.clone();
  let _detached = spawn(async move { cloned.set(true) });
  // The task runs on the same event loop, so yielding lets it proceed.
  yield_now().await;
  assert!(flag.get());
}

#[wasm_bindgen_test]
async fn spawn_accepts_non_send_futures() -> Result<(), JoinError> {
  let handle = spawn(async {
    let rc = Rc::new(5);
    // The `Rc` lives across an await point.
    yield_now().await;
    *rc
  });
  assert_eq!(handle.await?, 5);
  Ok(())
}

#[wasm_bindgen_test]
async fn abort_cancels_a_pending_task() {
  let handle = spawn(std::future::pending::<()>());
  let abort_handle = handle.abort_handle();
  let task_id = handle.id();
  assert_eq!(abort_handle.id(), task_id);
  // The task is waiting by now, with nothing left to wake it but the abort.
  yield_now().await;
  assert!(!abort_handle.is_finished());
  abort_handle.abort();
  let error = timeout(Duration::from_secs(1), handle).await.unwrap();
  let error = error.unwrap_err();
  assert!(error.is_cancelled() && !error.is_panic());
  assert_eq!(error.id(), task_id);
  assert_eq!(error.to_string(), "task was cancelled");
  assert!(abort_handle.is_finished());
}

#[wasm_bindgen_test]
async fn abort_after_completion_keeps_the_output() -> Result<(), JoinError> {
  let handle = spawn(async { 7 });
  let abort_handle = handle.abort_handle();
  assert!(!abort_handle.is_finished());
  sleep(Duration::from_millis(50)).await;
  assert!(abort_handle.is_finished());
  handle.abort();
  handle.abort(); // A second abort must be harmless too.
  assert_eq!(handle.await?, 7);
  Ok(())
}

#[wasm_bindgen_test]
async fn tasks_interleave_on_the_event_loop() -> Result<(), JoinError> {
  let log = Rc::new(RefCell::new(Vec::new()));
  let mut handles = Vec::new();
  for id in 0..3 {
    let log = log.clone();
    handles.push(spawn(async move {
      for _ in 0..2 {
        log.borrow_mut().push(id);
        yield_now().await;
      }
    }));
  }
  for handle in handles {
    handle.await?;
  }
  // Every task yielded, so no task finished both
  // rounds before the others started.
  let log = log.borrow();
  assert_eq!(log.len(), 6);
  assert_eq!(&log[..3], &[0, 1, 2]);
  Ok(())
}

#[wasm_bindgen_test]
async fn a_task_observes_its_own_id() -> Result<(), JoinError> {
  let handle = spawn(async { tokio::task::id() });
  let task_id = handle.id();
  assert_eq!(handle.await?, task_id);
  Ok(())
}

#[wasm_bindgen_test]
async fn a_blocking_task_observes_its_own_id() -> Result<(), JoinError> {
  let handle = spawn_blocking(tokio::task::try_id);
  let task_id = handle.id();
  assert_ne!(spawn_blocking(|| {}).id(), task_id);
  assert_eq!(handle.await?, Some(task_id));
  Ok(())
}

#[cfg(all(tokio_unstable, feature = "tracing"))]
#[wasm_bindgen_test]
async fn the_builder_spawns_every_kind_of_task() -> Result<(), JoinError> {
  use tokio_with_wasm::task::Builder;
  let named = Builder::new().name("answer").spawn(async { 6 * 7 });
  assert_eq!(named.unwrap().await?, 42);
  let local = Builder::new().spawn_local(async { *Rc::new(5) });
  assert_eq!(local.unwrap().await?, 5);
  let blocking = Builder::new().spawn_blocking(|| 3);
  assert_eq!(blocking.unwrap().await?, 3);
  Ok(())
}

#[wasm_bindgen_test]
async fn consume_budget_lets_other_tasks_run() {
  let ran = Rc::new(Cell::new(false));
  let flag = ran.clone();
  spawn_local(async move { flag.set(true) });
  // One budget's worth of calls has to yield once.
  for _ in 0..128 {
    consume_budget().await;
  }
  assert!(ran.get(), "the budget never ran out");
}

#[wasm_bindgen_test]
async fn unconstrained_futures_never_yield() {
  let ran = Rc::new(Cell::new(false));
  let flag = ran.clone();
  spawn_local(async move { flag.set(true) });
  let output = unconstrained(async {
    for _ in 0..1000 {
      consume_budget().await;
    }
    42
  })
  .await;
  assert_eq!(output, 42);
  assert!(!ran.get(), "an unconstrained future yielded");
}

tokio::task_local! {
  static NUMBER: u32;
}

#[wasm_bindgen_test]
async fn a_spawned_task_gets_its_own_scope() -> Result<(), JoinError> {
  // `task_local!` statics have the re-exported `LocalKey` type.
  let key: &'static LocalKey<u32> = &NUMBER;
  assert_eq!(spawn(key.scope(5, async { NUMBER.get() })).await?, 5);
  Ok(())
}
