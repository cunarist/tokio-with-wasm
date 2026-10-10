use tokio::sync::broadcast::{self, channel};
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
async fn weak_sender() {
  let (tx, mut rx) = channel(11);

  let tx_weak = tokio::spawn(async move {
    let tx_weak = tx.clone().downgrade();

    for i in 0..10 {
      if tx.send(i).is_err() {
        return None;
      }
    }

    let tx2 = tx_weak
      .upgrade()
      .expect("expected to be able to upgrade tx_weak");
    let _ = tx2.send(20);
    let tx_weak = tx2.downgrade();

    Some(tx_weak)
  })
  .await
  .unwrap();

  for i in 0..12 {
    let recvd = rx.recv().await;

    match recvd {
      Ok(msg) => {
        if i == 10 {
          assert_eq!(msg, 20);
        }
      }
      Err(_) => {
        assert_eq!(i, 11);
        break;
      }
    }
  }

  let tx_weak = tx_weak.unwrap();
  let upgraded = tx_weak.upgrade();
  assert!(upgraded.is_none());
}

#[wasm_bindgen_test]
fn downgrade_upgrade_sender_failure() {
  let (tx, _rx) = broadcast::channel::<i32>(1);
  let weak_tx = tx.downgrade();
  drop(tx);
  assert!(weak_tx.upgrade().is_none());
}

#[wasm_bindgen_test]
fn downgrade_drop_upgrade() {
  let (tx, _rx) = broadcast::channel::<i32>(1);

  let weak_tx = tx.clone().downgrade();
  drop(tx);
  assert!(weak_tx.upgrade().is_none());
}

#[wasm_bindgen_test]
fn tx_count_weak_sender() {
  let (tx, _rx) = broadcast::channel::<i32>(1);
  let tx_weak = tx.downgrade();
  let tx_weak2 = tx.downgrade();
  assert_eq!(tx.strong_count(), 1);
  assert_eq!(tx.weak_count(), 2);

  drop(tx);

  assert!(tx_weak.upgrade().is_none());
  assert!(tx_weak2.upgrade().is_none());
  assert_eq!(tx_weak.strong_count(), 0);
  assert_eq!(tx_weak.weak_count(), 2);
}

#[wasm_bindgen_test]
async fn rx_is_closed_when_dropping_all_senders_except_weak_senders() {
  let (tx, rx) = broadcast::channel::<()>(10);
  let weak_sender = tx.clone().downgrade();
  drop(tx);
  assert_eq!(weak_sender.strong_count(), 0);
  assert_eq!(weak_sender.weak_count(), 1);
  assert!(rx.is_closed());
}

#[wasm_bindgen_test]
async fn sender_strong_count_when_cloned() {
  let (tx, rx) = broadcast::channel::<()>(1);

  let tx2 = tx.clone();

  assert_eq!(tx.strong_count(), 2);
  assert_eq!(tx2.strong_count(), 2);
  assert_eq!(rx.sender_strong_count(), 2);
}

#[wasm_bindgen_test]
async fn sender_weak_count_when_downgraded() {
  let (tx, _rx) = broadcast::channel::<()>(1);

  let weak = tx.downgrade();

  assert_eq!(tx.weak_count(), 1);
  assert_eq!(weak.weak_count(), 1);
}

#[wasm_bindgen_test]
async fn sender_strong_count_when_dropped() {
  let (tx, rx) = broadcast::channel::<()>(1);

  let tx2 = tx.clone();

  drop(tx2);

  assert_eq!(tx.strong_count(), 1);
  assert_eq!(rx.sender_strong_count(), 1);
}

#[wasm_bindgen_test]
async fn sender_weak_count_when_dropped() {
  let (tx, rx) = broadcast::channel::<()>(1);

  let weak = tx.downgrade();

  drop(weak);

  assert_eq!(tx.weak_count(), 0);
  assert_eq!(rx.sender_weak_count(), 0);
}

#[wasm_bindgen_test]
async fn sender_strong_and_weak_conut() {
  let (tx, rx) = broadcast::channel::<()>(1);

  let tx2 = tx.clone();

  let weak = tx.downgrade();
  let weak2 = tx2.downgrade();

  assert_eq!(tx.strong_count(), 2);
  assert_eq!(tx2.strong_count(), 2);
  assert_eq!(weak.strong_count(), 2);
  assert_eq!(weak2.strong_count(), 2);
  assert_eq!(rx.sender_strong_count(), 2);

  assert_eq!(tx.weak_count(), 2);
  assert_eq!(tx2.weak_count(), 2);
  assert_eq!(weak.weak_count(), 2);
  assert_eq!(weak2.weak_count(), 2);
  assert_eq!(rx.sender_weak_count(), 2);

  drop(tx2);
  drop(weak2);

  assert_eq!(tx.strong_count(), 1);
  assert_eq!(weak.strong_count(), 1);
  assert_eq!(rx.sender_strong_count(), 1);

  assert_eq!(tx.weak_count(), 1);
  assert_eq!(weak.weak_count(), 1);
  assert_eq!(rx.sender_weak_count(), 1);
}

#[wasm_bindgen_test]
async fn weak_sender_upgrades_from_a_web_worker() {
  let (tx, mut rx) = channel(4);
  let weak = tx.downgrade();
  tokio::task::spawn_blocking(move || {
    weak.upgrade().unwrap().send(1).unwrap();
  })
  .await
  .unwrap();
  assert_eq!(rx.recv().await, Ok(1));
  drop(tx);
  assert!(rx.recv().await.is_err());
}
