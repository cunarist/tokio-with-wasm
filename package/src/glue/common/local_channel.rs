use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

/// Creates an unbounded channel, returning the sender and receiver.
/// This channel is not `Send`, which means it cannot be sent across threads.
/// The sender and receiver are not cloneable.
pub fn local_channel<T>() -> (LocalSender<T>, LocalReceiver<T>) {
  let shared = Rc::new(RefCell::new(ChannelCore {
    queue: VecDeque::new(),
    waker: None,
    closed: false,
  }));
  let sender = LocalSender {
    shared: shared.clone(),
  };
  let receiver = LocalReceiver { shared };
  (sender, receiver)
}

struct ChannelCore<T> {
  queue: VecDeque<T>,
  /// Waker for the task currently waiting on a message.
  waker: Option<Waker>,
  /// Indicates that the channel is closed.
  closed: bool,
}

pub struct LocalSender<T> {
  shared: Rc<RefCell<ChannelCore<T>>>,
}

/// The sender side of an unbounded channel.
impl<T> LocalSender<T> {
  /// Attempts to send an item into the channel.
  pub fn send(&self, item: T) {
    let mut shared = self.shared.borrow_mut();
    if shared.closed {
      return;
    }
    shared.queue.push_back(item);
    if let Some(waker) = shared.waker.take() {
      waker.wake();
    }
  }
}

impl<T> Drop for LocalSender<T> {
  fn drop(&mut self) {
    let mut shared = self.shared.borrow_mut();
    // When the sender is dropped,
    // mark the channel as closed and wake the receiver.
    shared.closed = true;
    if let Some(waker) = shared.waker.take() {
      waker.wake();
    }
  }
}

/// The receiver side of an unbounded channel.
pub struct LocalReceiver<T> {
  shared: Rc<RefCell<ChannelCore<T>>>,
}

impl<T> LocalReceiver<T> {
  /// Polls the channel for the next available message.
  fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
    let mut shared = self.shared.borrow_mut();
    if let Some(item) = shared.queue.pop_front() {
      Poll::Ready(Some(item))
    } else if shared.closed {
      // No more messages will ever arrive.
      Poll::Ready(None)
    } else {
      // No item available; store the waker to be notified.
      shared.waker = Some(cx.waker().clone());
      Poll::Pending
    }
  }

  /// Returns a future that resolves to the next available message.
  pub fn next(&mut self) -> ChannelNext<'_, T> {
    ChannelNext { receiver: self }
  }
}

/// A future that resolves to the next item received.
pub struct ChannelNext<'a, T> {
  receiver: &'a mut LocalReceiver<T>,
}

impl<T> Future for ChannelNext<'_, T> {
  type Output = Option<T>;
  fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
    // Delegate to the receiver’s poll_next method.
    self.get_mut().receiver.poll_next(cx)
  }
}

impl<T> Drop for LocalReceiver<T> {
  fn drop(&mut self) {
    let mut shared = self.shared.borrow_mut();
    // Mark the channel as closed when the receiver is dropped.
    shared.closed = true;
  }
}

#[cfg(test)]
mod tests {
  use super::super::test_util::CountingWaker;
  use super::*;
  use wasm_bindgen_test::wasm_bindgen_test;

  /// Polls the receiver once without a real runtime.
  fn poll_once<T>(
    receiver: &mut LocalReceiver<T>,
    waker: &Waker,
  ) -> Poll<Option<T>> {
    let mut cx = Context::from_waker(waker);
    Pin::new(&mut receiver.next()).poll(&mut cx)
  }

  #[wasm_bindgen_test]
  fn items_arrive_in_sending_order() {
    let (sender, mut receiver) = local_channel();
    sender.send(1);
    sender.send(2);
    sender.send(3);
    let counter = CountingWaker::new();
    let waker = counter.waker();
    assert_eq!(poll_once(&mut receiver, &waker), Poll::Ready(Some(1)));
    assert_eq!(poll_once(&mut receiver, &waker), Poll::Ready(Some(2)));
    assert_eq!(poll_once(&mut receiver, &waker), Poll::Ready(Some(3)));
    assert_eq!(poll_once(&mut receiver, &waker), Poll::Pending);
  }

  #[wasm_bindgen_test]
  fn send_wakes_the_waiting_receiver() {
    let (sender, mut receiver) = local_channel();
    let counter = CountingWaker::new();
    let waker = counter.waker();

    assert_eq!(poll_once(&mut receiver, &waker), Poll::Pending);
    assert_eq!(counter.count(), 0);

    sender.send(9);
    assert_eq!(counter.count(), 1);
    assert_eq!(poll_once(&mut receiver, &waker), Poll::Ready(Some(9)));
  }

  #[wasm_bindgen_test]
  fn queued_items_survive_the_sender() {
    let (sender, mut receiver) = local_channel();
    sender.send(1);
    sender.send(2);
    drop(sender);
    let counter = CountingWaker::new();
    let waker = counter.waker();
    // The queue drains first; only then does the channel report closure.
    assert_eq!(poll_once(&mut receiver, &waker), Poll::Ready(Some(1)));
    assert_eq!(poll_once(&mut receiver, &waker), Poll::Ready(Some(2)));
    assert_eq!(poll_once(&mut receiver, &waker), Poll::Ready(None));
  }

  #[wasm_bindgen_test]
  fn dropping_the_sender_wakes_the_receiver() {
    let (sender, mut receiver) = local_channel::<i32>();
    let counter = CountingWaker::new();
    let waker = counter.waker();

    assert_eq!(poll_once(&mut receiver, &waker), Poll::Pending);
    drop(sender);
    assert_eq!(counter.count(), 1);
    assert_eq!(poll_once(&mut receiver, &waker), Poll::Ready(None));
  }

  #[wasm_bindgen_test]
  fn sending_into_a_dropped_receiver_is_harmless() {
    let (sender, receiver) = local_channel();
    drop(receiver);
    sender.send(1);
  }
}
