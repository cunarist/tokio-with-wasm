#![allow(unused_macros, unused_imports, dead_code)]

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console, js_name = log)]
    pub fn log(s: &str);
    #[wasm_bindgen(js_namespace = Date, js_name = now)]
    pub fn now() -> f64;
    #[wasm_bindgen(js_name = setTimeout)]
    pub fn set_timeout(callback: &js_sys::Function, milliseconds: f64);
    #[wasm_bindgen(js_name = queueMicrotask)]
    pub fn queue_microtask(callback: &js_sys::Function);
}

macro_rules! console_log {
    ($($t:tt)*) => (log(&format_args!($($t)*).to_string()))
}
pub(crate) use console_log;

pub type Result<T> = std::result::Result<T, JsValue>;

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
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if let Poll::Ready(output) = self.future_a.as_mut().poll(cx) {
            return Poll::Ready(output);
        }
        if let Poll::Ready(output) = self.future_b.as_mut().poll(cx) {
            return Poll::Ready(output);
        }
        Poll::Pending
    }
}
