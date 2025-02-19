#![allow(unused_macros, unused_imports, dead_code)]

use js_sys::Function;
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
