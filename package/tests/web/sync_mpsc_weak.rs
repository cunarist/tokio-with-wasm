use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use tokio::sync::mpsc::{self, channel, unbounded_channel};
use tokio::sync::oneshot;
use tokio_with_wasm::alias as tokio;
use wasm_bindgen_test::wasm_bindgen_test;

static NUM_DROPPED: AtomicUsize = AtomicUsize::new(0);

struct Msg;

impl Drop for Msg {
  fn drop(&mut self) {
    NUM_DROPPED.fetch_add(1, SeqCst);
  }
}

static NUM_DROPPED_UNBOUNDED: AtomicUsize = AtomicUsize::new(0);

struct MsgUnbounded;

impl Drop for MsgUnbounded {
  fn drop(&mut self) {
    NUM_DROPPED_UNBOUNDED.fetch_add(1, SeqCst);
  }
}

#[wasm_bindgen_test]
async fn weak_sender() {
  let (tx, mut rx) = channel(11);

  let tx_weak = tokio::spawn(async move {
    let tx_weak = tx.clone().downgrade();
    for i in 0..10 {
      if tx.send(i).await.is_err() {
        return None;
      }
    }
    let tx2 = tx_weak.upgrade().unwrap();
    let _ = tx2.send(20).await;
    Some(tx2.downgrade())
  })
  .await
  .unwrap();

  for i in 0..12 {
    match rx.recv().await {
      Some(msg) => {
        if i == 10 {
          assert_eq!(msg, 20);
        }
      }
      None => {
        assert_eq!(i, 11);
        break;
      }
    }
  }

  assert!(tx_weak.unwrap().upgrade().is_none());
}

#[wasm_bindgen_test]
async fn actor_weak_sender() {
  struct MyActor {
    receiver: mpsc::Receiver<ActorMessage>,
    sender: mpsc::WeakSender<ActorMessage>,
    next_id: u32,
    received_self_msg: bool,
  }

  enum ActorMessage {
    GetUniqueId { respond_to: oneshot::Sender<u32> },
    SelfMessage,
  }

  impl MyActor {
    fn handle_message(&mut self, msg: ActorMessage) {
      match msg {
        ActorMessage::GetUniqueId { respond_to } => {
          self.next_id += 1;
          let _ = respond_to.send(self.next_id);
        }
        ActorMessage::SelfMessage => self.received_self_msg = true,
      }
    }

    async fn send_message_to_self(&mut self) {
      let sender = self.sender.clone();
      if let Some(sender) = sender.upgrade() {
        let _ = sender.send(ActorMessage::SelfMessage).await;
        self.sender = sender.downgrade();
      }
    }

    async fn run(&mut self) {
      let mut i = 0;
      while let Some(msg) = self.receiver.recv().await {
        self.handle_message(msg);
        if i == 0 {
          self.send_message_to_self().await;
        }
        i += 1;
      }
      assert!(self.received_self_msg);
    }
  }

  let (sender, receiver) = mpsc::channel(8);
  let mut actor = MyActor {
    receiver,
    sender: sender.clone().downgrade(),
    next_id: 0,
    received_self_msg: false,
  };
  let actor_handle = tokio::spawn(async move { actor.run().await });

  let (respond_to, response) = oneshot::channel();
  sender
    .send(ActorMessage::GetUniqueId { respond_to })
    .await
    .unwrap();
  assert_eq!(response.await.unwrap(), 1);
  drop(sender);

  actor_handle.await.unwrap();
}

#[wasm_bindgen_test]
async fn msgs_dropped_on_rx_drop() {
  let (tx, mut rx) = mpsc::channel(3);

  tx.send(Msg).await.unwrap();
  tx.send(Msg).await.unwrap();
  let sent_fut = tx.send(Msg);

  let _ = rx.recv().await.unwrap();
  let _ = rx.recv().await.unwrap();
  sent_fut.await.unwrap();

  drop(rx);
  assert_eq!(NUM_DROPPED.load(SeqCst), 3);

  assert!(tx.send(Msg).await.is_err());
  assert_eq!(NUM_DROPPED.load(SeqCst), 4);
}

#[wasm_bindgen_test]
fn downgrade_upgrade_sender_success() {
  let (tx, _rx) = mpsc::channel::<i32>(1);
  let weak_tx = tx.downgrade();
  assert!(weak_tx.upgrade().is_some());
}

#[wasm_bindgen_test]
fn downgrade_upgrade_sender_failure() {
  let (tx, _rx) = mpsc::channel::<i32>(1);
  let weak_tx = tx.downgrade();
  drop(tx);
  assert!(weak_tx.upgrade().is_none());
}

#[wasm_bindgen_test]
fn downgrade_drop_upgrade() {
  let (tx, _rx) = mpsc::channel::<i32>(1);
  let weak_tx = tx.clone().downgrade();
  drop(tx);
  assert!(weak_tx.upgrade().is_none());
}

