use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

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

#[cfg(test)]
mod tests {
  use super::super::test_util::CountingWaker;
  use super::*;
  use crate::once_channel;
  use std::future::ready;
  use std::task::Waker;
  use wasm_bindgen_test::wasm_bindgen_test;

  fn poll_once<T>(select: &mut SelectFuture<T>, waker: &Waker) -> Poll<T> {
    let mut cx = Context::from_waker(waker);
    Pin::new(select).poll(&mut cx)
  }

  #[wasm_bindgen_test]
  fn the_first_ready_future_wins() {
    let (_sender, receiver) = once_channel::<i32>();
    let mut select = SelectFuture::new(ready(1), receiver);
    let counter = CountingWaker::new();
    let waker = counter.waker();
    assert_eq!(poll_once(&mut select, &waker), Poll::Ready(1));
  }

  #[wasm_bindgen_test]
  fn the_second_ready_future_wins() {
    let (_sender, receiver) = once_channel::<i32>();
    let mut select = SelectFuture::new(receiver, ready(2));
    let counter = CountingWaker::new();
    let waker = counter.waker();
    assert_eq!(poll_once(&mut select, &waker), Poll::Ready(2));
  }

  #[wasm_bindgen_test]
  fn both_ready_prefers_the_first() {
    let mut select = SelectFuture::new(ready(1), ready(2));
    let counter = CountingWaker::new();
    let waker = counter.waker();
    assert_eq!(poll_once(&mut select, &waker), Poll::Ready(1));
  }

  #[wasm_bindgen_test]
  fn a_late_completion_wakes_and_resolves() {
    let (sender_a, receiver_a) = once_channel::<i32>();
    let (_sender_b, receiver_b) = once_channel::<i32>();
    let mut select = SelectFuture::new(receiver_a, receiver_b);
    let counter = CountingWaker::new();
    let waker = counter.waker();

    assert_eq!(poll_once(&mut select, &waker), Poll::Pending);
    sender_a.send(3);
    assert_eq!(counter.count(), 1);
    assert_eq!(poll_once(&mut select, &waker), Poll::Ready(3));
  }
}
