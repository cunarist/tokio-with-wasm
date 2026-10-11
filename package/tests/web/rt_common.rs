use crate::support::assert_elapsed;
use std::cell::Cell;
use std::rc::Rc;
use std::thread;
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn spawn_one_bg() {
  let (tx, rx) = oneshot::channel();
  tokio::spawn(async move {
    tx.send("ZOMG").unwrap();
  });
  assert_eq!(rx.await.unwrap(), "ZOMG");
}

#[wasm_bindgen_test]
async fn spawn_one_join() {
  let (tx, rx) = oneshot::channel();
  let handle = tokio::spawn(async move {
    tx.send("ZOMG").unwrap();
    "DONE"
  });
  assert_eq!(rx.await.unwrap(), "ZOMG");
  assert_eq!(handle.await.unwrap(), "DONE");
}

#[wasm_bindgen_test]
async fn spawn_two() {
  let (tx1, rx1) = oneshot::channel();
  let (tx2, rx2) = oneshot::channel();
  tokio::spawn(async move {
    tx1.send("ZOMG").unwrap();
  });
  tokio::spawn(async move {
    tx2.send(rx1.await.unwrap()).unwrap();
  });
  assert_eq!(rx2.await.unwrap(), "ZOMG");
}

#[wasm_bindgen_test]
async fn spawn_many_from_root() {
  const ITER: usize = 200;
  let (done_tx, mut done_rx) = mpsc::unbounded_channel();
  let txs: Vec<_> = (0..ITER)
    .map(|i| {
      let (tx, rx) = oneshot::channel();
      let done_tx = done_tx.clone();
      tokio::spawn(async move {
        let msg = rx.await.unwrap();
        assert_eq!(i, msg);
        done_tx.send(msg).unwrap();
      });
      tx
    })
    .collect();
  drop(done_tx);

  tokio::task::spawn_blocking(move || {
    for (i, tx) in txs.into_iter().enumerate() {
      tx.send(i).unwrap();
    }
  });

  let mut out = vec![];
  while let Some(i) = done_rx.recv().await {
    out.push(i);
  }
  out.sort_unstable();
  assert!(out.into_iter().eq(0..ITER));
}

#[wasm_bindgen_test]
async fn spawn_many_from_task() {
  const ITER: usize = 500;
  let out = tokio::spawn(async move {
    let (done_tx, mut done_rx) = mpsc::unbounded_channel();
    let txs: Vec<_> = (0..ITER)
      .map(|i| {
        let (tx, rx) = oneshot::channel();
        let done_tx = done_tx.clone();
        tokio::spawn(async move {
          let msg = rx.await.unwrap();
          assert_eq!(i, msg);
          done_tx.send(msg).unwrap();
        });
        tx
      })
      .collect();
    drop(done_tx);

    tokio::task::spawn_blocking(move || {
      for (i, tx) in txs.into_iter().enumerate() {
        tx.send(i).unwrap();
      }
    });

    let mut out = vec![];
    while let Some(i) = done_rx.recv().await {
      out.push(i);
    }
    out.sort_unstable();
    out
  })
  .await
  .unwrap();
  assert!(out.into_iter().eq(0..ITER));
}

#[wasm_bindgen_test]
async fn spawn_await_chain() {
  let out =
    tokio::spawn(async { tokio::spawn(async { "hello" }).await.unwrap() })
      .await
      .unwrap();
  assert_eq!(out, "hello");
}

#[wasm_bindgen_test]
async fn sleep_in_spawn() {
  let now = js_sys::Date::now();
  let (tx, rx) = oneshot::channel();
  tokio::spawn(async move {
    tokio::time::sleep(Duration::from_millis(50)).await;
    tx.send(()).unwrap();
  });
  rx.await.unwrap();
  assert_elapsed(now, 50);
}

#[wasm_bindgen_test]
async fn spawn_not_send() {
  let rc = Rc::new("hello");
  let out = tokio::spawn(async move {
    tokio::task::yield_now().await;
    *rc
  });
  assert_eq!(out.await.unwrap(), "hello");
}

#[wasm_bindgen_test]
async fn complete_task_under_load() {
  let spin = tokio::spawn(async {
    loop {
      tokio::task::yield_now().await;
    }
  });
  let (tx1, rx1) = oneshot::channel();
  let (tx2, rx2) = oneshot::channel();
  tokio::task::spawn_blocking(move || {
    thread::sleep(Duration::from_millis(50));
    tx1.send(()).unwrap();
  });
  tokio::spawn(async move {
    rx1.await.unwrap();
    tx2.send(()).unwrap();
  });
  rx2.await.unwrap();
  spin.abort();
}

// Web-only: a task whose futures keep waking each other must still let
// other tasks run.
#[wasm_bindgen_test]
async fn busy_task_lets_others_run() {
  // Bounds the busy task, so that starvation fails instead of hanging.
  const MAX_ROUNDS: usize = 100_000;
  let running = Rc::new(Cell::new(true));
  let (tx1, mut rx1) = mpsc::unbounded_channel();
  let (tx2, mut rx2) = mpsc::unbounded_channel();
  let ping = {
    let running = running.clone();
    async move {
      let mut rounds = 0;
      while running.get() && rounds < MAX_ROUNDS {
        tx1.send(()).unwrap();
        rx2.recv().await.unwrap();
        rounds += 1;
      }
      rounds
    }
  };
  let pong = async move {
    while rx1.recv().await.is_some() {
      tx2.send(()).unwrap();
    }
  };
  let busy = tokio::spawn(async move { tokio::join!(ping, pong).0 });
  tokio::spawn(async {}).await.unwrap();
  running.set(false);
  assert!(busy.await.unwrap() < MAX_ROUNDS, "starved");
}