#[wasm_bindgen_test]
async fn downgrade_get_permit_upgrade_no_senders() {
  let (tx, _rx) = mpsc::channel::<i32>(1);
  let weak_tx = tx.downgrade();
  let _permit = tx.reserve_owned().await.unwrap();
  assert!(weak_tx.upgrade().is_some());
}

#[wasm_bindgen_test]
async fn downgrade_upgrade_get_permit_no_senders() {
  let (tx, _rx) = mpsc::channel::<i32>(1);
  let tx2 = tx.clone();
  let _permit = tx.reserve_owned().await.unwrap();
  let weak_tx = tx2.downgrade();
  drop(tx2);
  assert!(weak_tx.upgrade().is_some());
}

#[wasm_bindgen_test]
fn tx_count_weak_sender() {
  let (tx, _rx) = mpsc::channel::<i32>(1);
  let tx_weak = tx.downgrade();
  let tx_weak2 = tx.downgrade();
  drop(tx);
  assert!(tx_weak.upgrade().is_none() && tx_weak2.upgrade().is_none());
}

#[wasm_bindgen_test]
async fn weak_unbounded_sender() {
  let (tx, mut rx) = unbounded_channel();

  let tx_weak = tokio::spawn(async move {
    let tx_weak = tx.clone().downgrade();
    for i in 0..10 {
      if tx.send(i).is_err() {
        return None;
      }
    }
    let tx2 = tx_weak.upgrade().unwrap();
    let _ = tx2.send(20);
    Some(tx2.downgrade())
  })
  .await
  .unwrap();

  for i in 0..12 {
    match rx.recv().await {
      Some(msg) => {
        if i == 10 {
          assert_eq!(msg, 20);
        }
      }
      None => {
        assert_eq!(i, 11);
        break;
      }
    }
  }

  assert!(tx_weak.unwrap().upgrade().is_none());
}

#[wasm_bindgen_test]
async fn actor_weak_unbounded_sender() {
  struct MyActor {
    receiver: mpsc::UnboundedReceiver<ActorMessage>,
    sender: mpsc::WeakUnboundedSender<ActorMessage>,
    next_id: u32,
    received_self_msg: bool,
  }

  enum ActorMessage {
    GetUniqueId { respond_to: oneshot::Sender<u32> },
    SelfMessage,
  }

  impl MyActor {
    fn handle_message(&mut self, msg: ActorMessage) {
      match msg {
        ActorMessage::GetUniqueId { respond_to } => {
          self.next_id += 1;
          let _ = respond_to.send(self.next_id);
        }
        ActorMessage::SelfMessage => self.received_self_msg = true,
      }
    }

    fn send_message_to_self(&mut self) {
      let sender = self.sender.clone();
      if let Some(sender) = sender.upgrade() {
        let _ = sender.send(ActorMessage::SelfMessage);
        self.sender = sender.downgrade();
      }
    }

    async fn run(&mut self) {
      let mut i = 0;
      while let Some(msg) = self.receiver.recv().await {
        self.handle_message(msg);
        if i == 0 {
          self.send_message_to_self();
        }
        i += 1;
      }
      assert!(self.received_self_msg);
    }
  }

  let (sender, receiver) = mpsc::unbounded_channel();
  let mut actor = MyActor {
    receiver,
    sender: sender.clone().downgrade(),
    next_id: 0,
    received_self_msg: false,
  };
  let actor_handle = tokio::spawn(async move { actor.run().await });

  let (respond_to, response) = oneshot::channel();
  sender
    .send(ActorMessage::GetUniqueId { respond_to })
    .unwrap();
  assert_eq!(response.await.unwrap(), 1);
  drop(sender);

  actor_handle.await.unwrap();
}

#[wasm_bindgen_test]
async fn msgs_dropped_on_unbounded_rx_drop() {
  let (tx, mut rx) = mpsc::unbounded_channel();

  tx.send(MsgUnbounded).unwrap();
  tx.send(MsgUnbounded).unwrap();
  tx.send(MsgUnbounded).unwrap();

  let _ = rx.recv().await.unwrap();
  let _ = rx.recv().await.unwrap();

  drop(rx);
  assert_eq!(NUM_DROPPED_UNBOUNDED.load(SeqCst), 3);

  assert!(tx.send(MsgUnbounded).is_err());
  assert_eq!(NUM_DROPPED_UNBOUNDED.load(SeqCst), 4);
}

#[wasm_bindgen_test]
fn downgrade_upgrade_unbounded_sender_success() {
  let (tx, _rx) = mpsc::unbounded_channel::<i32>();
  let weak_tx = tx.downgrade();
  assert!(weak_tx.upgrade().is_some());
}

