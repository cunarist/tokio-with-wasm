//! Browser tests for `JoinMap`.
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
use tokio_with_wasm::alias as tokio;
use tokio_with_wasm::task::{JoinError, JoinMap};
use tokio_with_wasm::time::{Duration, sleep};
use wasm_bindgen_test::wasm_bindgen_test;

wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
async fn join_next_returns_every_key_and_output() -> Result<(), JoinError> {
  let mut map = JoinMap::new();
  for i in 0..10 {
    map.spawn(i, async move { i * 2 });
  }
  assert_eq!(map.len(), 10);

  let mut seen = [false; 10];
  while let Some((key, result)) = map.join_next().await {
    assert_eq!(result?, key * 2);
    seen[key] = true;
  }
  assert!(seen.iter().all(|b| *b));
  assert!(map.is_empty());
  Ok(())
}

#[wasm_bindgen_test]
async fn with_hasher_supports_a_custom_hasher() -> Result<(), JoinError> {
  use std::hash::{BuildHasherDefault, DefaultHasher};
  let mut map: JoinMap<&str, i32, BuildHasherDefault<DefaultHasher>> =
    JoinMap::with_hasher(BuildHasherDefault::default());
  map.spawn("a", async { 1 });
  map.spawn("b", async { 2 });
  assert!(map.contains_key("a"));

  let mut outputs = Vec::new();
  while let Some((key, result)) = map.join_next().await {
    outputs.push((key, result?));
  }
  outputs.sort();
  assert_eq!(outputs, vec![("a", 1), ("b", 2)]);
  Ok(())
}

#[wasm_bindgen_test]
async fn spawning_a_known_key_replaces_the_task() -> Result<(), JoinError> {
  let mut map = JoinMap::new();
  map.spawn("key", async {
    sleep(Duration::from_millis(100)).await;
    "first"
  });
  map.spawn("key", async { "second" });
  // The replaced task is gone, not merely cancelled.
  assert_eq!(map.len(), 1);

  let Some((key, result)) = map.join_next().await else {
    panic!("the replacing task never finished");
  };
  assert_eq!(key, "key");
  assert_eq!(result?, "second");
  assert!(map.join_next().await.is_none());
  Ok(())
}

/// Equal by number alone, so that a replacement shows which key it kept.
struct Tagged(u8, &'static str);

impl PartialEq for Tagged {
  fn eq(&self, other: &Self) -> bool {
    self.0 == other.0
  }
}

impl Eq for Tagged {}

impl std::hash::Hash for Tagged {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.0.hash(state);
  }
}

#[wasm_bindgen_test]
async fn replacing_a_finished_task_keeps_the_new_key() -> Result<(), JoinError>
{
  let mut map = JoinMap::new();
  map.spawn(Tagged(1, "old"), async { tokio::task::id() });
  // The first task finishes without being joined.
  sleep(Duration::from_millis(10)).await;
  map.spawn(Tagged(1, "new"), async { tokio::task::id() });

  let Some((key, result)) = map.join_next().await else {
    panic!("the replacing task never finished");
  };
  assert_eq!(key.1, "new");
  assert!(!map.contains_task(&result?));
  assert!(map.join_next().await.is_none());
  Ok(())
}

#[wasm_bindgen_test]
async fn keys_and_contains_key_see_pending_tasks() {
  let mut map = JoinMap::new();
  map.spawn("alive", async {
    sleep(Duration::from_secs(10)).await;
  });

  assert!(map.contains_key("alive"));
  assert!(!map.contains_key("missing"));
  assert_eq!(map.keys().collect::<Vec<_>>(), vec![&"alive"]);
}

#[wasm_bindgen_test]
async fn abort_cancels_only_the_keyed_task() -> Result<(), JoinError> {
  let mut map = JoinMap::new();
  map.spawn("cancelled", async {
    sleep(Duration::from_secs(10)).await;
    1
  });
  map.spawn("kept", async { 2 });

  assert!(map.abort("cancelled"));
  assert!(!map.abort("never-spawned"));

  let mut outputs = Vec::new();
  while let Some((key, result)) = map.join_next().await {
    match result {
      Ok(output) => outputs.push((key, output)),
      Err(error) => assert!(error.is_cancelled() && key == "cancelled"),
    }
  }
  assert_eq!(outputs, vec![("kept", 2)]);
  Ok(())
}

