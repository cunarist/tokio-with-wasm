#![allow(dead_code)]

use js_sys::Function;
use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use wasm_bindgen::prelude::{wasm_bindgen, JsError};
use wasm_bindgen::JsValue;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = error)]
    pub fn error(s: &str);
    #[wasm_bindgen(js_namespace = Date, js_name = now)]
    pub fn now() -> f64;
    #[wasm_bindgen(js_namespace = globalThis, js_name = setTimeout)]
    pub fn set_timeout(callback: &Function, milliseconds: f64);
    #[wasm_bindgen(js_namespace = globalThis, js_name = setInterval)]
    pub fn set_interval(callback: &Function, milliseconds: f64) -> i32;
    #[wasm_bindgen(js_namespace = globalThis, js_name = clearInterval)]
    pub fn clear_interval(id: i32);
}

pub struct SelectFuture<T> {
    future_a: Pin<Box<dyn Future<Output = T>>>,
    future_b: Pin<Box<dyn Future<Output = T>>>,
}

impl<T> SelectFuture<T> {
    pub fn new(
        future_a: impl Future<Output = T> + 'static,
        future_b: impl Future<Output = T> + 'static,
    ) -> Self {
        SelectFuture {
            future_a: Box::pin(future_a),
            future_b: Box::pin(future_b),
        }
    }
}

impl<T> Future for SelectFuture<T> {
    type Output = T;
    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        if let Poll::Ready(output) = self.future_a.as_mut().poll(cx) {
            return Poll::Ready(output);
        }
        if let Poll::Ready(output) = self.future_b.as_mut().poll(cx) {
            return Poll::Ready(output);
        }
        Poll::Pending
    }
}

pub fn once_channel<T>() -> (OnceSender<T>, OnceReceiver<T>) {
    let notified = Arc::new(AtomicBool::new(false));
    let value = Arc::new(Mutex::new(None));
    let waker = Arc::new(Mutex::new(None));

    let sender = OnceSender {
        notified: notified.clone(),
        value: value.clone(),
        waker: waker.clone(),
    };
    let receiver = OnceReceiver {
        notified,
        value,
        waker,
    };

    (sender, receiver)
}

pub struct OnceSender<T> {
    notified: Arc<AtomicBool>,
    value: Arc<Mutex<Option<T>>>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl<T> OnceSender<T> {
    pub fn send(&self, value: T) {
        if let Ok(mut guard) = self.value.lock() {
            guard.replace(value);
            self.notified.store(true, Ordering::SeqCst);
        }
        if let Ok(mut guard) = self.waker.lock() {
            if let Some(waker) = guard.take() {
                waker.wake();
            }
        }
    }
}

pub struct OnceReceiver<T> {
    notified: Arc<AtomicBool>,
    value: Arc<Mutex<Option<T>>>,
    waker: Arc<Mutex<Option<Waker>>>,
}

impl<T> OnceReceiver<T> {
    pub fn is_done(&self) -> bool {
        self.notified.load(Ordering::SeqCst)
    }
}

impl<T> Future for OnceReceiver<T> {
    type Output = T;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.notified.load(Ordering::SeqCst) {
            if let Ok(mut guard) = self.value.lock() {
                if let Some(value) = guard.take() {
                    return Poll::Ready(value);
                }
            }
        }
        if let Ok(mut guard) = self.waker.lock() {
            guard.replace(cx.waker().clone());
        }
        Poll::Pending
    }
}

pub trait LogError {
    fn log_error(&self, code: &str);
}

impl LogError for JsError {
    fn log_error(&self, code: &str) {
        error(&format!(
            "Error `{}` in `tokio_with_wasm`:\n{:?}",
            code, self
        ));
    }
}

impl<T> LogError for Result<T, JsValue> {
    fn log_error(&self, code: &str) {
        if let Err(js_error) = self {
            error(&format!(
                "Error `{}` in `tokio_with_wasm`:\n{:?}",
                code, js_error
            ));
        }
    }
}

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