#[wasm_bindgen_test]
fn downgrade_upgrade_unbounded_sender_failure() {
  let (tx, _rx) = mpsc::unbounded_channel::<i32>();
  let weak_tx = tx.downgrade();
  drop(tx);
  assert!(weak_tx.upgrade().is_none());
}

#[wasm_bindgen_test]
fn downgrade_drop_upgrade_unbounded() {
  let (tx, _rx) = mpsc::unbounded_channel::<i32>();
  let weak_tx = tx.clone().downgrade();
  drop(tx);
  assert!(weak_tx.upgrade().is_none());
}

#[wasm_bindgen_test]
fn tx_count_weak_unbounded_sender() {
  let (tx, _rx) = mpsc::unbounded_channel::<i32>();
  let tx_weak = tx.downgrade();
  let tx_weak2 = tx.downgrade();
  drop(tx);
  assert!(tx_weak.upgrade().is_none() && tx_weak2.upgrade().is_none());
}

#[wasm_bindgen_test]
fn rx_is_closed_when_dropping_all_senders_except_weak_senders() {
  let (tx, rx) = mpsc::channel::<()>(10);
  let _weak_sender = tx.clone().downgrade();
  drop(tx);
  assert!(rx.is_closed());
}

#[wasm_bindgen_test]
fn rx_unbounded_is_closed_when_dropping_all_senders_except_weak_senders() {
  let (tx, rx) = mpsc::unbounded_channel::<()>();
  let _weak_sender = tx.clone().downgrade();
  drop(tx);
  assert!(rx.is_closed());
}

#[wasm_bindgen_test]
fn sender_strong_count_when_cloned() {
  let (tx, rx) = mpsc::channel::<()>(1);
  let tx2 = tx.clone();
  assert_eq!(tx.strong_count(), 2);
  assert_eq!(tx2.strong_count(), 2);
  assert_eq!(rx.sender_strong_count(), 2);
}

#[wasm_bindgen_test]
fn sender_weak_count_when_downgraded() {
  let (tx, _rx) = mpsc::channel::<()>(1);
  let weak = tx.downgrade();
  assert_eq!(tx.weak_count(), 1);
  assert_eq!(weak.weak_count(), 1);
}

#[wasm_bindgen_test]
fn sender_strong_count_when_dropped() {
  let (tx, rx) = mpsc::channel::<()>(1);
  drop(tx.clone());
  assert_eq!(tx.strong_count(), 1);
  assert_eq!(rx.sender_strong_count(), 1);
}

#[wasm_bindgen_test]
fn sender_weak_count_when_dropped() {
  let (tx, rx) = mpsc::channel::<()>(1);
  drop(tx.downgrade());
  assert_eq!(tx.weak_count(), 0);
  assert_eq!(rx.sender_weak_count(), 0);
}

#[wasm_bindgen_test]
fn sender_strong_and_weak_count() {
  let (tx, rx) = mpsc::channel::<()>(1);
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
fn unbounded_sender_strong_count_when_cloned() {
  let (tx, rx) = mpsc::unbounded_channel::<()>();
  let tx2 = tx.clone();
  assert_eq!(tx.strong_count(), 2);
  assert_eq!(tx2.strong_count(), 2);
  assert_eq!(rx.sender_strong_count(), 2);
}

#[wasm_bindgen_test]
fn unbounded_sender_weak_count_when_downgraded() {
  let (tx, rx) = mpsc::unbounded_channel::<()>();
  let weak = tx.downgrade();
  assert_eq!(tx.weak_count(), 1);
  assert_eq!(weak.weak_count(), 1);
  assert_eq!(rx.sender_weak_count(), 1);
}

#[wasm_bindgen_test]
fn unbounded_sender_strong_count_when_dropped() {
  let (tx, rx) = mpsc::unbounded_channel::<()>();
  drop(tx.clone());
  assert_eq!(tx.strong_count(), 1);
  assert_eq!(rx.sender_strong_count(), 1);
}

#[wasm_bindgen_test]
fn unbounded_sender_weak_count_when_dropped() {
  let (tx, rx) = mpsc::unbounded_channel::<()>();
  drop(tx.downgrade());
  assert_eq!(tx.weak_count(), 0);
  assert_eq!(rx.sender_weak_count(), 0);
}

#[wasm_bindgen_test]
fn unbounded_sender_strong_and_weak_count() {
  let (tx, rx) = mpsc::unbounded_channel::<()>();
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
async fn weak_sender_upgrades_in_a_web_worker() {
  let (tx, mut rx) = channel(1);
  let weak = tx.downgrade();
  let worker = tokio::task::spawn_blocking(move || {
    weak.upgrade().unwrap().blocking_send(5).unwrap();
    weak
  });
  assert_eq!(rx.recv().await, Some(5));
  let weak = worker.await.unwrap();
  drop(tx);
  assert!(weak.upgrade().is_none());
}