#[wasm_bindgen_test]
async fn try_join_next_sees_only_finished_tasks() -> Result<(), JoinError> {
  let mut map = JoinMap::new();
  map.spawn(7, async {
    sleep(Duration::from_millis(100)).await;
    5
  });
  // Nothing has finished yet.
  assert!(map.try_join_next().is_none());
  sleep(Duration::from_millis(200)).await;

  let Some((key, result)) = map.try_join_next() else {
    panic!("the finished task was not reported");
  };
  assert_eq!(key, 7);
  assert_eq!(result?, 5);
  assert!(map.try_join_next().is_none());
  Ok(())
}

#[wasm_bindgen_test]
async fn spawn_blocking_runs_in_a_worker() -> Result<(), JoinError> {
  let mut map = JoinMap::new();
  for i in 0..2 {
    map.spawn_blocking(i, move || {
      std::thread::sleep(std::time::Duration::from_millis(50));
      i * 3
    });
  }

  let mut outputs = Vec::new();
  while let Some((key, result)) = map.join_next().await {
    outputs.push((key, result?));
  }
  outputs.sort();
  assert_eq!(outputs, vec![(0, 0), (1, 3)]);
  Ok(())
}

#[wasm_bindgen_test]
async fn shutdown_aborts_and_drains() {
  let mut map = JoinMap::new();
  for i in 0..3 {
    map.spawn(i, async {
      sleep(Duration::from_secs(10)).await;
    });
  }
  map.shutdown().await;
  assert!(map.is_empty());
}

#[wasm_bindgen_test]
async fn dropping_the_map_aborts_its_tasks() {
  let flag = Rc::new(Cell::new(false));
  let cloned = flag.clone();
  let map = {
    let mut map = JoinMap::new();
    map.spawn("task", async move {
      sleep(Duration::from_millis(100)).await;
      cloned.set(true);
    });
    map
  };
  drop(map);
  sleep(Duration::from_millis(300)).await;
  assert!(!flag.get(), "the task outlived the dropped `JoinMap`");
}

#[wasm_bindgen_test]
async fn abort_after_finish_yields_the_output() -> Result<(), JoinError> {
  let mut map = JoinMap::new();
  map.spawn("done", async { 9 });
  sleep(Duration::from_millis(50)).await;

  // Aborting a task that already finished does not erase its output.
  assert!(map.abort("done"));
  let Some((key, result)) = map.join_next().await else {
    panic!("the finished task disappeared");
  };
  assert_eq!(key, "done");
  assert_eq!(result?, 9);
  Ok(())
}

#[wasm_bindgen_test]
async fn abort_matching_cancels_only_matching_keys() -> Result<(), JoinError> {
  let mut map = JoinMap::new();
  for i in 1..=2 {
    map.spawn(format!("sess-{i}"), async {
      sleep(Duration::from_secs(10)).await;
      0
    });
  }
  map.spawn("other".to_string(), async { 7 });
  map.abort_matching(|key| key.starts_with("sess-"));

  let mut cancelled = 0;
  let mut outputs = Vec::new();
  while let Some((key, result)) = map.join_next().await {
    match result {
      Ok(output) => outputs.push((key, output)),
      Err(error) => {
        assert!(error.is_cancelled() && key.starts_with("sess-"));
        cancelled += 1;
      }
    }
  }
  assert_eq!(cancelled, 2);
  assert_eq!(outputs, vec![("other".to_string(), 7)]);
  Ok(())
}

#[wasm_bindgen_test]
async fn detach_all_keeps_tasks_running() {
  let flag = Rc::new(Cell::new(false));
  let cloned = flag.clone();
  let mut map = JoinMap::new();
  map.spawn("task", async move {
    sleep(Duration::from_millis(100)).await;
    cloned.set(true);
  });
  map.detach_all();
  drop(map);
  sleep(Duration::from_millis(300)).await;
  assert!(flag.get(), "the detached task was aborted");
}
