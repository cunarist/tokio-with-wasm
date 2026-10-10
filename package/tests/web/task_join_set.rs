use std::rc::Rc;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::task::JoinSet;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

fn spawn_pending_tasks(
  set: &mut JoinSet<()>,
  n: usize,
) -> Vec<oneshot::Receiver<()>> {
  (0..n)
    .map(|_| {
      let (tx, rx) = oneshot::channel::<()>();
      set.spawn(async move {
        std::future::pending::<()>().await;
        drop(tx);
      });
      rx
    })
    .collect()
}

async fn await_receivers_and_assert(receivers: Vec<oneshot::Receiver<()>>) {
  for rx in receivers {
    assert!(rx.await.is_err());
  }
}

#[wasm_bindgen_test]
async fn test_with_sleep() {
  let mut set = JoinSet::new();
  for i in 0..10 {
    set.spawn(async move { i });
    assert_eq!(set.len(), 1 + i);
  }
  set.detach_all();
  assert_eq!(set.len(), 0);
  assert!(set.join_next().await.is_none());

  for i in 0..10 {
    set.spawn(async move {
      tokio::time::sleep(Duration::from_millis(i as u64)).await;
      i
    });
    assert_eq!(set.len(), 1 + i);
  }
  let mut seen = [false; 10];
  while let Some(res) = set.join_next().await.transpose().unwrap() {
    seen[res] = true;
  }
  assert!(seen.iter().all(|&s| s));
  assert!(set.join_next().await.is_none());
}

#[wasm_bindgen_test]
async fn test_abort_on_drop() {
  let mut set = JoinSet::new();
  let receivers = spawn_pending_tasks(&mut set, 16);
  drop(set);
  await_receivers_and_assert(receivers).await;
}

#[wasm_bindgen_test]
async fn alternating() {
  let mut set = JoinSet::new();
  set.spawn(async {});
  set.spawn(async {});
  assert_eq!(set.len(), 2);
  for _ in 0..16 {
    let () = set.join_next().await.unwrap().unwrap();
    assert_eq!(set.len(), 1);
    set.spawn(async {});
    assert_eq!(set.len(), 2);
  }
}

#[wasm_bindgen_test]
async fn abort_tasks() {
  let mut set = JoinSet::new();
  for i in 0..16 {
    let abort = set.spawn(async move {
      tokio::time::sleep(Duration::from_millis(i)).await;
      i
    });
    if i % 2 != 0 {
      abort.abort();
    }
  }
  let (mut num_canceled, mut num_completed) = (0, 0);
  while let Some(res) = set.join_next().await {
    match res {
      Ok(i) => {
        assert_eq!(i % 2, 0);
        num_completed += 1;
      }
      Err(e) => {
        assert!(e.is_cancelled());
        num_canceled += 1;
      }
    }
  }
  assert_eq!(num_canceled, 8);
  assert_eq!(num_completed, 8);
}

#[wasm_bindgen_test]
async fn join_all() {
  let mut set = JoinSet::new();
  for _ in 0..5 {
    set.spawn(async { 1 });
  }
  assert_eq!(set.join_all().await, [1; 5]);
}

#[wasm_bindgen_test]
async fn abort_all() {
  let mut set: JoinSet<()> = JoinSet::new();
  for _ in 0..5 {
    set.spawn(std::future::pending());
  }
  for _ in 0..5 {
    set.spawn(tokio::time::sleep(Duration::from_millis(1)));
  }
  tokio::time::sleep(Duration::from_millis(10)).await;

  set.abort_all();
  assert_eq!(set.len(), 10);
  let mut count = 0;
  while let Some(res) = set.join_next().await {
    if let Err(err) = res {
      assert!(err.is_cancelled());
    }
    count += 1;
  }
  assert_eq!(count, 10);
  assert_eq!(set.len(), 0);
}

#[wasm_bindgen_test]
async fn try_join_next() {
  const TASK_NUM: u32 = 1000;
  let (send, recv) = tokio::sync::watch::channel(());
  let mut set = JoinSet::new();
  for _ in 0..TASK_NUM {
    let mut recv = recv.clone();
    set.spawn(async move { recv.changed().await.unwrap() });
  }
  drop(recv);
  assert!(set.try_join_next().is_none());

  send.send_replace(());
  send.closed().await;
  let mut count = 0;
  while let Some(res) = set.try_join_next() {
    res.unwrap();
    count += 1;
  }
  assert_eq!(count, TASK_NUM);
}

#[wasm_bindgen_test]
async fn spawn_then_join_next() {
  let mut set = JoinSet::new();
  for i in 0..8 {
    let rc = Rc::new(i);
    set.spawn(async move { *rc });
  }
  assert!(set.try_join_next().is_none());
  let mut seen = [false; 8];
  while let Some(res) = set.join_next().await {
    seen[res.unwrap()] = true;
  }
  assert!(seen.iter().all(|&s| s));
}

#[wasm_bindgen_test]
async fn spawn_then_shutdown() {
  let mut set = JoinSet::new();
  let receivers = spawn_pending_tasks(&mut set, 8);
  assert!(set.try_join_next().is_none());
  set.shutdown().await;
  assert!(set.is_empty());
  await_receivers_and_assert(receivers).await;
}

#[wasm_bindgen_test]
async fn spawn_blocking() {
  let mut set = JoinSet::new();
  for i in 0..4 {
    set.spawn_blocking(move || i);
  }
  let mut out = set.join_all().await;
  out.sort_unstable();
  assert_eq!(out, [0, 1, 2, 3]);
}
