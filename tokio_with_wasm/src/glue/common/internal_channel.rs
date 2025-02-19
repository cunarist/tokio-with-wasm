use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

/// Creates an unbounded channel, returning the sender and receiver.
/// The sender and receiver are not cloneable.
pub fn internal_channel<T>() -> (InternalSender<T>, InternalReceiver<T>) {
    let shared = Arc::new(Mutex::new(ChannelCore {
        queue: VecDeque::new(),
        waker: None,
        closed: false,
    }));
    let sender = InternalSender {
        shared: shared.clone(),
    };
    let receiver = InternalReceiver { shared };
    (sender, receiver)
}

struct ChannelCore<T> {
    queue: VecDeque<T>,
    /// Waker for the task currently waiting on a message.
    waker: Option<Waker>,
    /// Indicates that the channel is closed.
    closed: bool,
}

/// The sender side of an unbounded channel.
pub struct InternalSender<T> {
    shared: Arc<Mutex<ChannelCore<T>>>,
}

impl<T> InternalSender<T> {
    /// Attempts to send an item into the channel.
    /// Returns an error if the receiver has been dropped.
    pub fn send(&self, item: T) {
        let mut shared = self.shared.lock().unwrap();
        if shared.closed {
            return;
        }
        shared.queue.push_back(item);
        if let Some(waker) = shared.waker.take() {
            waker.wake();
        }
    }
}

impl<T> Drop for InternalSender<T> {
    fn drop(&mut self) {
        let mut shared = self.shared.lock().unwrap();
        // When the sender is dropped,
        // mark the channel as closed and wake the receiver.
        shared.closed = true;
        if let Some(waker) = shared.waker.take() {
            waker.wake();
        }
    }
}

/// The receiver side of an unbounded channel.
pub struct InternalReceiver<T> {
    shared: Arc<Mutex<ChannelCore<T>>>,
}

impl<T> InternalReceiver<T> {
    /// Polls the channel for the next available message.
    fn poll_next(&mut self, cx: &mut Context<'_>) -> Poll<Option<T>> {
        let mut shared = self.shared.lock().unwrap();
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
    receiver: &'a mut InternalReceiver<T>,
}

impl<T> Future for ChannelNext<'_, T> {
    type Output = Option<T>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Delegate to the receiver’s poll_next method.
        self.get_mut().receiver.poll_next(cx)
    }
}

impl<T> Drop for InternalReceiver<T> {
    fn drop(&mut self) {
        let mut shared = self.shared.lock().unwrap();
        // Mark the channel as closed when the receiver is dropped.
        shared.closed = true;
    }
}
