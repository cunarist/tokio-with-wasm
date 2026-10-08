//! Browser tests for `task::Builder`, `task::spawn_local`,
//! and the cooperative scheduling helpers.
//! Run with `wasm-pack test --headless --chrome package`.

// The glue code only exists on the web target,
// so this file is empty everywhere else.
#![cfg(all(
  target_family = "wasm",
  target_vendor = "unknown",
  target_os = "unknown"
))]

use std::cell::Cell;
use std::rc::Rc;
use tokio_with_wasm::task::coop::{consume_budget, unconstrained};
use tokio_with_wasm::task::{Builder, JoinError, spawn_local, yield_now};
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn builder_spawns_a_named_task() -> Result<(), JoinError> {
  let Ok(handle) = Builder::new().name("answer").spawn(async { 6 * 7 }) else {
    panic!("the builder failed to spawn");
  };
  assert_eq!(handle.await?, 42);
  Ok(())
}

#[wasm_bindgen_test]
async fn builder_spawns_local_tasks() -> Result<(), JoinError> {
  let Ok(handle) = Builder::new().spawn_local(async {
    // A `!Send` value is fine in a local task.
    let rc = Rc::new(5);
    *rc
  }) else {
    panic!("the builder failed to spawn");
  };
  assert_eq!(handle.await?, 5);
  Ok(())
}

#[wasm_bindgen_test]
async fn builder_spawns_blocking_tasks() -> Result<(), JoinError> {
  let Ok(handle) = Builder::new()
    .name("blocking")
    .spawn_blocking(|| "from a web worker".to_string())
  else {
    panic!("the builder failed to spawn");
  };
  assert_eq!(handle.await?, "from a web worker");
  Ok(())
}

#[wasm_bindgen_test]
async fn spawn_local_accepts_non_send_futures() -> Result<(), JoinError> {
  let handle = spawn_local(async {
    let rc = Rc::new(7);
    // The `Rc` lives across an await point.
    yield_now().await;
    *rc
  });
  assert_eq!(handle.await?, 7);
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
